//! Device token storage via the OS keyring.
//!
//! Cipher contract: store `access_token` only (no refresh_token, no credentials.json).
//! Service: `skl`. Account: `device_token`.
//!
//! Local API without Clerk accepts `Authorization: Bearer dev:<user_id>`.
//! `SKL_TOKEN` (then `SKL_TOKEN_FILE`) overrides the keyring (tests / CI).

use keyring::Entry;

use crate::api::DEV_AUTH_PREFIX;
use crate::error::{Result, SklError};

pub const KEYRING_SERVICE: &str = "skl";
pub const KEYRING_ACCOUNT: &str = "device_token";
pub const TOKEN_ENV: &str = "SKL_TOKEN";

fn entry() -> Result<Entry> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(SklError::from)
}

fn env_token() -> Option<String> {
    std::env::var(TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Optional file token (`SKL_TOKEN_FILE`) for headless / CI when the OS
/// keyring is missing. Env `SKL_TOKEN` still wins.
fn file_token() -> Option<String> {
    const FILE_ENV: &str = "SKL_TOKEN_FILE";
    let path = std::env::var_os(FILE_ENV)?;
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn store_device_token(token: &str) -> Result<()> {
    if token.is_empty() {
        return Err(SklError::DeviceAuthFailed("empty access_token".into()));
    }
    // Headless CI / smoke: SKL_TOKEN(+_FILE) already overrides reads. Skip
    // Secret Service so `skl login --dev-user` does not need DBus.
    if env_token().is_some() || file_token().is_some() {
        return Ok(());
    }
    match entry()?.set_password(token) {
        Ok(()) => Ok(()),
        Err(err) => Err(SklError::from(err)),
    }
}

pub fn load_device_token() -> Result<String> {
    if let Some(token) = env_token() {
        return Ok(token);
    }
    if let Some(token) = file_token() {
        return Ok(token);
    }
    match entry()?.get_password() {
        Ok(token) if !token.is_empty() => Ok(token),
        Ok(_) => Err(SklError::NotLoggedIn),
        Err(keyring::Error::NoEntry) => Err(SklError::NotLoggedIn),
        Err(err) => Err(SklError::from(err)),
    }
}

pub fn format_dev_token(user_id: &str) -> Result<String> {
    let trimmed = user_id.trim();
    if trimmed.is_empty() {
        return Err(SklError::DeviceAuthFailed("empty --dev-user".into()));
    }
    if trimmed.starts_with(DEV_AUTH_PREFIX) {
        if trimmed.len() == DEV_AUTH_PREFIX.len() {
            return Err(SklError::DeviceAuthFailed("empty --dev-user".into()));
        }
        return Ok(trimmed.to_string());
    }
    Ok(format!("{DEV_AUTH_PREFIX}{trimmed}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenPresence {
    Present { preview: String },
    Absent,
    Error(String),
}

pub fn token_presence() -> TokenPresence {
    match load_device_token() {
        Ok(token) => TokenPresence::Present {
            preview: token_preview(&token),
        },
        Err(SklError::NotLoggedIn) => TokenPresence::Absent,
        Err(err) => TokenPresence::Error(err.to_string()),
    }
}

fn token_preview(token: &str) -> String {
    if token.starts_with(DEV_AUTH_PREFIX) {
        return token.to_string();
    }
    if token.len() <= 12 {
        return "(set)".into();
    }
    format!("{}…", &token[..12])
}

#[allow(dead_code)]
pub fn delete_device_token() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(SklError::from(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dev_token() {
        assert_eq!(format_dev_token("alice").unwrap(), "dev:alice");
        assert_eq!(format_dev_token("dev:bob").unwrap(), "dev:bob");
        assert!(format_dev_token("").is_err());
        assert!(format_dev_token("dev:").is_err());
    }

    #[test]
    fn load_prefers_skl_token_env() {
        let prev = std::env::var_os(TOKEN_ENV);
        std::env::set_var(TOKEN_ENV, "dev:from-env");
        let loaded = load_device_token();
        match prev {
            Some(value) => std::env::set_var(TOKEN_ENV, value),
            None => std::env::remove_var(TOKEN_ENV),
        }
        assert_eq!(loaded.unwrap(), "dev:from-env");
    }

    #[test]
    fn store_skips_keyring_when_skl_token_set() {
        let prev = std::env::var_os(TOKEN_ENV);
        std::env::set_var(TOKEN_ENV, "dev:ci-bypass");
        let stored = store_device_token("dev:ci-bypass");
        match prev {
            Some(value) => std::env::set_var(TOKEN_ENV, value),
            None => std::env::remove_var(TOKEN_ENV),
        }
        stored.expect("SKL_TOKEN must bypass OS keyring store");
    }

    #[test]
    fn load_reads_skl_token_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "dev:from-file\n").unwrap();
        let prev_env = std::env::var_os(TOKEN_ENV);
        let prev_file = std::env::var_os("SKL_TOKEN_FILE");
        std::env::remove_var(TOKEN_ENV);
        std::env::set_var("SKL_TOKEN_FILE", &path);
        let loaded = load_device_token();
        match prev_env {
            Some(value) => std::env::set_var(TOKEN_ENV, value),
            None => std::env::remove_var(TOKEN_ENV),
        }
        match prev_file {
            Some(value) => std::env::set_var("SKL_TOKEN_FILE", value),
            None => std::env::remove_var("SKL_TOKEN_FILE"),
        }
        assert_eq!(loaded.unwrap(), "dev:from-file");
    }
}
