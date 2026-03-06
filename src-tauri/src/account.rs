//! Auto-account creation and authentication for Open WebUI.
//!
//! With WEBUI_AUTH=false, users never see a login page.
//! We still create an admin account for API access (push system prompt, etc.).
//! No Keychain usage — avoids macOS password prompts.

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

/// User update request body.
#[derive(Serialize)]
struct UserUpdateRequest {
    name: String,
    email: String,
    role: String,
    profile_image_url: String,
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

/// Obtain a JWT token for Open WebUI API access.
///
/// With WEBUI_AUTH=false, Open WebUI auto-creates a default admin user with
/// email "admin@localhost". We sign in with those default credentials, then
/// update the display name to "IBEX" so the UI shows the correct identity.
///
/// Tries: sign in → sign up (first run) → retry with backoff.
/// On first launch, Open WebUI may still be initializing its database when we
/// attempt auth, causing both sign-in and sign-up to fail. We retry up to 5
/// times with exponential backoff (2s, 4s, 8s, 16s, 32s = ~62s total) to
/// give it time to finish startup. This matches the 90s healthy wait timeout.
///
/// No Keychain access — avoids macOS password prompts.
pub async fn ensure_authenticated(base_url: &str) -> Result<Option<String>, String> {
    let client = reqwest::Client::new();

    // IMPORTANT: Use the default email that WEBUI_AUTH=false creates.
    // Do NOT change this — Open WebUI's auth system expects "admin@localhost"
    // when auth is disabled. Changing it breaks sign-in on next restart.
    let email = "admin@localhost";
    let password = "ibex-desktop-local";
    let display_name = "IBEX";

    let max_retries = 5;
    let mut last_error = String::from("unknown error");

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(2u64.pow(attempt as u32));
            log::info!(
                "Auth attempt {}/{} — retrying in {}s...",
                attempt + 1,
                max_retries + 1,
                delay.as_secs()
            );
            tokio::time::sleep(delay).await;
        }

        // Try sign-in first (account may already exist from a previous launch)
        match sign_in(&client, base_url, email, password).await {
            Ok(jwt) => {
                log::info!("Signed in to Open WebUI for API access");
                // Update display name from default "User" to "IBEX".
                if let Err(e) =
                    update_user_profile(&client, base_url, &jwt, display_name).await
                {
                    log::warn!("Could not update user profile (non-fatal): {e}");
                }
                return Ok(Some(jwt));
            }
            Err(signin_err) => {
                log::info!("Sign-in failed (attempt {}): {signin_err}", attempt + 1);

                // Try creating the account (first run)
                match sign_up(&client, base_url, email, password, display_name).await {
                    Ok(jwt) => {
                        log::info!("Created admin account for API access");
                        // Update display name
                        if let Err(e) =
                            update_user_profile(&client, base_url, &jwt, display_name).await
                        {
                            log::warn!("Could not update user profile (non-fatal): {e}");
                        }
                        return Ok(Some(jwt));
                    }
                    Err(signup_err) => {
                        log::info!("Sign-up also failed (attempt {}): {signup_err}", attempt + 1);
                        last_error = format!("sign-in: {signin_err}; sign-up: {signup_err}");
                    }
                }
            }
        }
    }

    // All retries exhausted — non-fatal since auth is disabled
    log::warn!(
        "Could not obtain API token after {} attempts (non-fatal): {last_error}",
        max_retries + 1
    );
    Ok(None)
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

/// IBEX icon as a base64 data URI (96×96 PNG).
/// Embedded directly so it works regardless of URL origin.
const IBEX_ICON_DATA_URI: &str = "data:image/png;base64,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";

