//! Auto-account creation and authentication for Open WebUI.
//!
//! On first launch: creates an admin account with generated credentials → Keychain.
//! On subsequent launches: signs in with stored credentials → fresh JWT.

use crate::keychain;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// Open WebUI auth response.
#[derive(Debug, Deserialize)]
struct AuthResponse {
    token: Option<String>,
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    email: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    role: Option<String>,
    // Error case
    detail: Option<String>,
}

/// Sign-up request body.
#[derive(Serialize)]
struct SignUpRequest {
    email: String,
    password: String,
    name: String,
}

/// Sign-in request body.
#[derive(Serialize)]
struct SignInRequest {
    email: String,
    password: String,
}

/// Generate a random password (24 chars, alphanumeric + symbols).
fn generate_password() -> String {
    let charset = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*";
    let mut rng = rand::thread_rng();
    (0..24)
        .map(|_| {
            let idx = rng.gen_range(0..charset.len());
            charset[idx] as char
        })
        .collect()
}

/// Ensure an admin account exists and return a valid JWT.
///
/// Flow:
/// 1. Check Keychain for existing credentials
/// 2. If found → sign in → return JWT
/// 3. If not found → create account → store in Keychain → return JWT
pub async fn ensure_authenticated(base_url: &str) -> Result<String, String> {
    let client = reqwest::Client::new();

    // Check for existing credentials
    if let Some(creds) = keychain::get_credentials() {
        log::info!("Found existing credentials, signing in...");

        match sign_in(&client, base_url, &creds.email, &creds.password).await {
            Ok(jwt) => {
                keychain::set_jwt(&jwt).ok();
                return Ok(jwt);
            }
            Err(e) => {
                log::warn!("Sign-in failed with stored creds: {e}. Will try creating new account.");
            }
        }
    }

    // No credentials or sign-in failed — create new account
    log::info!("Creating new admin account...");

    let email = "ibex-admin@localhost".to_string();
    let password = generate_password();
    let name = "IBEX Admin".to_string();

    let jwt = sign_up(&client, base_url, &email, &password, &name).await?;

    // Store in Keychain
    keychain::set_credentials(&keychain::AdminCredentials {
        email: email.clone(),
        password,
        jwt: Some(jwt.clone()),
    })?;

    log::info!("Admin account created and credentials stored in Keychain");
    Ok(jwt)
}

/// Sign in to Open WebUI.
async fn sign_in(
    client: &reqwest::Client,
    base_url: &str,
    email: &str,
    password: &str,
) -> Result<String, String> {
    let url = format!("{base_url}/api/v1/auths/signin");

    let resp = client
        .post(&url)
        .json(&SignInRequest {
            email: email.to_string(),
            password: password.to_string(),
        })
        .send()
        .await
        .map_err(|e| format!("Sign-in request failed: {e}"))?;

    let auth: AuthResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse sign-in response: {e}"))?;

    if let Some(detail) = auth.detail {
        return Err(format!("Sign-in error: {detail}"));
    }

    auth.token
        .ok_or_else(|| "No token in sign-in response".to_string())
}

/// Create a new account on Open WebUI.
async fn sign_up(
    client: &reqwest::Client,
    base_url: &str,
    email: &str,
    password: &str,
    name: &str,
) -> Result<String, String> {
    let url = format!("{base_url}/api/v1/auths/signup");

    let resp = client
        .post(&url)
        .json(&SignUpRequest {
            email: email.to_string(),
            password: password.to_string(),
            name: name.to_string(),
        })
        .send()
        .await
        .map_err(|e| format!("Sign-up request failed: {e}"))?;

    let auth: AuthResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse sign-up response: {e}"))?;

    if let Some(detail) = auth.detail {
        return Err(format!("Sign-up error: {detail}"));
    }

    auth.token
        .ok_or_else(|| "No token in sign-up response".to_string())
}

/// Push system prompt to Open WebUI via user settings API.
pub async fn push_system_prompt(
    base_url: &str,
    jwt: &str,
    system_prompt: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    // First, get existing settings to avoid overwriting
    let settings_url = format!("{base_url}/api/v1/users/user/settings");
    let existing: serde_json::Value = match client
        .get(&settings_url)
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
    {
        Ok(resp) => resp.json().await.unwrap_or(serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };

    // Merge system prompt into existing settings
    let mut settings = match existing {
        serde_json::Value::Object(map) => serde_json::Value::Object(map),
        _ => serde_json::json!({}),
    };

    if let serde_json::Value::Object(ref mut map) = settings {
        map.insert(
            "system".to_string(),
            serde_json::Value::String(system_prompt.to_string()),
        );
    }

    // Push updated settings
    let update_url = format!("{base_url}/api/v1/users/user/settings/update");
    let resp = client
        .post(&update_url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .json(&settings)
        .send()
        .await
        .map_err(|e| format!("Failed to push system prompt: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Push system prompt failed ({status}): {body}"));
    }

    log::info!("System prompt pushed to Open WebUI");
    Ok(())
}
