//! Secret-scrub hook — hammer owns the implementation.
//!
//! Call site: immediately before POST /v1/sync and before each blob PUT.

use crate::api::SyncRequest;
use crate::error::Result;

/// Inspect / redact local skill bytes before they leave the machine.
///
/// TODO(hammer/secret-scrub): scan skill files for secrets (tokens, keys,
/// `.env`, private keys) and fail or redact before upload. Do not invent a
/// scrubber in this crate.
pub fn scrub_before_upload(request: &SyncRequest) -> Result<()> {
    let _ = request;
    // TODO(hammer/secret-scrub): implement. Currently a no-op boundary.
    Ok(())
}

/// Per-blob hook before PUT /v1/blobs/:hash.
///
/// TODO(hammer/secret-scrub): inspect `bytes` for secrets; refuse the upload
/// if the blob looks dirty.
pub fn scrub_blob_before_upload(hash: &str, bytes: &[u8]) -> Result<()> {
    let _ = (hash, bytes);
    // TODO(hammer/secret-scrub): implement. Currently a no-op boundary.
    Ok(())
}
