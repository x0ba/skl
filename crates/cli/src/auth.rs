//! Device token storage via the OS keyring.
//!
//! Cipher contract: store `access_token` only (no refresh_token, no credentials.json).
//! Service: `skl`. Account: `device_token`.

use keyring::Entry;

use crate::error::{Result, SklError};

pub const KEYRING_SERVICE: &str = "skl";
pub const KEYRING_ACCOUNT: &str = "device_token";

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
    match entry()?.get_password() {
        Ok(token) if !token.is_empty() => Ok(token),
        Ok(_) => Err(SklError::NotLoggedIn),
        Err(keyring::Error::NoEntry) => Err(SklError::NotLoggedIn),
        Err(err) => Err(SklError::from(err)),
    }
}

#[allow(dead_code)]
pub fn delete_device_token() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(SklError::from(err)),
    }
}