/// Update the user's display name and profile image in Open WebUI.
///
/// Uses the self-service auth profile endpoint (same as the frontend uses)
/// to ensure changes are reflected in getSessionUser / $user store.
/// With WEBUI_AUTH=false, the auto-created user has name="User". We update
/// it to "IBEX" with the ibex icon so the UI displays the correct identity.
async fn update_user_profile(
    client: &reqwest::Client,
    base_url: &str,
    jwt: &str,
    name: &str,
) -> Result<(), String> {
    // Use the self-service profile update endpoint — this is the same endpoint
    // the frontend uses (POST /api/v1/auths/update/profile). The admin endpoint
    // (/api/v1/users/{id}/update) updates a different record that getSessionUser
    // may not read from.
    let update_url = format!("{base_url}/api/v1/auths/update/profile");
    let body = serde_json::json!({
        "name": name,
        "profile_image_url": IBEX_ICON_DATA_URI,
    });

    let resp = client
        .post(&update_url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to update user profile: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Update user profile failed ({status}): {body}"));
    }

    log::info!("Updated user profile: name={name}, icon=ibex");
    Ok(())
}

/// Push system prompt and default model to Open WebUI via user settings API.
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
        // Top-level "system" — used by Open WebUI backend for chat completions
        map.insert(
            "system".to_string(),
            serde_json::Value::String(system_prompt.to_string()),
        );

        // Also inside "ui" — used by the frontend Settings UI to display the prompt.
        // Open WebUI stores UI preferences under "ui" and the frontend reads
        // $settings.system from userSettings.ui, not the top level.
        let ui = map
            .entry("ui".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(ui_obj) = ui.as_object_mut() {
            ui_obj.insert(
                "system".to_string(),
                serde_json::Value::String(system_prompt.to_string()),
            );

            // Explicitly disable Open WebUI's built-in memory feature.
            // IBEX has its own GitHub-backed memory system via the memory MCP server.
            // The built-in memory triggers queryMemory() on every chat, which shows
            // "No memories found" toast errors on fresh installs. Disabling it
            // prevents these errors and avoids confusion with IBEX's memory.
            ui_obj.insert(
                "memory".to_string(),
                serde_json::Value::Bool(false),
            );
        }
    }

    // Auto-configure default model.
    //
    // Three cases:
    //   1. No model set → full discovery with retries + fallback to first model.
    //   2. Model set but NOT preferred → quick check for preferred; upgrade if found.
    //   3. Model set and IS preferred → no-op.
    //
    // Case 2 handles the common first-run race: startup_sequence() runs before
    // LLM backends finish loading, discovers a non-preferred model (e.g.
    // qwen3-coder-30b). When restart_servers() calls us again later, we
    // upgrade to the preferred model (e.g. qwen3.5-35b) if it's now available.
    if let serde_json::Value::Object(ref mut map) = settings {
        let current_models = map
            .get("models")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let has_models = !current_models.is_empty();

        let current_is_preferred = current_models
            .first()
            .and_then(|m| m.as_str())
            .map(|id| {
                let lower = id.to_lowercase();
                PREFERRED_MODEL_PATTERNS.iter().any(|p| lower.contains(p))
            })
            .unwrap_or(false);

        if !has_models {
            // Case 1: No model — full discovery with retries
            if let Some(model_id) = discover_first_model(&client, base_url, jwt).await {
                log::info!("Setting default model: {model_id}");
                map.insert(
                    "models".to_string(),
                    serde_json::json!([model_id]),
                );
            }
        } else if !current_is_preferred {
            // Case 2: Non-preferred model set — quick check for preferred
            if let Some(preferred_id) = discover_preferred_model(&client, base_url, jwt).await {
                log::info!(
                    "Upgrading default model from '{}' to preferred: {preferred_id}",
                    current_models.first().and_then(|m| m.as_str()).unwrap_or("?")
                );
                map.insert(
                    "models".to_string(),
                    serde_json::json!([preferred_id]),
                );
            } else {
                log::info!(
                    "Keeping non-preferred model '{}' — preferred not available yet",
                    current_models.first().and_then(|m| m.as_str()).unwrap_or("?")
                );
            }
        }
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
        .map_err(|e| format!("Failed to push settings: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Push settings failed ({status}): {body}"));
    }

    log::info!("System prompt and settings pushed to Open WebUI");
    Ok(())
}

/// Preferred default model.
///
/// When auto-configuring the default model on first launch, prefer Qwen 3.5 35B.
/// The model may appear under different IDs depending on the backend:
///   - Ollama: "qwen3.5:35b"
///   - vLLM/OpenAI-compat: "qwen/qwen3.5-35b-a3b"
const PREFERRED_MODEL_PATTERNS: &[&str] = &["qwen3.5", "qwen3.5-35b"];

