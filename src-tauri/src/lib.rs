//! IBEX Desktop — native macOS wrapper for IBEX workplace AI assistant.

pub mod account;
pub mod config;
pub mod docker;
pub mod keychain;
pub mod process;
pub mod prompt;
pub mod state;
pub mod tray;

use state::AppState;
use std::sync::Arc;
use tauri::tray::TrayIcon;
use tauri::{Emitter, Listener, Manager, RunEvent, WindowEvent};

// ── Tauri Commands (callable from Svelte frontend) ──

/// Get current connector configuration (for settings UI).
#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> Result<config::IbexConfig, String> {
    let cfg = state.config.lock().map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

/// Save connector configuration (from settings UI).
#[tauri::command]
fn save_config(
    state: tauri::State<'_, AppState>,
    new_config: config::IbexConfig,
) -> Result<(), String> {
    new_config.save()?;
    let mut cfg = state.config.lock().map_err(|e| e.to_string())?;
    *cfg = new_config;
    Ok(())
}

/// Get app health status (for frontend display).
#[tauri::command]
fn get_health(state: tauri::State<'_, AppState>) -> state::AppHealth {
    state.compute_health()
}

/// Get list of configured connectors.
#[tauri::command]
fn get_configured_connectors(state: tauri::State<'_, AppState>) -> Vec<String> {
    let cfg = state.config.lock().unwrap();
    cfg.configured_connectors()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Get Docker status.
#[tauri::command]
fn get_docker_status(state: tauri::State<'_, AppState>) -> state::DockerStatus {
    state.docker_status.lock().unwrap().clone()
}

/// Get all server statuses.
#[tauri::command]
fn get_server_statuses(
    state: tauri::State<'_, AppState>,
) -> std::collections::HashMap<String, state::ServerStatus> {
    state.server_statuses.lock().unwrap().clone()
}

/// Get JWT token (for auto-login injection into localStorage).
#[tauri::command]
fn get_jwt_token(state: tauri::State<'_, AppState>) -> Option<String> {
    state.jwt_token.lock().unwrap().clone()
}

/// Get startup status (for setup wizard to poll).
#[tauri::command]
fn get_startup_status(
    state: tauri::State<'_, AppState>,
) -> std::collections::HashMap<String, serde_json::Value> {
    let mut result = std::collections::HashMap::new();
    let complete = *state.startup_complete.lock().unwrap();
    let error = state.startup_error.lock().unwrap().clone();
    result.insert("complete".to_string(), serde_json::json!(complete));
    result.insert("error".to_string(), serde_json::json!(error));
    result
}

/// Check if the setup wizard should be shown.
///
/// Returns true if no connectors are configured (first run or factory reset).
/// The frontend calls this on startup so it can redirect to /setup even when
/// a stale stores.json already has webui_base_url set from a previous install.
#[tauri::command]
fn needs_setup(state: tauri::State<'_, AppState>) -> bool {
    let config = state.config.lock().unwrap();
    config.configured_connectors().is_empty()
}

/// Check network connectivity by trying to reach well-known endpoints.
/// Used by the setup wizard to check whether the Percona VPN is connected.
///
/// Probes internal Percona endpoints (`*.int.percona.com`) that are only
/// reachable over the corporate VPN. A successful TCP connection (or any
/// HTTP response) to any of them means the VPN tunnel is up.
#[tauri::command]
async fn check_network_connectivity() -> bool {
    use std::net::ToSocketAddrs;

    // Internal-only Percona hosts — only reachable on VPN
    let internal_hosts = [
        "mac-studio-lm.int.percona.com",
        "mac-studio-ollama.int.percona.com",
    ];

    // Quick DNS + TCP check: if the hostname resolves and we can open a
    // TCP socket, the VPN is up. This is faster than a full HTTP request.
    for host in &internal_hosts {
        let addr = format!("{host}:443");
        match addr.to_socket_addrs() {
            Ok(addrs) => {
                for a in addrs {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        tokio::net::TcpStream::connect(a),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            log::info!("VPN connectivity confirmed — {host} reachable");
                            return true;
                        }
                        Ok(Err(e)) => {
                            log::debug!("TCP connect to {host} ({a}): {e}");
                        }
                        Err(_) => {
                            log::debug!("TCP connect to {host} ({a}) timed out");
                        }
                    }
                }
            }
            Err(e) => {
                log::debug!("DNS resolution failed for {host}: {e}");
            }
        }
    }

    log::warn!("VPN check failed — no internal hosts reachable");
    false
}

