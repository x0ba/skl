//! Device token storage via the OS keyring.
//!
//! Cipher contract: store `access_token` only (no refresh_token, no credentials.json).
//! Service: `skl`. Account: `device_token`.
//!
//! Local API without Clerk accepts `Authorization: Bearer dev:<user_id>`.
//! `SKL_TOKEN` overrides the keyring (tests / headless smoke).

use keyring::Entry;

use crate::api::DEV_AUTH_PREFIX;
use crate::error::{Result, SklError};

pub const KEYRING_SERVICE: &str = "skl";
pub const KEYRING_ACCOUNT: &str = "device_token";
pub const TOKEN_ENV: &str = "SKL_TOKEN";

fn entry() -> Result<Entry> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(SklError::from)
}

pub fn store_device_token(token: &str) -> Result<()> {
    if token.is_empty() {
        return Err(SklError::DeviceAuthFailed("empty access_token".into()));
    }
    entry()?.set_password(token)?;
    Ok(())
}

pub fn load_device_token() -> Result<String> {
    if let Ok(token) = std::env::var(TOKEN_ENV) {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
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
}
