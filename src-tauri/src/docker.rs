//! Docker container management via bollard (Unix socket, no CLI dependency).
//!
//! Manages the Open WebUI Docker container lifecycle.

use crate::config::IbexConfig;
use crate::state::DockerStatus;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, PortBinding};
use bollard::Docker;
use futures::StreamExt;
use std::collections::HashMap;

const IMAGE: &str = "ghcr.io/open-webui/open-webui:main";
const CONTAINER_NAME: &str = "open-webui";
const HOST_PORT: &str = "8080";
const CONTAINER_PORT: &str = "8080/tcp";

/// MCP server definition for TOOL_SERVER_CONNECTIONS.
struct McpServer {
    port: u16,
    name: &'static str,
    id: &'static str,
    description: &'static str,
}

/// Build the TOOL_SERVER_CONNECTIONS JSON from configured connectors.
/// Port of install.sh build_mcp_connections().
pub fn build_mcp_connections(config: &IbexConfig) -> String {
    let mut servers: Vec<McpServer> = Vec::new();

    if config.is_slack_configured() {
        servers.push(McpServer {
            port: 3001,
            name: "Slack",
            id: "slack",
            description: "Search messages, read channels, and browse threads",
        });
    }
    if config.is_notion_configured() {
        servers.push(McpServer {
            port: 3002,
            name: "Notion",
            id: "notion",
            description: "Search pages, read content, and query databases",
        });
    }
    if config.is_jira_configured() {
        servers.push(McpServer {
            port: 3003,
            name: "Jira",
            id: "jira",
            description: "Search issues with JQL, read details and comments",
        });
    }
    if config.is_memory_configured() {
        servers.push(McpServer {
            port: 3004,
            name: "Memory",
            id: "memory",
            description: "Read and write persistent memory backed by GitHub",
        });
    }
    if config.is_servicenow_configured() {
        servers.push(McpServer {
            port: 3005,
            name: "ServiceNow",
            id: "servicenow",
            description: "Query tables, get records, and list tables",
        });
    }
    if config.is_salesforce_configured() {
        servers.push(McpServer {
            port: 3006,
            name: "Salesforce",
            id: "salesforce",
            description: "Run SOQL queries, get records, and search objects",
        });
    }

    let entries: Vec<String> = servers
        .iter()
        .map(|s| {
            format!(
                r#"{{"url":"http://host.docker.internal:{}/mcp","path":"","type":"mcp","auth_type":"none","key":"","config":{{"enable":true,"access_grants":[{{"principal_type":"user","principal_id":"*","permission":"read"}}]}},"info":{{"id":"{}","name":"{}","description":"{}"}}}}"#,
                s.port, s.id, s.name, s.description
            )
        })
        .collect();

    format!("[{}]", entries.join(","))
}

/// Connect to Docker daemon.
pub async fn connect() -> Result<Docker, String> {
    Docker::connect_with_socket_defaults()
        .map_err(|e| format!("Failed to connect to Docker: {e}"))
}

/// Check Docker and container status.
pub async fn check_status() -> DockerStatus {
    let docker = match connect().await {
        Ok(d) => d,
        Err(_) => return DockerStatus::NotInstalled,
    };

    // Verify Docker is running
    if docker.ping().await.is_err() {
        return DockerStatus::NotRunning;
    }

    // Check for our container
    let mut filters = HashMap::new();
    filters.insert("name", vec![CONTAINER_NAME]);

    let opts = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    match docker.list_containers(Some(opts)).await {
        Ok(containers) => {
            if let Some(container) = containers.first() {
                match container.state.as_deref() {
                    Some("running") => {
                        // Check if healthy via HTTP
                        match check_webui_health().await {
                            true => DockerStatus::Healthy,
                            false => DockerStatus::ContainerRunning,
                        }
                    }
                    Some("exited") | Some("created") | Some("dead") => {
                        DockerStatus::ContainerStopped
                    }
                    _ => DockerStatus::ContainerStopped,
                }
            } else {
                DockerStatus::ContainerMissing
            }
        }
        Err(_) => DockerStatus::NotRunning,
    }
}

/// Pull the Open WebUI image.
pub async fn pull_image() -> Result<(), String> {
    let docker = connect().await?;

    let opts = CreateImageOptions {
        from_image: IMAGE,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(opts), None, None);

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = info.status {
                    log::info!("Pull: {status}");
                }
            }
            Err(e) => {
                return Err(format!("Image pull failed: {e}"));
            }
        }
    }

    Ok(())
}