/// Wait for Docker container to become healthy (up to 60s).
/// Called by the setup wizard after restart_servers to ensure the container
/// is ready before transitioning to the main app.
#[tauri::command]
async fn wait_for_docker_healthy() -> Result<(), String> {
    docker::wait_for_healthy(60).await
}

/// Restart MCP servers (callable from settings UI after config change).
///
/// Order of operations matters:
///   1. Stop existing MCP servers
///   2. Start NEW MCP servers (so they're listening before Open WebUI boots)
///   3. Recreate Docker container (Open WebUI reads TOOL_SERVER_CONNECTIONS on boot)
///   4. Wait for Docker to be healthy
///   5. Push tool connections via admin API (ensures database matches env var)
///   6. Push system prompt + default model
#[tauri::command]
async fn restart_servers(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    log::info!("Restarting MCP servers...");

    // 1. Stop existing servers
    {
        let mut processes = state.server_processes.lock().map_err(|e| e.to_string())?;
        for (name, child) in processes.iter_mut() {
            log::info!("Stopping {name}...");
            let _ = child.start_kill();
        }
        processes.clear();
    }

    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    let base_url = state.webui_url.lock().map_err(|e| e.to_string())?.clone();
    let jwt = state
        .jwt_token
        .lock()
        .map_err(|e| e.to_string())?
        .clone();

    // 2. Start NEW MCP servers BEFORE Docker recreation.
    //    Open WebUI verifies MCP connections on startup — if servers aren't
    //    listening yet, it marks them as unavailable.
    let node_bin = resolve_node_binary();
    let servers_dir = resolve_servers_dir();
    if let (Some(ref node_bin), Some(ref servers_dir)) = (node_bin, servers_dir) {
        let new_processes = process::start_all(node_bin, servers_dir, &config);
        let started: Vec<String> = new_processes.keys().cloned().collect();
        log::info!("Started MCP servers: {:?}", started);

        let mut procs = state.server_processes.lock().map_err(|e| e.to_string())?;
        for (name, child) in new_processes {
            procs.insert(name, child);
        }
    }

    // Brief pause to let MCP servers bind to ports
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 3. Recreate Docker container with updated TOOL_SERVER_CONNECTIONS
    if let Err(e) = docker::ensure_running(&config).await {
        log::warn!("Failed to update container on restart: {e}");
    }

    // 4. Wait for Docker to be healthy before pushing config
    if let Err(e) = docker::wait_for_healthy(60).await {
        log::warn!("Docker health wait failed after restart: {e}");
    }

    // 5. Re-authenticate — container recreation may have changed the JWT secret
    //    (if WEBUI_SECRET_KEY wasn't set on the old container). Always re-auth
    //    to ensure we have a valid token for the push operations below.
    let fresh_jwt = match account::ensure_authenticated(&base_url).await {
        Ok(Some(token)) => {
            log::info!("Re-authenticated after container restart");
            *state.jwt_token.lock().map_err(|e| e.to_string())? = Some(token.clone());
            Some(token)
        }
        Ok(None) => {
            log::warn!("Re-authentication returned no token");
            jwt.clone()
        }
        Err(e) => {
            log::warn!("Re-authentication failed: {e}");
            jwt.clone()
        }
    };

    // 6. Push tool connections via admin API (belt-and-suspenders).
    //    Open WebUI's PersistentConfig may prefer stale database values over
    //    the env var. This API call updates both in-memory and database.
    if let Some(ref jwt) = fresh_jwt {
        let mcp_json = docker::build_mcp_connections(&config);
        if let Err(e) = account::push_tool_connections(&base_url, jwt, &mcp_json).await {
            log::warn!("Failed to push tool connections via API: {e}");
        }
    }

    // 7. Push system prompt + attempt model upgrade
    if let Some(ref jwt) = fresh_jwt {
        let system_prompt = prompt::build_system_prompt(&config);
        if let Err(e) = account::push_system_prompt(&base_url, jwt, &system_prompt).await {
            log::warn!("Failed to update system prompt on restart: {e}");
        }
    }

    app.emit("servers-restarted", ()).ok();
    log::info!("Server restart complete");
    Ok(())
}

