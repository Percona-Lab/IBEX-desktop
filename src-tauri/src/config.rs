//! Read/write IBEX connector configuration.
//!
//! Sensitive credentials (tokens, passwords, API secrets) are stored encrypted
//! in the macOS Keychain via the `keychain` module. Non-sensitive values
//! (domains, usernames, instance URLs) remain in ~/.ibex-mcp.env.
//!
//! On load, secrets from the Keychain take precedence. If secrets are found in
//! the .env file (legacy/migration), they are automatically migrated to the
//! Keychain and removed from the .env file.
//!
//! Backward-compatible with the terminal-based IBEX configure.sh.

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
    pub salesforce_username: Option<String>,
    pub salesforce_password: Option<String>,
    pub salesforce_security_token: Option<String>,

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

    /// Load configuration from ~/.ibex-mcp.env and macOS Keychain.
    ///
    /// Non-sensitive values are read from the .env file.
    /// Sensitive credentials are read from the macOS Keychain.
    ///
    /// **Migration:** If secrets are found in the .env file (legacy format),
    /// they are automatically migrated to the Keychain and stripped from the
    /// .env file. This handles upgrades and `configure.sh` backward compat.
    pub fn load() -> Self {
        let path = Self::env_file_path();

        let mut config = Self::default();

        // 1. Load from .env file (non-sensitive + possibly legacy secrets)
        if path.exists() {
            let file = match fs::File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    log::warn!("Failed to open {}: {}", path.display(), e);
                    // Still try Keychain for secrets
                    crate::keychain::load_secrets(&mut config);
                    return config;
                }
            };

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
                        "SALESFORCE_USERNAME" => config.salesforce_username = value_opt,
                        "SALESFORCE_PASSWORD" => config.salesforce_password = value_opt,
                        "SALESFORCE_SECURITY_TOKEN" => {
                            config.salesforce_security_token = value_opt
                        }
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
        }

        // 2. Check if any secrets were found in .env (need migration)
        let env_had_secrets = config.slack_token.is_some()
            || config.notion_token.is_some()
            || config.jira_api_token.is_some()
            || config.github_token.is_some()
            || config.servicenow_password.is_some()
            || config.salesforce_password.is_some()
            || config.salesforce_security_token.is_some()
            || config.google_client_secret.is_some()
            || config.google_refresh_token.is_some();

        // 3. Load secrets from Keychain (overrides .env values)
        crate::keychain::load_secrets(&mut config);

        // 4. Auto-migrate: if .env had secrets, move them to Keychain
        if env_had_secrets {
            log::info!("Migrating credentials from .env to macOS Keychain...");
            if let Err(e) = crate::keychain::save_secrets(&config) {
                log::error!("Failed to migrate secrets to Keychain: {e}");
                // Don't strip .env — migration failed, keep secrets where they are
                return config;
            }
            // Re-save .env without secrets (save_env_only writes non-sensitive only)
            if let Err(e) = config.save_env_only() {
                log::warn!("Failed to clean secrets from .env: {e}");
            } else {
                log::info!("Credentials migrated to macOS Keychain — .env cleaned");
            }
        }

        // 5. Clean up old per-key Keychain entries (from previous implementation)
        crate::keychain::cleanup_legacy_entries();

        config
    }

    /// Save configuration: secrets to macOS Keychain, non-sensitive to .env.
    ///
    /// Sensitive credentials are encrypted in the Keychain.
    /// The .env file only contains non-sensitive values (domains, usernames, URLs).
    pub fn save(&self) -> Result<(), String> {
        // 1. Store secrets in macOS Keychain (encrypted)
        crate::keychain::save_secrets(self)?;

        // 2. Write non-sensitive values to .env file
        self.save_env_only()
    }

    /// Write only non-sensitive configuration to ~/.ibex-mcp.env.
    ///
    /// Secrets (tokens, passwords, API keys) are NEVER written to this file.
    /// They are stored exclusively in the macOS Keychain.
    fn save_env_only(&self) -> Result<(), String> {
        let path = Self::env_file_path();
        let mut file = fs::File::create(&path)
            .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;

        let now = chrono_lite_now();
        writeln!(file, "# IBEX MCP Server Configuration").ok();
        writeln!(file, "# Last updated: {now}").ok();
        writeln!(file, "# Sensitive credentials are stored in macOS Keychain").ok();

        // Slack: token is a secret, so only write a marker if configured
        if self.slack_token.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Slack (token stored in Keychain)").ok();
        }

        // Notion: token is a secret, so only write a marker if configured
        if self.notion_token.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Notion (token stored in Keychain)").ok();
        }

        // Jira: domain and email are non-sensitive, api_token is a secret
        if self.jira_domain.is_some() || self.jira_email.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Jira (API token stored in Keychain)").ok();
            if let Some(ref v) = self.jira_domain {
                writeln!(file, "JIRA_DOMAIN={v}").ok();
            }
            if let Some(ref v) = self.jira_email {
                writeln!(file, "JIRA_EMAIL={v}").ok();
            }
        }

        // Memory (GitHub): owner, repo, path are non-sensitive; token is a secret
        if self.github_owner.is_some() || self.github_repo.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Memory (GitHub token stored in Keychain)").ok();
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

        // ServiceNow: instance and username are non-sensitive; password is a secret
        if self.servicenow_instance.is_some() || self.servicenow_username.is_some() {
            writeln!(file).ok();
            writeln!(file, "# ServiceNow (password stored in Keychain)").ok();
            if let Some(ref v) = self.servicenow_instance {
                writeln!(file, "SERVICENOW_INSTANCE={v}").ok();
            }
            if let Some(ref v) = self.servicenow_username {
                writeln!(file, "SERVICENOW_USERNAME={v}").ok();
            }
        }

        // Salesforce: instance_url and username are non-sensitive; password + security_token are secrets
        if self.salesforce_instance_url.is_some() || self.salesforce_username.is_some() {
            writeln!(file).ok();
            writeln!(file, "# Salesforce (password & token stored in Keychain)").ok();
            if let Some(ref v) = self.salesforce_instance_url {
                writeln!(file, "SALESFORCE_INSTANCE_URL={v}").ok();
            }
            if let Some(ref v) = self.salesforce_username {
                writeln!(file, "SALESFORCE_USERNAME={v}").ok();
            }
        }

        // Memory sync: page_id, doc_id, client_id are non-sensitive; client_secret + refresh_token are secrets
        if self.notion_sync_page_id.is_some()
            || self.google_doc_id.is_some()
            || self.google_client_id.is_some()
        {
            writeln!(file).ok();
            writeln!(file, "# Memory sync (secrets stored in Keychain)").ok();
            if let Some(ref v) = self.notion_sync_page_id {
                writeln!(file, "NOTION_SYNC_PAGE_ID={v}").ok();
            }
            if let Some(ref v) = self.google_doc_id {
                writeln!(file, "GOOGLE_DOC_ID={v}").ok();
            }
            if let Some(ref v) = self.google_client_id {
                writeln!(file, "GOOGLE_CLIENT_ID={v}").ok();
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
        self.salesforce_instance_url.is_some()
            && self.salesforce_username.is_some()
            && self.salesforce_password.is_some()
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
