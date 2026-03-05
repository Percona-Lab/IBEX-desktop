//! Shared application state accessible from Tauri commands and background tasks.

use crate::config::IbexConfig;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::process::Child;

/// Overall Docker status.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum DockerStatus {
    NotInstalled,
    NotRunning,
    ContainerMissing,
    ContainerStopped,
    ContainerRunning,
    Healthy,
}

/// Status of an individual MCP server.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerStatus {
    pub name: String,
    pub port: u16,
    pub running: bool,
    pub healthy: bool,
}

/// Overall app health for menu bar icon.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AppHealth {
    /// All systems go (green icon)
    Healthy,
    /// Some servers down or Docker issues (yellow icon)
    Degraded,
    /// Major failure (red icon)
    Error,
    /// Still starting up
    Starting,
}

/// Managed state shared across the Tauri app.
pub struct AppState {
    /// IBEX connector configuration (~/.ibex-mcp.env)
    pub config: Mutex<IbexConfig>,

    /// Docker container status
    pub docker_status: Mutex<DockerStatus>,

    /// Running MCP server processes (name → child handle)
    pub server_processes: Mutex<HashMap<String, Child>>,

    /// Server health status (name → status)
    pub server_statuses: Mutex<HashMap<String, ServerStatus>>,

    /// Overall app health
    pub app_health: Mutex<AppHealth>,

    /// JWT token for Open WebUI API
    pub jwt_token: Mutex<Option<String>>,

    /// Open WebUI base URL
    pub webui_url: Mutex<String>,
}

impl AppState {
    pub fn new() -> Self {
        let config = IbexConfig::load();

        Self {
            config: Mutex::new(config),
            docker_status: Mutex::new(DockerStatus::ContainerMissing),
            server_processes: Mutex::new(HashMap::new()),
            server_statuses: Mutex::new(HashMap::new()),
            app_health: Mutex::new(AppHealth::Starting),
            jwt_token: Mutex::new(None),
            webui_url: Mutex::new("http://localhost:8080".to_string()),
        }
    }

    /// Reload config from disk (e.g., after terminal configure.sh changes it).
    pub fn reload_config(&self) {
        let new_config = IbexConfig::load();
        if let Ok(mut cfg) = self.config.lock() {
            *cfg = new_config;
        }
    }

    /// Get current app health based on Docker + server statuses.
    pub fn compute_health(&self) -> AppHealth {
        let docker = self.docker_status.lock().unwrap().clone();
        let servers = self.server_statuses.lock().unwrap();

        if docker != DockerStatus::Healthy {
            return AppHealth::Error;
        }

        if servers.is_empty() {
            // No servers configured — that's okay, Docker is healthy
            return AppHealth::Healthy;
        }

        let all_healthy = servers.values().all(|s| s.healthy);
        let any_running = servers.values().any(|s| s.running);

        if all_healthy {
            AppHealth::Healthy
        } else if any_running {
            AppHealth::Degraded
        } else {
            AppHealth::Error
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