// ── App Entry Point ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_health,
            get_configured_connectors,
            get_docker_status,
            get_server_statuses,
            get_startup_status,
            get_jwt_token,
            needs_setup,
            check_network_connectivity,
            restart_servers,
            wait_for_docker_healthy,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Setup system tray
            let tray: Option<Arc<TrayIcon>> = match tray::setup_tray(&handle) {
                Ok(t) => {
                    log::info!("System tray initialized");
                    Some(Arc::new(t))
                }
                Err(e) => {
                    log::error!("Failed to setup tray: {e}");
                    None
                }
            };

            // Spawn background startup sequence
            let startup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = startup_handle.state::<AppState>();
                match startup_sequence(&startup_handle).await {
                    Ok(()) => {
                        *state.startup_complete.lock().unwrap() = true;
                    }
                    Err(e) => {
                        log::error!("Startup failed: {e}");
                        *state.startup_error.lock().unwrap() = Some(e.clone());
                        startup_handle.emit("startup-error", e).ok();
                    }
                }
            });

            // Spawn background health poller (updates tray + server status every 10s)
            if let Some(tray_arc) = tray {
                let poller_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    health_poller(&poller_handle, &tray_arc).await;
                });

                // Listen for restart events from tray menu
                let restart_handle = handle.clone();
                handle.listen("tray-restart-servers", move |_| {
                    let rh = restart_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = rh.state::<AppState>();

                        // 1. Stop existing servers
                        {
                            let mut procs =
                                state.server_processes.lock().unwrap_or_else(|e| e.into_inner());
                            for (name, child) in procs.iter_mut() {
                                log::info!("Stopping {name}...");
                                let _ = child.start_kill();
                            }
                            procs.clear();
                        }

                        let config = state.config.lock().unwrap().clone();
                        let base_url = state.webui_url.lock().unwrap().clone();
                        let jwt = state.jwt_token.lock().unwrap().clone();

                        // 2. Start MCP servers BEFORE Docker recreation
                        let node_bin = resolve_node_binary();
                        let servers_dir = resolve_servers_dir();
                        if let (Some(ref node_bin), Some(ref servers_dir)) = (node_bin, servers_dir) {
                            let new_procs = process::start_all(node_bin, servers_dir, &config);
                            let mut procs =
                                state.server_processes.lock().unwrap_or_else(|e| e.into_inner());
                            for (name, child) in new_procs {
                                procs.insert(name, child);
                            }
                        }

                        // Brief pause for servers to bind
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                        // 3. Recreate Docker container
                        let _ = docker::ensure_running(&config).await;

                        // 4. Wait for healthy
                        let _ = docker::wait_for_healthy(60).await;

                        // 5. Re-authenticate (container recreation may invalidate JWT)
                        let fresh_jwt = match account::ensure_authenticated(&base_url).await {
                            Ok(Some(token)) => {
                                *state.jwt_token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
                                Some(token)
                            }
                            Ok(None) => jwt.clone(),
                            Err(_) => jwt.clone(),
                        };

                        // 6. Push tool connections + prompt
                        if let Some(ref jwt) = fresh_jwt {
                            let mcp_json = docker::build_mcp_connections(&config);
                            let _ = account::push_tool_connections(&base_url, jwt, &mcp_json).await;

                            let prompt = prompt::build_system_prompt(&config);
                            let _ = account::push_system_prompt(&base_url, jwt, &prompt).await;
                        }

                        log::info!("Servers restarted from tray menu");
                    });
                });
            }

            Ok(())
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(move |app_handle, event| {
            match event {
                RunEvent::WindowEvent {
                    label,
                    event: WindowEvent::CloseRequested { api, .. },
                    ..
                } => {
                    // Close window → hide to tray (don't quit app)
                    if label == "main" {
                        api.prevent_close();
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                        log::info!("Main window hidden to tray");
                    }
                    // Settings window closes normally (destroyed)
                }
                RunEvent::ExitRequested { .. } => {
                    // Graceful shutdown: kill MCP servers, keep Docker running
                    let state = app_handle.state::<AppState>();
                    let mut processes = match state.server_processes.lock() {
                        Ok(p) => p,
                        Err(_) => return,
                    };
                    for (name, child) in processes.iter_mut() {
                        log::info!("Shutting down {name}...");
                        let _ = child.start_kill();
                    }
                    processes.clear();
                    drop(processes);
                    // Docker container intentionally left running for fast restart
                    log::info!("IBEX shutdown complete");
                }
                _ => {}
            }
        });
}

