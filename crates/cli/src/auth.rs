//! Device token storage in the local state DB (Atuin-style).
//!
//! Cipher contract: store `access_token` only (no refresh_token, no credentials.json).
//! Primary store: `state.db` `meta.device_token` under `Paths.data_dir` / `SKL_DATA_DIR`.
//! File mode `0600`, parent data dir `0700`. No E2EE of the store in v1.
//!
//! Precedence: `SKL_TOKEN` > `SKL_TOKEN_FILE` > local store.
//! If the local store is empty, migrate once from the OS keyring (best-effort;
//! leftover keyring entries are unused after migrate). Normal login/logout
//! does not require keyring / DBus / Secret Service.
//!
//! Local API without Clerk accepts `Authorization: Bearer dev:<user_id>`.

use crate::api::DEV_AUTH_PREFIX;
use crate::config::Paths;
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;

/// Legacy OS keyring coordinates (migrate-once / optional cleanup only).
pub const KEYRING_SERVICE: &str = "skl";
pub const KEYRING_ACCOUNT: &str = "device_token";
pub const TOKEN_ENV: &str = "SKL_TOKEN";
pub const TOKEN_FILE_ENV: &str = "SKL_TOKEN_FILE";
pub const META_DEVICE_TOKEN: &str = "device_token";

