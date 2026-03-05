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
use tauri::{Emitter, Manager, RunEvent};

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
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Setup system tray
            match tray::setup_tray(&handle) {
                Ok(_tray) => {
                    log::info!("System tray initialized");
                }
                Err(e) => {
                    log::error!("Failed to setup tray: {e}");
                }
            }

            // Spawn background startup sequence
            let startup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = startup_sequence(&startup_handle).await {
                    log::error!("Startup failed: {e}");
                    startup_handle.emit("startup-error", e.clone()).ok();
                }
            });

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
            if let RunEvent::ExitRequested { .. } = event {
                // Graceful shutdown: kill MCP servers
                let state = app_handle.state::<AppState>();
                let mut processes = match state.server_processes.lock() {
                    Ok(p) => p,
                    Err(_) => return,
                };
                for (name, child) in processes.iter_mut() {
                    log::info!("Shutting down {name}...");
                    // Use start_kill() for sync context
                    let _ = child.start_kill();
                }
                processes.clear();
                drop(processes);
                // Note: Docker container intentionally left running for fast restart
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