/// Background startup sequence.
///
/// 1. Load config
/// 2. Check Docker → ensure container running
/// 3. Wait for healthy
/// 4. Authenticate (create account or sign in)
/// 5. Start configured MCP servers
/// 6. Build + push system prompt
/// 7. Update tray status
async fn startup_sequence(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    app.emit("startup-status", "Loading configuration...").ok();

    // 1. Config is already loaded in AppState::new()
    let config = state.config.lock().unwrap().clone();
    let connectors = config.configured_connectors();
    log::info!("Configured connectors: {:?}", connectors);

    // 2. Check Docker
    app.emit("startup-status", "Checking Docker...").ok();
    let docker_status = docker::check_status().await;
    *state.docker_status.lock().unwrap() = docker_status.clone();

    match docker_status {
        state::DockerStatus::NotInstalled => {
            return Err(
                "Docker is not installed. Please install Docker Desktop from docker.com"
                    .to_string(),
            );
        }
        state::DockerStatus::NotRunning => {
            return Err(
                "Docker is not running. Please start Docker Desktop.".to_string(),
            );
        }
        _ => {}
    }

    // 3. Ensure container running
    app.emit("startup-status", "Starting IBEX container...")
        .ok();
    docker::ensure_running(&config).await?;
    *state.docker_status.lock().unwrap() = state::DockerStatus::ContainerRunning;

    // 4. Wait for healthy
    app.emit("startup-status", "Waiting for IBEX to start...")
        .ok();
    docker::wait_for_healthy(90).await?;
    *state.docker_status.lock().unwrap() = state::DockerStatus::Healthy;

    // 5. Authenticate (non-fatal: if admin already exists, user logs in manually)
    app.emit("startup-status", "Authenticating...").ok();
    let base_url = state.webui_url.lock().unwrap().clone();
    let jwt_opt = account::ensure_authenticated(&base_url).await?;
    *state.jwt_token.lock().unwrap() = jwt_opt.clone();

    // 6. Push tool connections + system prompt (only if we have an auth token)
    if let Some(ref jwt) = jwt_opt {
        app.emit("startup-status", "Configuring AI assistant...")
            .ok();

        // Push tool connections via admin API to ensure database is in sync.
        // On first run this may push an empty list (no connectors configured yet).
        // restart_servers() will push the real connections after setup wizard.
        let mcp_json = docker::build_mcp_connections(&config);
        if let Err(e) = account::push_tool_connections(&base_url, jwt, &mcp_json).await {
            log::warn!("Failed to push tool connections: {e}");
        }

        let system_prompt = prompt::build_system_prompt(&config);
        if let Err(e) = account::push_system_prompt(&base_url, jwt, &system_prompt).await {
            log::warn!("Failed to push system prompt: {e}");
        }

        // Inject JWT into the webview's localStorage so the frontend can use it
        // immediately without going through the /auth flow. This fixes:
        // - Socket.IO connecting without auth (session_id error)
        // - Unnecessary redirect to /auth page
        // - Frontend authenticating as wrong user (stale token from previous session)
        //
        // IMPORTANT: First remove any stale token. A previous session may have
        // left a token for the default "User" account (created by /auth auto-signin).
        // Without clearing it first, the frontend races: it finds the old token,
        // authenticates as "User", sets $user, and then the startup-complete event
        // can't override it.
        if let Some(window) = app.get_webview_window("main") {
            let escaped_jwt = jwt.replace('\\', "\\\\").replace('\'', "\\'");

            // On first run (no connectors configured), the user is in the setup
            // wizard. We inject the JWT but do NOT reload the page — the reload
            // would disrupt the setup wizard. The setup wizard handles the
            // transition to the main app via goToChat() after the user finishes
            // configuration.
            //
            // For returning users (connectors already configured), we inject +
            // reload. The reload is critical because:
            // 1. Socket.IO may have already connected without auth (stale session_id)
            // 2. The frontend reads localStorage('token') on page load to authenticate
            // 3. A reload establishes a fresh Socket.IO connection with the correct JWT
            let is_first_run = connectors.is_empty();

            let js = if is_first_run {
                format!(
                    "localStorage.removeItem('token'); \
                     localStorage.setItem('token', '{escaped_jwt}');"
                )
            } else {
                format!(
                    "localStorage.removeItem('token'); \
                     localStorage.setItem('token', '{escaped_jwt}'); \
                     setTimeout(() => location.reload(), 500)"
                )
            };

            match window.eval(&js) {
                Ok(()) => {
                    if is_first_run {
                        log::info!("JWT injected (no reload — first run, setup wizard active)");
                    } else {
                        log::info!("JWT injected + page reload scheduled");
                    }
                }
                Err(e) => log::warn!("Failed to inject JWT into webview: {e}"),
            }
        }
    } else {
        log::info!("Skipping system prompt push — no admin auth. User will log in manually.");
    }

    // 7. Start MCP servers
    app.emit("startup-status", "Starting MCP servers...").ok();

    // Pre-flight: check for port conflicts on MCP server ports
    for server_def in process::SERVERS {
        if process::should_start(server_def.name, &config) {
            if let Err(msg) = process::check_port_available(server_def.port) {
                log::warn!("Port conflict: {msg}");
                app.emit("startup-warning", &msg).ok();
            }
        }
    }

    // Resolve Node.js binary and server scripts directory.
    // In production: use Tauri-bundled sidecar + resources.
    // In development: use system node and the original ~/IBEX repo.
    let node_bin = resolve_node_binary();
    let servers_dir = resolve_servers_dir();

    if let (Some(node_bin), Some(servers_dir)) = (node_bin, servers_dir) {
        log::info!("Node.js: {}", node_bin.display());
        log::info!("Servers dir: {}", servers_dir.display());

        let new_processes = process::start_all(&node_bin, &servers_dir, &config);
        let started: Vec<String> = new_processes.keys().cloned().collect();
        log::info!("Started MCP servers: {:?}", started);

        // Store child process handles for lifecycle management
        let mut procs = state.server_processes.lock().unwrap();
        for (name, child) in new_processes {
            procs.insert(name, child);
        }
    } else {
        log::warn!(
            "Node.js binary or server scripts not found — MCP servers will not start. \
             Ensure Node.js is installed and IBEX server scripts are available."
        );
    }

    // 8. Update health
    *state.app_health.lock().unwrap() = state::AppHealth::Healthy;

    app.emit("startup-status", "Ready!").ok();
    app.emit("startup-complete", true).ok();

    log::info!("IBEX startup complete");
    Ok(())
}

