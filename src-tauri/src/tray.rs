//! Menu bar (system tray) icon and status display.
//!
//! Shows: green (healthy), yellow (degraded), red (error).
//! Menu: Docker status, each server + port, Open WebUI link, Settings, Quit.

use crate::state::{AppHealth, AppState};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};

/// Create the system tray icon and menu.
pub fn setup_tray(app: &AppHandle) -> Result<TrayIcon, String> {
    let tray = TrayIconBuilder::new()
        .title("IBEX")
        .tooltip("IBEX — Starting...")
        .on_tray_icon_event(|tray_icon, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                // Left click → show/focus main window
                if let Some(window) = tray_icon.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)
        .map_err(|e| format!("Failed to create tray: {e}"))?;

    // Set initial menu
    update_tray_menu(app, &tray)?;

    Ok(tray)
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

    // Update title (shown next to icon in macOS menu bar)
    let title = match health {
        AppHealth::Healthy => "IBEX",
        AppHealth::Degraded => "IBEX ⚠",
        AppHealth::Error => "IBEX ✗",
        AppHealth::Starting => "IBEX …",
    };
    tray.set_title(Some(title))
        .map_err(|e| format!("Failed to set title: {e}"))?;

    // Build menu
    let docker_status = state.docker_status.lock().unwrap().clone();
    let servers = state.server_statuses.lock().unwrap().clone();

    let mut builder = MenuBuilder::new(app);

    // Docker status
    let docker_label = format!(
        "Docker: {}",
        match docker_status {
            crate::state::DockerStatus::Healthy => "Running ✓",
            crate::state::DockerStatus::ContainerRunning => "Starting…",
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
                "✓"
            } else if status.running {
                "…"
            } else {
                "✗"
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
        &MenuItemBuilder::with_id("settings", "Settings…")
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