/// Quick check for the preferred model (no retries, no fallback).
///
/// Used when a non-preferred model is already set — we just want to see
/// if the preferred model has become available without the delay of retries.
async fn discover_preferred_model(
    client: &reqwest::Client,
    base_url: &str,
    jwt: &str,
) -> Option<String> {
    let url = format!("{base_url}/api/models");

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .ok()?;

    let body: serde_json::Value = resp.json().await.ok()?;
    let models = body.get("data")?.as_array()?;

    for pattern in PREFERRED_MODEL_PATTERNS {
        for model in models {
            if let Some(id) = model.get("id").and_then(|v| v.as_str()) {
                if id.to_lowercase().contains(pattern) {
                    return Some(id.to_string());
                }
            }
        }
    }

    None
}

/// Discover the best available model from Open WebUI.
///
/// Prefers Qwen 3.5 35B (see PREFERRED_MODEL_PATTERNS). Falls back to the
/// first available model if the preferred model is not found.
///
/// Retries up to 3 times with 3-second delays because LLM backends (Ollama,
/// vLLM) may still be loading models when Open WebUI first becomes healthy.
/// Returns None only if no models are available after all retries.
async fn discover_first_model(
    client: &reqwest::Client,
    base_url: &str,
    jwt: &str,
) -> Option<String> {
    let url = format!("{base_url}/api/models");

    for attempt in 0..3 {
        if attempt > 0 {
            log::info!(
                "Model discovery retry {}/3 — waiting for backends to load models...",
                attempt + 1
            );
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }

        let resp = match client
            .get(&url)
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                log::debug!("Failed to query models (attempt {}): {e}", attempt + 1);
                continue;
            }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                log::debug!("Failed to parse models response (attempt {}): {e}", attempt + 1);
                continue;
            }
        };

        // Open WebUI returns { "data": [ { "id": "model-name", ... }, ... ] }
        let models = match body.get("data").and_then(|d| d.as_array()) {
            Some(m) if !m.is_empty() => m,
            _ => {
                log::debug!("No models available yet (attempt {})", attempt + 1);
                continue;
            }
        };

        // Try to find the preferred model (Qwen 3.5 35B)
        for pattern in PREFERRED_MODEL_PATTERNS {
            for model in models {
                if let Some(id) = model.get("id").and_then(|v| v.as_str()) {
                    if id.to_lowercase().contains(pattern) {
                        log::info!("Found preferred model: {id}");
                        return Some(id.to_string());
                    }
                }
            }
        }

        // Models exist but preferred not found — use first available
        let first = models
            .first()
            .and_then(|m| m.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string());

        if let Some(ref id) = first {
            log::info!("Preferred model not found, falling back to: {id}");
        }

        return first;
    }

    log::warn!("Model discovery failed after 3 attempts — no models available");
    None
}

/// Push tool server connections to Open WebUI via admin API.
///
/// This ensures the database-persisted config matches the Docker env var.
/// Open WebUI uses PersistentConfig which prefers database values over env
/// vars once the database has been written to. By calling the admin API,
/// we update both in-memory config AND the database.
///
/// Called after docker::ensure_running() to ensure the new container's
/// tool connections are properly registered.
pub async fn push_tool_connections(
    base_url: &str,
    jwt: &str,
    mcp_json: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    // Parse the TOOL_SERVER_CONNECTIONS JSON that we also pass to Docker
    let connections: serde_json::Value = serde_json::from_str(mcp_json)
        .map_err(|e| format!("Failed to parse MCP connections JSON: {e}"))?;

    let body = serde_json::json!({
        "TOOL_SERVER_CONNECTIONS": connections,
    });

    let url = format!("{base_url}/api/v1/configs/tool_servers");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to push tool connections: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Push tool connections failed ({status}): {body}"));
    }

    let count = connections.as_array().map(|a| a.len()).unwrap_or(0);
    log::info!("Pushed {count} tool server connections to Open WebUI");
    Ok(())
}