/// Resolve the path to the Node.js binary.
///
/// Tries in order:
/// 1. Bundled sidecar (production: IBEX.app/Contents/MacOS/node-aarch64-apple-darwin)
/// 2. /opt/homebrew/bin/node (macOS ARM64 Homebrew)
/// 3. /usr/local/bin/node (macOS Intel Homebrew)
/// 4. `which node` via PATH
fn resolve_node_binary() -> Option<std::path::PathBuf> {
    // 1. Bundled sidecar — Tauri externalBin places the binary alongside the app
    //    executable. Tauri strips the target-triple suffix when bundling, so the
    //    production binary is just "node". Dev builds may keep the full name.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Production: Tauri strips the triple → "node"
            let sidecar = dir.join("node");
            if sidecar.exists() {
                log::info!("Using bundled Node.js: {}", sidecar.display());
                return Some(sidecar);
            }
            // Dev builds may have the full name with triple
            let sidecar_dev = dir.join("node-aarch64-apple-darwin");
            if sidecar_dev.exists() {
                log::info!("Using bundled Node.js: {}", sidecar_dev.display());
                return Some(sidecar_dev);
            }
        }
    }

    // 2. Development: well-known Homebrew paths
    let candidates = ["/opt/homebrew/bin/node", "/usr/local/bin/node"];
    for path in &candidates {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // 3. Fallback: search PATH
    if let Ok(output) = std::process::Command::new("which").arg("node").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(std::path::PathBuf::from(path));
            }
        }
    }

    None
}

