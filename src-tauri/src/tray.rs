//! Menu bar (system tray) icon and status display.
//!
//! Shows: green (healthy), yellow (degraded), red (error).
//! Menu: Docker status, each server + port, Open WebUI link, Settings, Quit.

use crate::state::{AppHealth, AppState};
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager};

/// Create the system tray icon and menu.
pub fn setup_tray(app: &AppHandle) -> Result<TrayIcon, String> {
    // Load ibex icon for menu bar (embedded at compile time)
    let icon = Image::from_bytes(include_bytes!("../icons/tray-icon@2x.png"))
        .map_err(|e| format!("Failed to load tray icon: {e}"))?;

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true) // macOS auto-adjusts for light/dark mode
        .tooltip("IBEX — Starting...")
        .on_tray_icon_event(|tray_icon, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                // Left click → show/focus main window
                show_main_window(tray_icon.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .build(app)
        .map_err(|e| format!("Failed to create tray: {e}"))?;

    // Set initial menu
    update_tray_menu(app, &tray)?;

    Ok(tray)
}

/// Handle tray menu item clicks.
fn handle_menu_event(app: &AppHandle, menu_id: &str) {
    match menu_id {
        "open_webui" => {
            show_main_window(app);
        }
        "connectors" => {
            open_connectors_window(app);
        }
        "settings" => {
            open_settings_window(app);
        }
        "restart" => {
            // Emit event that lib.rs restart handler will pick up
            app.emit("tray-restart-servers", ()).ok();
            log::info!("Restart servers requested from tray menu");
        }
        "quit" => {
            log::info!("Quit requested from tray menu");
            app.exit(0);
        }
        _ => {}
    }
}

/// Show and focus the main window.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Open the connectors/setup wizard window, or focus it if already open.
fn open_connectors_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("connectors") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        match tauri::WebviewWindowBuilder::new(
            app,
            "connectors",
            tauri::WebviewUrl::App("/setup".into()),
        )
        .title("IBEX — Connectors")
        .inner_size(640.0, 700.0)
        .min_inner_size(500.0, 400.0)
        .resizable(true)
        .minimizable(true)
        .build()
        {
            Ok(_) => {
                log::info!("Connectors window opened");
            }
            Err(e) => {
                log::error!("Failed to open connectors window: {e}");
            }
        }
    }
}

/// Open the settings window, or focus it if already open.
fn open_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        // Settings window already exists — focus it
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        // Create settings window
        match tauri::WebviewWindowBuilder::new(app, "settings", tauri::WebviewUrl::App("/settings".into()))
            .title("IBEX Settings")
            .inner_size(640.0, 700.0)
            .min_inner_size(500.0, 400.0)
            .resizable(true)
            .minimizable(true)
            .build()
        {
            Ok(_) => {
                log::info!("Settings window opened");
            }
            Err(e) => {
                log::error!("Failed to open settings window: {e}");
            }
        }
    }
}

/// Update the tray menu and tooltip based on current state.
pub fn update_tray_menu(app: &AppHandle, tray: &TrayIcon) -> Result<(), String> {
    let state = app.state::<AppState>();
    let health = state.compute_health();

    // Update tooltip
    let tooltip = match health {
        AppHealth::Healthy => "IBEX — All systems running",
        AppHealth::Degraded => "IBEX — Some services degraded",
        AppHealth::Error => "IBEX — Error",
        AppHealth::Starting => "IBEX — Starting...",
    };
    tray.set_tooltip(Some(tooltip))
        .map_err(|e| format!("Failed to set tooltip: {e}"))?;

    // Build menu
    let docker_status = state.docker_status.lock().unwrap().clone();
    let servers = state.server_statuses.lock().unwrap().clone();

    let mut builder = MenuBuilder::new(app);

    // Docker status
    let docker_label = format!(
        "Docker: {}",
        match docker_status {
            crate::state::DockerStatus::Healthy => "Running \u{2713}",
            crate::state::DockerStatus::ContainerRunning => "Starting\u{2026}",
            crate::state::DockerStatus::ContainerStopped => "Stopped",
            crate::state::DockerStatus::ContainerMissing => "Not created",
            crate::state::DockerStatus::NotRunning => "Not running",
            crate::state::DockerStatus::NotInstalled => "Not installed",
        }
    );
    builder = builder.item(
        &MenuItemBuilder::with_id("docker_status", &docker_label)
            .enabled(false)
            .build(app)
            .map_err(|e| format!("Menu error: {e}"))?,
    );

    builder = builder.separator();

    // Server statuses
    if servers.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("no_servers", "No servers configured")
                .enabled(false)
                .build(app)
                .map_err(|e| format!("Menu error: {e}"))?,
        );
    } else {
        for (name, status) in &servers {
            let icon = if status.healthy {
                "\u{2713}"
            } else if status.running {
                "\u{2026}"
            } else {
                "\u{2717}"
            };
            let label = format!("{icon} {name} (:{port})", name = name, port = status.port);
            builder = builder.item(
                &MenuItemBuilder::with_id(format!("server_{name}"), &label)
                    .enabled(false)
                    .build(app)
                    .map_err(|e| format!("Menu error: {e}"))?,
            );
        }
    }

    builder = builder.separator();

    // Actions
    builder = builder.item(
        &MenuItemBuilder::with_id("open_webui", "Open IBEX")
            .build(app)
            .map_err(|e| format!("Menu error: {e}"))?,
    );
    builder = builder.item(
        &MenuItemBuilder::with_id("connectors", "Connectors\u{2026}")
            .build(app)
            .map_err(|e| format!("Menu error: {e}"))?,
    );

    builder = builder.separator();

    builder = builder.item(
        &MenuItemBuilder::with_id("restart", "Restart Servers")
            .build(app)
            .map_err(|e| format!("Menu error: {e}"))?,
    );
    builder = builder.item(
        &MenuItemBuilder::with_id("quit", "Quit IBEX")
            .build(app)
            .map_err(|e| format!("Menu error: {e}"))?,
    );

    let menu = builder.build().map_err(|e| format!("Failed to build menu: {e}"))?;
    tray.set_menu(Some(menu))
        .map_err(|e| format!("Failed to set menu: {e}"))?;

    Ok(())
}
