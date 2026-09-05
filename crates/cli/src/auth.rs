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
pub const TOKEN_FILE_ENV: &str = "SKL_TOKEN_FILE";

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
    let path = std::env::var_os(TOKEN_FILE_ENV)?;
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn has_token_override() -> bool {
    env_token().is_some() || file_token().is_some()
}

pub fn store_device_token(token: &str) -> Result<()> {
    if token.is_empty() {
        return Err(SklError::DeviceAuthFailed("empty access_token".into()));
    }
    // Always persist when the OS keyring works so `skl login` remains
    // durable after SKL_TOKEN / SKL_TOKEN_FILE is unset. Headless CI has
    // no Secret Service: fail-soft only when an override already covers reads.
    match entry()?.set_password(token) {
        Ok(()) => Ok(()),
        Err(err) if has_token_override() => {
            eprintln!(
                "warn: could not persist token to the OS keyring ({err}); \
                 SKL_TOKEN / SKL_TOKEN_FILE is set, continuing"
            );
            Ok(())
        }
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
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_token_env<T>(body: impl FnOnce() -> T) -> T {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        body()
    }

    fn restore_var(name: &str, prev: Option<std::ffi::OsString>) {
        match prev {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    #[test]
    fn formats_dev_token() {
        assert_eq!(format_dev_token("alice").unwrap(), "dev:alice");
        assert_eq!(format_dev_token("dev:bob").unwrap(), "dev:bob");
        assert!(format_dev_token("").is_err());
        assert!(format_dev_token("dev:").is_err());
    }

    #[test]
    fn load_prefers_skl_token_env() {
        with_token_env(|| {
            let prev = std::env::var_os(TOKEN_ENV);
            std::env::set_var(TOKEN_ENV, "dev:from-env");
            let loaded = load_device_token();
            restore_var(TOKEN_ENV, prev);
            assert_eq!(loaded.unwrap(), "dev:from-env");
        });
    }

    #[test]
    fn store_fail_soft_when_override_set_and_keyring_fails() {
        with_token_env(|| {
            let prev = std::env::var_os(TOKEN_ENV);
            std::env::set_var(TOKEN_ENV, "dev:ci-bypass");
            let stored = store_device_token("dev:ci-bypass");
            restore_var(TOKEN_ENV, prev);
            stored.expect("SKL_TOKEN must fail-soft if the OS keyring is missing");
        });
    }

    #[test]
    fn load_reads_skl_token_file() {
        with_token_env(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("token");
            std::fs::write(&path, "dev:from-file\n").unwrap();
            let prev_env = std::env::var_os(TOKEN_ENV);
            let prev_file = std::env::var_os(TOKEN_FILE_ENV);
            std::env::remove_var(TOKEN_ENV);
            std::env::set_var(TOKEN_FILE_ENV, &path);
            let loaded = load_device_token();
            restore_var(TOKEN_ENV, prev_env);
            restore_var(TOKEN_FILE_ENV, prev_file);
            assert_eq!(loaded.unwrap(), "dev:from-file");
        });
    }
}