/// Resolve the path to the IBEX server scripts directory.
///
/// Tries in order:
/// 1. Bundled resources (production: IBEX.app/Contents/Resources/resources/ibex-servers/)
/// 2. ~/IBEX (development — the original IBEX repo)
fn resolve_servers_dir() -> Option<std::path::PathBuf> {
    // 1. Bundled resources — Tauri preserves the directory structure from
    //    src-tauri/resources/, so the path is:
    //      exe = IBEX.app/Contents/MacOS/IBEX
    //      resources = IBEX.app/Contents/Resources/resources/ibex-servers/
    //    (note the nested "resources/" — Tauri keeps the src-tauri/resources/ dirname)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                // Production: Tauri nests under Resources/resources/
                let bundled = contents_dir
                    .join("Resources")
                    .join("resources")
                    .join("ibex-servers");
                if bundled.join("servers").exists() && bundled.join("connectors").exists() {
                    log::info!("Using bundled servers: {}", bundled.display());
                    return Some(bundled);
                }
                // Fallback: check without the extra nesting (in case Tauri changes)
                let bundled_alt = contents_dir.join("Resources").join("ibex-servers");
                if bundled_alt.join("servers").exists()
                    && bundled_alt.join("connectors").exists()
                {
                    log::info!("Using bundled servers: {}", bundled_alt.display());
                    return Some(bundled_alt);
                }
            }
        }
    }

    // 2. Development: ~/IBEX directory
    if let Some(home) = dirs::home_dir() {
        let dev_path = home.join("IBEX");
        if dev_path.join("servers").exists() && dev_path.join("connectors").exists() {
            return Some(dev_path);
        }
    }

    None
}

/// Background health poller — runs every 10 seconds.
///
/// Checks Docker and MCP server health, updates state and tray icon.
/// Also reloads config from disk (picks up changes from terminal configure.sh).
async fn health_poller(app: &tauri::AppHandle, tray: &TrayIcon) {
    // Wait for initial startup to complete
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let state = app.state::<AppState>();

        // Reload config from disk (backward compat with terminal configure.sh)
        state.reload_config();

        // Check Docker status
        let docker_status = docker::check_status().await;
        *state.docker_status.lock().unwrap() = docker_status;

        // Check MCP server health
        let config = state.config.lock().unwrap().clone();
        for server_def in process::SERVERS {
            if process::should_start(server_def.name, &config) {
                let healthy = process::health_check(server_def.port).await;
                let mut statuses = state.server_statuses.lock().unwrap();
                statuses.insert(
                    server_def.name.to_string(),
                    state::ServerStatus {
                        name: server_def.name.to_string(),
                        port: server_def.port,
                        running: healthy, // If we can health-check it, it's running
                        healthy,
                    },
                );
            }
        }

        // Recompute health and update tray
        let health = state.compute_health();
        *state.app_health.lock().unwrap() = health;

        if let Err(e) = tray::update_tray_menu(app, tray) {
            log::warn!("Failed to update tray: {e}");
        }
    }
}
