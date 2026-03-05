//! macOS Keychain integration for storing Open WebUI admin credentials.
//!
//! Stores: email, password, JWT token.
//! Service name: "com.percona.ibex"

use keyring::Entry;

const SERVICE: &str = "com.percona.ibex";

/// Keys stored in the keychain.
const KEY_EMAIL: &str = "admin_email";
const KEY_PASSWORD: &str = "admin_password";
const KEY_JWT: &str = "jwt_token";

/// Admin credentials stored in the macOS Keychain.
#[derive(Debug, Clone)]
pub struct AdminCredentials {
    pub email: String,
    pub password: String,
    pub jwt: Option<String>,
}

/// Get the admin email from keychain.
pub fn get_email() -> Option<String> {
    get_entry(KEY_EMAIL)
}

/// Get the admin password from keychain.
pub fn get_password() -> Option<String> {
    get_entry(KEY_PASSWORD)
}

/// Get the JWT token from keychain.
pub fn get_jwt() -> Option<String> {
    get_entry(KEY_JWT)
}

/// Get full admin credentials from keychain.
/// Returns None if email or password is missing.
pub fn get_credentials() -> Option<AdminCredentials> {
    let email = get_email()?;
    let password = get_password()?;
    let jwt = get_jwt();
    Some(AdminCredentials {
        email,
        password,
        jwt,
    })
}

/// Store admin email in keychain.
pub fn set_email(value: &str) -> Result<(), String> {
    set_entry(KEY_EMAIL, value)
}

/// Store admin password in keychain.
pub fn set_password(value: &str) -> Result<(), String> {
    set_entry(KEY_PASSWORD, value)
}

/// Store JWT token in keychain.
pub fn set_jwt(value: &str) -> Result<(), String> {
    set_entry(KEY_JWT, value)
}

/// Store all admin credentials.
pub fn set_credentials(creds: &AdminCredentials) -> Result<(), String> {
    set_email(&creds.email)?;
    set_password(&creds.password)?;
    if let Some(ref jwt) = creds.jwt {
        set_jwt(jwt)?;
    }
    Ok(())
}

/// Delete all stored credentials (used for reset).
pub fn clear_credentials() -> Result<(), String> {
    delete_entry(KEY_EMAIL)?;
    delete_entry(KEY_PASSWORD)?;
    delete_entry(KEY_JWT)?;
    Ok(())
}

/// Check if credentials exist in the keychain.
pub fn has_credentials() -> bool {
    get_email().is_some() && get_password().is_some()
}

// ── Private helpers ──

fn get_entry(key: &str) -> Option<String> {
    let entry = Entry::new(SERVICE, key).ok()?;
    entry.get_password().ok()
}

fn set_entry(key: &str, value: &str) -> Result<(), String> {
    let entry =
        Entry::new(SERVICE, key).map_err(|e| format!("Keychain entry error for {key}: {e}"))?;
    entry
        .set_password(value)
        .map_err(|e| format!("Failed to set keychain {key}: {e}"))
}

fn delete_entry(key: &str) -> Result<(), String> {
    let entry = match Entry::new(SERVICE, key) {
        Ok(e) => e,
        Err(_) => return Ok(()), // Entry doesn't exist, that's fine
    };
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already gone
        Err(e) => Err(format!("Failed to delete keychain {key}: {e}")),
    }
}