fn env_token() -> Option<String> {
    std::env::var(TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Optional file token (`SKL_TOKEN_FILE`) for headless / CI.
/// Env `SKL_TOKEN` still wins.
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

fn open_store() -> Result<LocalDb> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    LocalDb::open(&paths.db_file)
}

fn load_local_token() -> Result<Option<String>> {
    let paths = match Paths::resolve() {
        Ok(paths) => paths,
        Err(_) => return Ok(None),
    };
    if !paths.db_file.exists() {
        return Ok(None);
    }
    let db = LocalDb::open(&paths.db_file)?;
    Ok(db
        .get_meta(META_DEVICE_TOKEN)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn try_keyring_password() -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).ok()?;
    match entry.get_password() {
        Ok(token) => {
            let trimmed = token.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

/// Copy a legacy keyring token into the local store when the store is empty.
/// Returns the adopted token. Does not overwrite an existing local token.
/// Keyring is left in place (stale leftover is unused).
fn migrate_keyring_once() -> Result<Option<String>> {
    let Some(token) = try_keyring_password() else {
        return Ok(None);
    };
    if adopt_legacy_token_if_empty(&token)? {
        return Ok(Some(token));
    }
    Ok(None)
}

/// Persist `token` into `state.db` when the local store is empty.
/// Used by migrate-once and tests. Does not touch the OS keyring.
pub fn adopt_legacy_token_if_empty(token: &str) -> Result<bool> {
    if token.is_empty() {
        return Err(SklError::DeviceAuthFailed("empty access_token".into()));
    }
    if load_local_token()?.is_some() {
        return Ok(false);
    }
    let db = open_store()?;
    db.set_meta(META_DEVICE_TOKEN, token)?;
    Ok(true)
}

pub fn store_device_token(token: &str) -> Result<()> {
    if token.is_empty() {
        return Err(SklError::DeviceAuthFailed("empty access_token".into()));
    }
    let db = open_store()?;
    db.set_meta(META_DEVICE_TOKEN, token)?;
    Ok(())
}

pub fn load_device_token() -> Result<String> {
    if let Some(token) = env_token() {
        return Ok(token);
    }
    if let Some(token) = file_token() {
        return Ok(token);
    }
    if let Some(token) = load_local_token()? {
        return Ok(token);
    }
    if let Some(token) = migrate_keyring_once()? {
        return Ok(token);
    }
    Err(SklError::NotLoggedIn)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Env,
    File,
    Local,
    None,
}

pub fn token_source() -> TokenSource {
    if env_token().is_some() {
        return TokenSource::Env;
    }
    if file_token().is_some() {
        return TokenSource::File;
    }
    match load_local_token() {
        Ok(Some(_)) => TokenSource::Local,
        _ => TokenSource::None,
    }
}

pub fn token_store_path() -> Option<std::path::PathBuf> {
    Paths::resolve().ok().map(|paths| paths.db_file)
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

/// Clear the local store. Best-effort keyring delete (leftover is unused if it fails).
pub fn delete_device_token() -> Result<()> {
    if let Ok(paths) = Paths::resolve() {
        if paths.db_file.exists() {
            let db = LocalDb::open(&paths.db_file)?;
            db.delete_meta(META_DEVICE_TOKEN)?;
        }
    }
    try_delete_keyring();
    Ok(())
}

fn try_delete_keyring() {
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) else {
        return;
    };
    match entry.delete_credential() {
        Ok(()) => {}
        Err(keyring::Error::NoEntry) => {}
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialise env-mutating tests so parallel `cargo test` does not race.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        token: Option<std::ffi::OsString>,
        file: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        config: Option<std::ffi::OsString>,
        _dir: Option<tempfile::TempDir>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn isolated() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
            let token = std::env::var_os(TOKEN_ENV);
            let file = std::env::var_os(TOKEN_FILE_ENV);
            let data = std::env::var_os("SKL_DATA_DIR");
            let config = std::env::var_os("SKL_CONFIG_DIR");
            std::env::remove_var(TOKEN_ENV);
            std::env::remove_var(TOKEN_FILE_ENV);
            let dir = tempfile::tempdir().unwrap();
            std::env::set_var("SKL_DATA_DIR", dir.path().join("data"));
            std::env::set_var("SKL_CONFIG_DIR", dir.path().join("config"));
            Self {
                token,
                file,
                data,
                config,
                _dir: Some(dir),
                _guard: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.token {
                Some(value) => std::env::set_var(TOKEN_ENV, value),
                None => std::env::remove_var(TOKEN_ENV),
            }
            match &self.file {
                Some(value) => std::env::set_var(TOKEN_FILE_ENV, value),
                None => std::env::remove_var(TOKEN_FILE_ENV),
            }
            match &self.data {
                Some(value) => std::env::set_var("SKL_DATA_DIR", value),
                None => std::env::remove_var("SKL_DATA_DIR"),
            }
            match &self.config {
                Some(value) => std::env::set_var("SKL_CONFIG_DIR", value),
                None => std::env::remove_var("SKL_CONFIG_DIR"),
            }
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
    fn store_and_load_without_keyring() {
        let _env = EnvGuard::isolated();
        store_device_token("dev:local-store").unwrap();
        assert_eq!(load_device_token().unwrap(), "dev:local-store");
        assert_eq!(token_source(), TokenSource::Local);
        let paths = Paths::resolve().unwrap();
        assert!(paths.db_file.exists());
        let db = LocalDb::open(&paths.db_file).unwrap();
        assert_eq!(
            db.get_meta(META_DEVICE_TOKEN).unwrap().as_deref(),
            Some("dev:local-store")
        );
    }

    #[test]
    fn load_prefers_skl_token_env() {
        let _env = EnvGuard::isolated();
        store_device_token("dev:local-store").unwrap();
        std::env::set_var(TOKEN_ENV, "dev:from-env");
        assert_eq!(load_device_token().unwrap(), "dev:from-env");
        assert_eq!(token_source(), TokenSource::Env);
    }

    #[test]
    fn load_reads_skl_token_file_over_local() {
        let _env = EnvGuard::isolated();
        store_device_token("dev:local-store").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "dev:from-file\n").unwrap();
        std::env::set_var(TOKEN_FILE_ENV, &path);
        assert_eq!(load_device_token().unwrap(), "dev:from-file");
        assert_eq!(token_source(), TokenSource::File);
    }

    #[test]
    fn env_wins_over_token_file() {
        let _env = EnvGuard::isolated();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "dev:from-file\n").unwrap();
        std::env::set_var(TOKEN_FILE_ENV, &path);
        std::env::set_var(TOKEN_ENV, "dev:from-env");
        assert_eq!(load_device_token().unwrap(), "dev:from-env");
        assert_eq!(token_source(), TokenSource::Env);
    }

    #[test]
    fn logout_clears_local_store() {
        let _env = EnvGuard::isolated();
        store_device_token("dev:to-clear").unwrap();
        delete_device_token().unwrap();
        match load_device_token() {
            Err(SklError::NotLoggedIn) => {}
            other => panic!("expected NotLoggedIn, got {other:?}"),
        }
        assert_eq!(token_source(), TokenSource::None);
        let paths = Paths::resolve().unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        assert!(db.get_meta(META_DEVICE_TOKEN).unwrap().is_none());
    }

    #[test]
    fn migrate_once_adopts_legacy_when_local_empty() {
        let _env = EnvGuard::isolated();
        assert!(adopt_legacy_token_if_empty("dev:from-keyring").unwrap());
        assert_eq!(load_device_token().unwrap(), "dev:from-keyring");
        assert!(!adopt_legacy_token_if_empty("dev:other").unwrap());
        assert_eq!(load_device_token().unwrap(), "dev:from-keyring");
    }

    #[test]
    fn store_empty_token_fails() {
        let _env = EnvGuard::isolated();
        assert!(store_device_token("").is_err());
    }

    #[test]
    fn absent_without_store_or_env() {
        let _env = EnvGuard::isolated();
        match load_device_token() {
            Err(SklError::NotLoggedIn) => {}
            other => panic!("expected NotLoggedIn, got {other:?}"),
        }
        assert_eq!(token_presence(), TokenPresence::Absent);
    }
}
