//! `skl sync` — thin wrapper around the `/v1` hash-sync engine.

use crate::error::Result;
use crate::sync;

pub async fn run(api_base: String) -> Result<()> {
    sync::run(api_base).await
}
