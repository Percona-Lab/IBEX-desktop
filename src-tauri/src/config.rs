//! Read/write ~/.ibex-mcp.env configuration file.
//!
//! Backward-compatible with the terminal-based IBEX configure.sh.
//! Format: KEY=VALUE lines, # comments, blank lines preserved.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// All known IBEX connector credentials.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IbexConfig {
    // Slack
    pub slack_token: Option<String>,

    // Notion
    pub notion_token: Option<String>,

    // Jira
    pub jira_domain: Option<String>,
    pub jira_email: Option<String>,
    pub jira_api_token: Option<String>,

    // Memory (GitHub-backed)
    pub github_token: Option<String>,
    pub github_owner: Option<String>,
    pub github_repo: Option<String>,
    pub github_memory_path: Option<String>,

    // ServiceNow
    pub servicenow_instance: Option<String>,
    pub servicenow_username: Option<String>,
    pub servicenow_password: Option<String>,

    // Salesforce
    pub salesforce_instance_url: Option<String>,
    pub salesforce_access_token: Option<String>,

    // Memory sync (optional)
    pub notion_sync_page_id: Option<String>,
    pub google_doc_id: Option<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub google_refresh_token: Option<String>,

    /// Any unknown KEY=VALUE pairs from the file (preserved on write)
    #[serde(skip)]
    pub extra: HashMap<String, String>,
}

impl IbexConfig {
    /// Path to the env file: ~/.ibex-mcp.env
    pub fn env_file_path() -> PathBuf {
        dirs::home_dir()
            .expect("Cannot determine home directory")
            .join(".ibex-mcp.env")
    }