/// Create the Open WebUI container with IBEX configuration.
pub async fn create_container(config: &IbexConfig) -> Result<String, String> {
    let docker = connect().await?;

    let mcp_json = build_mcp_connections(config);
    let home = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .display()
        .to_string();

    let volume = format!("{home}/open-webui-data:/app/backend/data");

    let mut port_bindings = HashMap::new();
    port_bindings.insert(
        CONTAINER_PORT.to_string(),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(HOST_PORT.to_string()),
        }]),
    );

    let mut env = vec![format!("TOOL_SERVER_CONNECTIONS={mcp_json}")];

    // Disable auth — IBEX Desktop owns the container, no login needed
    env.push("WEBUI_AUTH=false".to_string());

    // Stable JWT secret so tokens survive container recreation.
    // Without this, Open WebUI generates a random secret on each boot,
    // invalidating all existing JWTs after restart_servers recreates the
    // container — causing 401 errors on push_tool_connections and
    // push_system_prompt.
    env.push("WEBUI_SECRET_KEY=ibex-desktop-local-secret-key".to_string());

    // Default model fallback — Open WebUI uses this if no model is set via API.
    // Belt-and-suspenders: the Rust code also discovers and sets the model via
    // push_system_prompt(), but this env var ensures a sensible default even if
    // the API-based model selection fails (e.g. LLM backends unreachable).
    env.push("DEFAULT_MODELS=qwen3.5:35b".to_string());

    // Preconfigure LLM backends for Percona internal servers
    // LM Studio (OpenAI-compatible API)
    env.push(
        "OPENAI_API_BASE_URLS=https://mac-studio-lm.int.percona.com/v1".to_string(),
    );
    env.push("OPENAI_API_KEYS=not-needed".to_string());

    // Ollama backend
    env.push(
        "OLLAMA_BASE_URLS=https://mac-studio-ollama.int.percona.com".to_string(),
    );

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(vec![volume]),
        ..Default::default()
    };

    let mut exposed_ports = HashMap::new();
    exposed_ports.insert(CONTAINER_PORT.to_string(), HashMap::new());

    let container_config = Config {
        image: Some(IMAGE.to_string()),
        env: Some(env.clone()),
        exposed_ports: Some(exposed_ports),
        host_config: Some(host_config),
        ..Default::default()
    };

    let opts = CreateContainerOptions {
        name: CONTAINER_NAME,
        ..Default::default()
    };

    let response = docker
        .create_container(Some(opts), container_config)
        .await
        .map_err(|e| format!("Failed to create container: {e}"))?;

    log::info!("Container created: {}", response.id);
    Ok(response.id)
}

/// Start the container.
pub async fn start_container() -> Result<(), String> {
    let docker = connect().await?;

    docker
        .start_container(CONTAINER_NAME, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| format!("Failed to start container: {e}"))
}

/// Stop the container.
pub async fn stop_container() -> Result<(), String> {
    let docker = connect().await?;

    docker
        .stop_container(CONTAINER_NAME, Some(StopContainerOptions { t: 10 }))
        .await
        .map_err(|e| format!("Failed to stop container: {e}"))
}

/// Remove the container (for recreation with new config).
pub async fn remove_container() -> Result<(), String> {
    let docker = connect().await?;

    docker
        .remove_container(
            CONTAINER_NAME,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| format!("Failed to remove container: {e}"))
}

