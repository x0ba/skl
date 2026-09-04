//! `skl sync` — thin wrapper around the `/v1` hash-sync engine.
//!
//! Furnace call sites (implemented in this PR, not TODOs):
//!
//! 1. Scrub before upload
//!    - `crate::hooks::scrub::scrub_before_upload` — before `POST /v1/sync`
//!    - `crate::hooks::scrub::scrub_blob_before_upload` — before `PUT /v1/blobs/:hash`
//!    - Scanner: `crate::scrub` (`guard_bytes` / `guard_bytes_with`)
//!
//! 2. Conflict keep-local / keep-remote, then re-POST `/v1/sync`
//!    - `crate::hooks::conflict::resolve_conflicts` — `conflicts[]` incl. `remote_updated_at`
//!    - Prompt UX: `crate::prompt` (skill, short hashes, remote_updated_at, local mtime)
//!    - Apply + re-POST: `crate::sync::run_with_opts`
//!
//! Flags: `skl sync --keep-local` | `--keep-remote` | (TTY prompt).

use crate::error::Result;
use crate::sync::{self, SyncOptions};

pub async fn run(api_base: String, opts: SyncOptions) -> Result<()> {
    sync::run(api_base, opts).await
}
