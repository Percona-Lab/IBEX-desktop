//! macOS Keychain integration for encrypting connector credentials.
//!
//! All sensitive credentials (tokens, passwords, API secrets) are stored as a
//! **single JSON blob** in one macOS Keychain entry. This design means macOS
//! only prompts for Keychain access once (not per-credential), and "Always
//! Allow" covers all secrets with a single approval.
//!
//! Service: "com.percona.ibex"  ·  Account: "connector_secrets"

use crate::config::IbexConfig;
use keyring::Entry;
use std::collections::HashMap;

const SERVICE: &str = "com.percona.ibex";
const ACCOUNT: &str = "connector_secrets";

// ── Core Keychain Operations (single entry) ──

/// Read the secrets JSON blob from the Keychain.
/// Returns an empty map if no entry exists.
fn read_blob() -> HashMap<String, String> {
    let entry = match Entry::new(SERVICE, ACCOUNT) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("Keychain entry error: {e}");
            return HashMap::new();
        }
    };

    match entry.get_password() {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(keyring::Error::NoEntry) => HashMap::new(),
        Err(e) => {
            log::warn!("Failed to read Keychain: {e}");
            HashMap::new()
        }
    }
}

/// Write the secrets JSON blob to the Keychain.
fn write_blob(secrets: &HashMap<String, String>) -> Result<(), String> {
    let entry = Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| format!("Keychain entry error: {e}"))?;

    if secrets.is_empty() {
        // Nothing to store — delete the entry
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Failed to delete Keychain entry: {e}")),
        }
    } else {
        let json = serde_json::to_string(secrets)
            .map_err(|e| format!("Failed to serialize secrets: {e}"))?;
        entry
            .set_password(&json)
            .map_err(|e| format!("Failed to write Keychain: {e}"))
    }
}

// ── Bulk Operations for IbexConfig ──

/// Load all secrets from the Keychain into an IbexConfig.
///
/// Secrets found in the Keychain take precedence over any values already in the
/// config (which may have been loaded from the .env file).
pub fn load_secrets(config: &mut IbexConfig) {
    let secrets = read_blob();
    if secrets.is_empty() {
        return;
    }

    if let Some(v) = secrets.get("slack_token") {
        config.slack_token = Some(v.clone());
    }
    if let Some(v) = secrets.get("notion_token") {
        config.notion_token = Some(v.clone());
    }
    if let Some(v) = secrets.get("jira_api_token") {
        config.jira_api_token = Some(v.clone());
    }
    if let Some(v) = secrets.get("github_token") {
        config.github_token = Some(v.clone());
    }
    if let Some(v) = secrets.get("servicenow_password") {
        config.servicenow_password = Some(v.clone());
    }
    if let Some(v) = secrets.get("salesforce_password") {
        config.salesforce_password = Some(v.clone());
    }
    if let Some(v) = secrets.get("salesforce_security_token") {
        config.salesforce_security_token = Some(v.clone());
    }
    if let Some(v) = secrets.get("google_client_secret") {
        config.google_client_secret = Some(v.clone());
    }
    if let Some(v) = secrets.get("google_refresh_token") {
        config.google_refresh_token = Some(v.clone());
    }
}

/// Store all secrets from an IbexConfig into the Keychain as a single entry.
///
/// Only non-empty values are stored. If a credential was removed (set to None),
/// it is omitted from the blob.
pub fn save_secrets(config: &IbexConfig) -> Result<(), String> {
    let mut secrets = HashMap::new();

    insert_if_some(&mut secrets, "slack_token", &config.slack_token);
    insert_if_some(&mut secrets, "notion_token", &config.notion_token);
    insert_if_some(&mut secrets, "jira_api_token", &config.jira_api_token);
    insert_if_some(&mut secrets, "github_token", &config.github_token);
    insert_if_some(&mut secrets, "servicenow_password", &config.servicenow_password);
    insert_if_some(&mut secrets, "salesforce_password", &config.salesforce_password);
    insert_if_some(
        &mut secrets,
        "salesforce_security_token",
        &config.salesforce_security_token,
    );
    insert_if_some(&mut secrets, "google_client_secret", &config.google_client_secret);
    insert_if_some(&mut secrets, "google_refresh_token", &config.google_refresh_token);

    write_blob(&secrets)
}

/// Delete all stored secrets (used for reset/uninstall).
pub fn clear_all_secrets() -> Result<(), String> {
    write_blob(&HashMap::new())
}

// ── Migration Helper ──

/// Clean up old per-key Keychain entries from the previous implementation.
/// Call this once after migrating to the single-blob format.
pub fn cleanup_legacy_entries() {
    let legacy_keys = [
        "slack_token",
        "notion_token",
        "jira_api_token",
        "github_token",
        "servicenow_password",
        "salesforce_password",
        "salesforce_security_token",
        "google_client_secret",
        "google_refresh_token",
        // Old admin credential keys (from original keychain.rs)
        "admin_email",
        "admin_password",
        "jwt_token",
    ];

    for key in &legacy_keys {
        if let Ok(entry) = Entry::new(SERVICE, key) {
            match entry.delete_credential() {
                Ok(()) => log::debug!("Cleaned up legacy Keychain entry: {key}"),
                Err(keyring::Error::NoEntry) => {} // Already gone
                Err(e) => log::debug!("Could not clean legacy entry {key}: {e}"),
            }
        }
    }
}

// ── Private Helpers ──

fn insert_if_some(map: &mut HashMap<String, String>, key: &str, value: &Option<String>) {
    if let Some(ref v) = value {
        if !v.is_empty() {
            map.insert(key.to_string(), v.clone());
        }
    }
}