    /// Load configuration from ~/.ibex-mcp.env.
    /// Returns default config if file doesn't exist.
    pub fn load() -> Self {
        let path = Self::env_file_path();
        if !path.exists() {
            return Self::default();
        }

        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Failed to open {}: {}", path.display(), e);
                return Self::default();
            }
        };

        let mut config = Self::default();
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim().to_string();
                let value_opt = if value.is_empty() {
                    None
                } else {
                    Some(value)
                };

                match key {
                    "SLACK_TOKEN" => config.slack_token = value_opt,
                    "NOTION_TOKEN" => config.notion_token = value_opt,
                    "JIRA_DOMAIN" => config.jira_domain = value_opt,
                    "JIRA_EMAIL" => config.jira_email = value_opt,
                    "JIRA_API_TOKEN" => config.jira_api_token = value_opt,
                    "GITHUB_TOKEN" => config.github_token = value_opt,
                    "GITHUB_OWNER" => config.github_owner = value_opt,
                    "GITHUB_REPO" => config.github_repo = value_opt,
                    "GITHUB_MEMORY_PATH" => config.github_memory_path = value_opt,
                    "SERVICENOW_INSTANCE" => config.servicenow_instance = value_opt,
                    "SERVICENOW_USERNAME" => config.servicenow_username = value_opt,
                    "SERVICENOW_PASSWORD" => config.servicenow_password = value_opt,
                    "SALESFORCE_INSTANCE_URL" => config.salesforce_instance_url = value_opt,
                    "SALESFORCE_ACCESS_TOKEN" => config.salesforce_access_token = value_opt,
                    "NOTION_SYNC_PAGE_ID" => config.notion_sync_page_id = value_opt,
                    "GOOGLE_DOC_ID" => config.google_doc_id = value_opt,
                    "GOOGLE_CLIENT_ID" => config.google_client_id = value_opt,
                    "GOOGLE_CLIENT_SECRET" => config.google_client_secret = value_opt,
                    "GOOGLE_REFRESH_TOKEN" => config.google_refresh_token = value_opt,
                    _ => {
                        if let Some(v) = value_opt {
                            config.extra.insert(key.to_string(), v);
                        }
                    }
                }
            }
        }

        config
    }

    /// Save configuration to ~/.ibex-mcp.env.
    /// Format matches configure.sh output for backward compatibility.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::env_file_path();
        let mut file = fs::File::create(&path)
            .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;

        let now = chrono_lite_now();
        writeln!(file, "# IBEX MCP Server Configuration").ok();
        writeln!(file, "# Last updated: {now}").ok();

        if let Some(ref v) = self.slack_token {
            writeln!(file).ok();
            writeln!(file, "# Slack (user token required for search)").ok();
            writeln!(file, "SLACK_TOKEN={v}").ok();
        }

        if let Some(ref v) = self.notion_token {
            writeln!(file).ok();
            writeln!(file, "# Notion").ok();
            writeln!(file, "NOTION_TOKEN={v}").ok();
        }

        if self.jira_domain.is_some()
            || self.jira_email.is_some()
            || self.jira_api_token.is_some()
        {
            writeln!(file).ok();
            writeln!(file, "# Jira").ok();
            if let Some(ref v) = self.jira_domain {
                writeln!(file, "JIRA_DOMAIN={v}").ok();
            }
            if let Some(ref v) = self.jira_email {
                writeln!(file, "JIRA_EMAIL={v}").ok();
            }
            if let Some(ref v) = self.jira_api_token {
                writeln!(file, "JIRA_API_TOKEN={v}").ok();
            }
        }

        if self.github_token.is_some()
            || self.github_owner.is_some()
            || self.github_repo.is_some()
        {
            writeln!(file).ok();
            writeln!(file, "# Memory (GitHub-backed)").ok();
            if let Some(ref v) = self.github_token {
                writeln!(file, "GITHUB_TOKEN={v}").ok();
            }
            if let Some(ref v) = self.github_owner {
                writeln!(file, "GITHUB_OWNER={v}").ok();
            }
            if let Some(ref v) = self.github_repo {
                writeln!(file, "GITHUB_REPO={v}").ok();
            }
            writeln!(
                file,
                "GITHUB_MEMORY_PATH={}",
                self.github_memory_path
                    .as_deref()
                    .unwrap_or("MEMORY.md")
            )
            .ok();
        }

        if self.servicenow_instance.is_some()
            || self.servicenow_username.is_some()
            || self.servicenow_password.is_some()
        {
            writeln!(file).ok();
            writeln!(file, "# ServiceNow").ok();
            if let Some(ref v) = self.servicenow_instance {
                writeln!(file, "SERVICENOW_INSTANCE={v}").ok();
            }
            if let Some(ref v) = self.servicenow_username {
                writeln!(file, "SERVICENOW_USERNAME={v}").ok();
            }
            if let Some(ref v) = self.servicenow_password {
                writeln!(file, "SERVICENOW_PASSWORD={v}").ok();
            }
        }

        if self.salesforce_instance_url.is_some() || self.salesforce_access_token.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Salesforce").ok();
            if let Some(ref v) = self.salesforce_instance_url {
                writeln!(file, "SALESFORCE_INSTANCE_URL={v}").ok();
            }
            if let Some(ref v) = self.salesforce_access_token {
                writeln!(file, "SALESFORCE_ACCESS_TOKEN={v}").ok();
            }
        }

        // Memory sync settings
        if self.notion_sync_page_id.is_some() || self.google_doc_id.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Memory sync (optional)").ok();
            if let Some(ref v) = self.notion_sync_page_id {
                writeln!(file, "NOTION_SYNC_PAGE_ID={v}").ok();
            }
            if let Some(ref v) = self.google_doc_id {
                writeln!(file, "GOOGLE_DOC_ID={v}").ok();
            }
            if let Some(ref v) = self.google_client_id {
                writeln!(file, "GOOGLE_CLIENT_ID={v}").ok();
            }
            if let Some(ref v) = self.google_client_secret {
                writeln!(file, "GOOGLE_CLIENT_SECRET={v}").ok();
            }
            if let Some(ref v) = self.google_refresh_token {
                writeln!(file, "GOOGLE_REFRESH_TOKEN={v}").ok();
            }
        }

        // Preserve unknown settings
        if !self.extra.is_empty() {
            writeln!(file).ok();
            writeln!(file, "# Additional settings").ok();
            for (k, v) in &self.extra {
                writeln!(file, "{k}={v}").ok();
            }
        }

        writeln!(file).ok();

        // Set permissions to 600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(&path, perms)
                .map_err(|e| format!("Failed to set permissions: {e}"))?;
        }

        Ok(())
    }

    /// Check which connectors are fully configured.
    pub fn is_slack_configured(&self) -> bool {
        self.slack_token.is_some()
    }

    pub fn is_notion_configured(&self) -> bool {
        self.notion_token.is_some()
    }

    pub fn is_jira_configured(&self) -> bool {
        self.jira_domain.is_some()
            && self.jira_email.is_some()
            && self.jira_api_token.is_some()
    }

    pub fn is_memory_configured(&self) -> bool {
        self.github_token.is_some()
            && self.github_owner.is_some()
            && self.github_repo.is_some()
    }

    pub fn is_servicenow_configured(&self) -> bool {
        self.servicenow_instance.is_some()
            && self.servicenow_username.is_some()
            && self.servicenow_password.is_some()
    }

    pub fn is_salesforce_configured(&self) -> bool {
        self.salesforce_instance_url.is_some() && self.salesforce_access_token.is_some()
    }

    /// List of configured connector names (for display).
    pub fn configured_connectors(&self) -> Vec<&str> {
        let mut list = Vec::new();
        if self.is_slack_configured() {
            list.push("Slack");
        }
        if self.is_notion_configured() {
            list.push("Notion");
        }
        if self.is_jira_configured() {
            list.push("Jira");
        }
        if self.is_servicenow_configured() {
            list.push("ServiceNow");
        }
        if self.is_salesforce_configured() {
            list.push("Salesforce");
        }
        if self.is_memory_configured() {
            list.push("Memory");
        }
        list
    }
}

/// Simple timestamp without pulling in chrono.
fn chrono_lite_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Basic formatting: seconds since epoch → readable
    // We'll just use the Unix timestamp; the bash script uses `date '+%Y-%m-%d %H:%M:%S'`
    // but for config file comments this is fine
    format!("{now}")
}
