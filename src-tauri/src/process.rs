//! MCP server process lifecycle management.
//!
//! Spawns and manages Node.js MCP server processes.
//! Each server runs as `node servers/<name>.js --http` on its assigned port.

use crate::config::IbexConfig;
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use tokio::process::{Child, Command};

/// Server definition: name, script file, port, and config check.
#[derive(Debug, Clone)]
pub struct ServerDef {
    pub name: &'static str,
    pub script: &'static str,
    pub port: u16,
}

/// All known MCP servers with their ports.
pub const SERVERS: &[ServerDef] = &[
    ServerDef {
        name: "slack",
        script: "servers/slack.js",
        port: 3001,
    },
    ServerDef {
        name: "notion",
        script: "servers/notion.js",
        port: 3002,
    },
    ServerDef {
        name: "jira",
        script: "servers/jira.js",
        port: 3003,
    },
    ServerDef {
        name: "memory",
        script: "servers/memory.js",
        port: 3004,
    },
    ServerDef {
        name: "servicenow",
        script: "servers/servicenow.js",
        port: 3005,
    },
    ServerDef {
        name: "salesforce",
        script: "servers/salesforce.js",
        port: 3006,
    },
];

/// Check if a server should be started based on config.
pub fn should_start(name: &str, config: &IbexConfig) -> bool {
    match name {
        "slack" => config.is_slack_configured(),
        "notion" => config.is_notion_configured(),
        "jira" => config.is_jira_configured(),
        "memory" => config.is_memory_configured(),
        "servicenow" => config.is_servicenow_configured(),
        "salesforce" => config.is_salesforce_configured(),
        _ => false,
    }
}

/// Build environment variables for a server process.
fn build_env(config: &IbexConfig) -> HashMap<String, String> {
    let mut env = HashMap::new();

    if let Some(ref v) = config.slack_token {
        env.insert("SLACK_TOKEN".to_string(), v.clone());
    }
    if let Some(ref v) = config.notion_token {
        env.insert("NOTION_TOKEN".to_string(), v.clone());
    }
    if let Some(ref v) = config.jira_domain {
        env.insert("JIRA_DOMAIN".to_string(), v.clone());
    }
    if let Some(ref v) = config.jira_email {
        env.insert("JIRA_EMAIL".to_string(), v.clone());
    }
    if let Some(ref v) = config.jira_api_token {
        env.insert("JIRA_API_TOKEN".to_string(), v.clone());
    }
    if let Some(ref v) = config.github_token {
        env.insert("GITHUB_TOKEN".to_string(), v.clone());
    }
    if let Some(ref v) = config.github_owner {
        env.insert("GITHUB_OWNER".to_string(), v.clone());
    }
    if let Some(ref v) = config.github_repo {
        env.insert("GITHUB_REPO".to_string(), v.clone());
    }
    if let Some(ref v) = config.github_memory_path {
        env.insert("GITHUB_MEMORY_PATH".to_string(), v.clone());
    }
    if let Some(ref v) = config.servicenow_instance {
        env.insert("SERVICENOW_INSTANCE".to_string(), v.clone());
    }
    if let Some(ref v) = config.servicenow_username {
        env.insert("SERVICENOW_USERNAME".to_string(), v.clone());
    }
    if let Some(ref v) = config.servicenow_password {
        env.insert("SERVICENOW_PASSWORD".to_string(), v.clone());
    }
    if let Some(ref v) = config.salesforce_instance_url {
        env.insert("SALESFORCE_INSTANCE_URL".to_string(), v.clone());
    }
    if let Some(ref v) = config.salesforce_username {
        env.insert("SALESFORCE_USERNAME".to_string(), v.clone());
    }
    if let Some(ref v) = config.salesforce_password {
        env.insert("SALESFORCE_PASSWORD".to_string(), v.clone());
    }
    if let Some(ref v) = config.salesforce_security_token {
        env.insert("SALESFORCE_SECURITY_TOKEN".to_string(), v.clone());
    }

    env
}

/// Spawn a single MCP server process.
///
/// `node_bin`: path to the Node.js binary
/// `servers_dir`: path to the directory containing server scripts
pub fn spawn_server(
    server: &ServerDef,
    node_bin: &PathBuf,
    servers_dir: &PathBuf,
    config: &IbexConfig,
) -> Result<Child, String> {
    let script_path = servers_dir.join(server.script);

    if !script_path.exists() {
        return Err(format!(
            "Server script not found: {}",
            script_path.display()
        ));
    }

    let env = build_env(config);

    let child = Command::new(node_bin)
        .arg(&script_path)
        .arg("--http")
        .envs(&env)
        .current_dir(servers_dir)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {e}", server.name))?;

    log::info!("Started {} on port {}", server.name, server.port);
    Ok(child)
}

/// Start all configured servers.
/// Returns a map of server name → child process.
pub fn start_all(
    node_bin: &PathBuf,
    servers_dir: &PathBuf,
    config: &IbexConfig,
) -> HashMap<String, Child> {
    let mut processes = HashMap::new();

    for server in SERVERS {
        if should_start(server.name, config) {
            match spawn_server(server, node_bin, servers_dir, config) {
                Ok(child) => {
                    processes.insert(server.name.to_string(), child);
                }
                Err(e) => {
                    log::error!("Failed to start {}: {e}", server.name);
                }
            }
        } else {
            log::info!("Skipping {} (not configured)", server.name);
        }
    }

    processes
}

/// Stop all running server processes.
pub async fn stop_all(processes: &mut HashMap<String, Child>) {
    for (name, child) in processes.iter_mut() {
        log::info!("Stopping {name}...");
        if let Err(e) = child.kill().await {
            log::warn!("Failed to kill {name}: {e}");
        }
    }
    processes.clear();
}

/// Health check a single server via HTTP.
pub async fn health_check(port: u16) -> bool {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    let url = format!("http://localhost:{port}/health");
    client
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Check if a TCP port is available for binding.
/// Returns Ok(()) if available, Err(description) if already in use.
pub fn check_port_available(port: u16) -> Result<(), String> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => Ok(()), // Port is free; the listener drops immediately
        Err(_) => {
            let process_info = identify_port_user(port);
            Err(format!(
                "Port {port} is already in use{}. Please free this port and restart IBEX.",
                if let Some(info) = process_info {
                    format!(" by {info}")
                } else {
                    String::new()
                }
            ))
        }
    }
}

/// Best-effort: identify what process is using a port (macOS only).
fn identify_port_user(port: u16) -> Option<String> {
    let output = std::process::Command::new("lsof")
        .args(["-i", &format!(":{port}"), "-sTCP:LISTEN", "-P", "-n"])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse first data line: COMMAND PID USER ...
        stdout.lines().nth(1).map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                format!("{} (PID {})", parts[0], parts[1])
            } else {
                line.to_string()
            }
        })
    } else {
        None
    }
}

/// Health check all known servers, returning name → healthy status.
pub async fn health_check_all(config: &IbexConfig) -> HashMap<String, bool> {
    let mut results = HashMap::new();

    for server in SERVERS {
        if should_start(server.name, config) {
            let healthy = health_check(server.port).await;
            results.insert(server.name.to_string(), healthy);
        }
    }

    results
}
