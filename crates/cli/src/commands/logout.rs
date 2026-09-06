//! `skl logout` — clear the local device token store.

use crate::auth;
use crate::error::Result;

pub fn run() -> Result<()> {
    let store = auth::token_store_path();
    auth::delete_device_token()?;
    eprintln!("Logged out. Local device token cleared.");
    if let Some(path) = store {
        eprintln!("  store {}", path.display());
    }
    eprintln!("OS keyring is not required; leftover keyring entries (if any) are unused.");
    Ok(())
}
