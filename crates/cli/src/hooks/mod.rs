//! Hook points wired from `crate::sync`.
//!
//! Scrub refuses dirty bytes before POST / PUT blobs. Conflict applies
//! keep-local / keep-remote (no auto-merge) then the engine re-POSTs.

pub mod conflict;
pub mod scrub;
