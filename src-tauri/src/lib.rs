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

/// Restart MCP servers (callable from settings UI after config change).
#[tauri::command]
async fn restart_servers(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    log::info!("Restarting MCP servers...");

    // Stop existing servers
    {
        let mut processes = state.server_processes.lock().map_err(|e| e.to_string())?;
        for (name, child) in processes.iter_mut() {
            log::info!("Stopping {name}...");
            let _ = child.start_kill();
        }
        processes.clear();
    }

    // Rebuild prompt + re-push
    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    let base_url = state.webui_url.lock().map_err(|e| e.to_string())?.clone();
    let jwt = state
        .jwt_token
        .lock()
        .map_err(|e| e.to_string())?
        .clone();

    if let Some(jwt) = jwt {
        let system_prompt = prompt::build_system_prompt(&config);
        if let Err(e) = account::push_system_prompt(&base_url, &jwt, &system_prompt).await {
            log::warn!("Failed to update system prompt on restart: {e}");
        }

        // Also update docker container env with new TOOL_SERVER_CONNECTIONS
        if let Err(e) = docker::ensure_running(&config).await {
            log::warn!("Failed to update container on restart: {e}");
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
            restart_servers,
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
                if let Err(e) = startup_sequence(&startup_handle).await {
                    log::error!("Startup failed: {e}");
                    startup_handle.emit("startup-error", e.clone()).ok();
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

                        // Stop existing servers
                        {
                            let mut procs =
                                state.server_processes.lock().unwrap_or_else(|e| e.into_inner());
                            for (name, child) in procs.iter_mut() {
                                log::info!("Stopping {name}...");
                                let _ = child.start_kill();
                            }
                            procs.clear();
                        }

                        // Rebuild prompt
                        let config = state.config.lock().unwrap().clone();
                        let base_url = state.webui_url.lock().unwrap().clone();
                        let jwt = state.jwt_token.lock().unwrap().clone();

                        if let Some(jwt) = jwt {
                            let prompt = prompt::build_system_prompt(&config);
                            let _ = account::push_system_prompt(&base_url, &jwt, &prompt).await;
                            let _ = docker::ensure_running(&config).await;
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
    app.emit("startup-status", "Starting Open WebUI container...")
        .ok();
    docker::ensure_running(&config).await?;
    *state.docker_status.lock().unwrap() = state::DockerStatus::ContainerRunning;

    // 4. Wait for healthy
    app.emit("startup-status", "Waiting for Open WebUI to start...")
        .ok();
    docker::wait_for_healthy(90).await?;
    *state.docker_status.lock().unwrap() = state::DockerStatus::Healthy;

    // 5. Authenticate
    app.emit("startup-status", "Authenticating...").ok();
    let base_url = state.webui_url.lock().unwrap().clone();
    let jwt = account::ensure_authenticated(&base_url).await?;
    *state.jwt_token.lock().unwrap() = Some(jwt.clone());

    // 6. Build + push system prompt
    app.emit("startup-status", "Configuring AI assistant...")
        .ok();
    let system_prompt = prompt::build_system_prompt(&config);
    account::push_system_prompt(&base_url, &jwt, &system_prompt).await?;

    // 7. Start MCP servers
    // Note: In the bundled app, node_bin and servers_dir come from Tauri resources.
    // For now, fall back to system paths for development.
    app.emit("startup-status", "Starting MCP servers...").ok();

    // TODO: Use bundled Node.js and server scripts from Tauri resources
    // For now, log what would be started
    for server in process::SERVERS {
        if process::should_start(server.name, &config) {
            log::info!(
                "Would start {} on port {} (bundled servers not yet configured)",
                server.name,
                server.port
            );
        }
    }

    // 8. Update health
    *state.app_health.lock().unwrap() = state::AppHealth::Healthy;

    app.emit("startup-status", "Ready!").ok();
    app.emit("startup-complete", true).ok();

    log::info!("IBEX startup complete");
    Ok(())
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
