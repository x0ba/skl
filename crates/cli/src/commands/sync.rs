//! `skl sync` — thin wrapper around the `/v1` hash-sync engine.

use crate::error::Result;
use crate::sync::{self, SyncOptions};

pub async fn run(api_base: String, opts: SyncOptions) -> Result<()> {
    sync::run(api_base, opts).await
}