/// Wait for Open WebUI to become healthy (up to timeout_secs).
///
/// If the backend refuses to start because WEBUI_AUTH=false conflicts with
/// existing users in the DB, automatically resets the DB and restarts.
pub async fn wait_for_healthy(timeout_secs: u64) -> Result<(), String> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let mut auth_reset_attempted = false;

    while start.elapsed() < timeout {
        if check_webui_health().await {
            return Ok(());
        }

        // Check if the container crashed due to auth migration conflict.
        // Open WebUI exits when WEBUI_AUTH=false but existing users are in the DB.
        if !auth_reset_attempted {
            if let Ok(docker) = connect().await {
                if let Ok(info) = docker.inspect_container(CONTAINER_NAME, None).await {
                    let is_running = info
                        .state
                        .as_ref()
                        .and_then(|s| s.running)
                        .unwrap_or(true);

                    if !is_running {
                        log::warn!("Container stopped unexpectedly — attempting DB reset for auth migration");
                        auth_reset_attempted = true;
                        reset_webui_db();
                        let _ = start_container().await;
                        // Give it time to start fresh
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Err(format!(
        "IBEX not healthy after {timeout_secs}s"
    ))
}

/// Check if Open WebUI is responding on localhost:8080.
async fn check_webui_health() -> bool {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    client
        .get("http://localhost:8080/health")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Check if an existing container needs to be recreated (e.g., env vars changed).
/// Returns (needs_recreation, was_auth_enabled) so callers know if DB reset is needed.
///
/// Compares: WEBUI_AUTH, LLM backends, and TOOL_SERVER_CONNECTIONS (MCP servers).
/// The TOOL_SERVER_CONNECTIONS check ensures that when connectors are added/removed
/// in Settings, the container is recreated so Open WebUI discovers the new MCP servers.
async fn needs_recreation(config: &IbexConfig) -> (bool, bool) {
    let docker = match connect().await {
        Ok(d) => d,
        Err(_) => return (false, false),
    };

    match docker.inspect_container(CONTAINER_NAME, None).await {
        Ok(info) => {
            let env_vars = info
                .config
                .and_then(|c| c.env)
                .unwrap_or_default();

            // Recreate if container still has old WEBUI_AUTH=true
            let has_auth_false = env_vars.iter().any(|e| e == "WEBUI_AUTH=false");
            if !has_auth_false {
                log::info!("Container needs recreation: WEBUI_AUTH=false not set");
                return (true, true); // was_auth_enabled = true → need DB reset
            }

            // Recreate if LLM backend env vars are missing
            let has_ollama = env_vars
                .iter()
                .any(|e| e.starts_with("OLLAMA_BASE_URLS="));
            let has_openai = env_vars
                .iter()
                .any(|e| e.starts_with("OPENAI_API_BASE_URLS="));
            if !has_ollama || !has_openai {
                log::info!("Container needs recreation: LLM backend env vars missing");
                return (true, false); // no DB reset needed
            }

            // Recreate if TOOL_SERVER_CONNECTIONS changed (connectors added/removed)
            let expected_mcp = build_mcp_connections(config);
            let current_mcp = env_vars
                .iter()
                .find(|e| e.starts_with("TOOL_SERVER_CONNECTIONS="))
                .map(|e| e.trim_start_matches("TOOL_SERVER_CONNECTIONS=").to_string())
                .unwrap_or_default();
            if current_mcp != expected_mcp {
                log::info!(
                    "Container needs recreation: TOOL_SERVER_CONNECTIONS changed \
                     (current has {} servers, expected has {} servers)",
                    current_mcp.matches("\"type\":\"mcp\"").count(),
                    expected_mcp.matches("\"type\":\"mcp\"").count()
                );
                return (true, false);
            }

            (false, false)
        }
        Err(_) => (false, false),
    }
}

/// Delete webui.db to allow switching from WEBUI_AUTH=true to WEBUI_AUTH=false.
///
/// Open WebUI refuses to disable auth when existing users are in the database.
/// This is only called when upgrading from an auth-enabled container.
fn reset_webui_db() {
    let db_path = match dirs::home_dir() {
        Some(home) => home.join("open-webui-data").join("webui.db"),
        None => return,
    };
    if db_path.exists() {
        match std::fs::remove_file(&db_path) {
            Ok(()) => log::info!("Deleted {} for auth migration", db_path.display()),
            Err(e) => log::warn!("Failed to delete {}: {e}", db_path.display()),
        }
    }
}

/// Ensure container is running. Creates if missing, starts if stopped.
/// Recreates if env vars changed (e.g., auth disabled).
pub async fn ensure_running(config: &IbexConfig) -> Result<DockerStatus, String> {
    let status = check_status().await;

    match status {
        DockerStatus::NotInstalled => {
            Err("Docker is not installed. Please install Docker Desktop.".to_string())
        }
        DockerStatus::NotRunning => {
            Err("Docker is not running. Please start Docker Desktop.".to_string())
        }
        DockerStatus::ContainerMissing => {
            log::info!("Container missing, pulling image and creating...");
            pull_image().await?;
            create_container(config).await?;
            start_container().await?;
            Ok(DockerStatus::ContainerRunning)
        }
        DockerStatus::ContainerStopped => {
            let (recreate, was_auth) = needs_recreation(config).await;
            if recreate {
                log::info!("Recreating container with updated config...");
                remove_container().await?;
                if was_auth {
                    reset_webui_db();
                }
                create_container(config).await?;
                start_container().await?;
            } else {
                log::info!("Container stopped, starting...");
                start_container().await?;
            }
            Ok(DockerStatus::ContainerRunning)
        }
        DockerStatus::ContainerRunning | DockerStatus::Healthy => {
            let (recreate, was_auth) = needs_recreation(config).await;
            if recreate {
                log::info!("Recreating running container with updated config...");
                remove_container().await?;
                if was_auth {
                    reset_webui_db();
                }
                create_container(config).await?;
                start_container().await?;
                Ok(DockerStatus::ContainerRunning)
            } else {
                Ok(status)
            }
        }
    }
}
