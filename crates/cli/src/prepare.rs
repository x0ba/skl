//! Scrub file bytes **before** hashing.
//!
//! Upload order (furnace):
//! 1. [`prepare_bytes`] / [`prepare_sync`] — scrub, then SHA-256 those bytes
//! 2. `POST /v1/sync` with the resulting `tree_hash` / `files` map
//! 3. `PUT /v1/blobs/:hash` for each `upload[]` hash (bytes that were hashed)
//! 4. `PUT /v1/skills/:name/tree` with the same `{ tree_hash, files }`
//!
//! Never hash first and scrub later — the blob hash must be the scrubbed bytes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use thiserror::Error;

use crate::conflict::{SkillState, SyncRequest};
use crate::scrub::{guard_bytes_with, ScanReport, UploadDecision};
use crate::skill_tree::{hash_bytes, slash_path, tree_hash_from_map, SkillTree, TreeError};

#[derive(Debug, Error)]
pub enum PrepareError {
    #[error(transparent)]
    Tree(#[from] TreeError),
    #[error("upload blocked: secret findings")]
    Blocked { report: ScanReport },
}

/// Bytes that passed scrub, plus the SHA-256 of **those** bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBlob {
    pub skill: String,
    pub path: PathBuf,
    pub hash: String,
    pub bytes: Vec<u8>,
}

/// Local inventory ready for `POST /v1/sync` + later blob/tree PUTs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSync {
    pub request: SyncRequest,
    /// hash → raw bytes (the same bytes that were hashed after scrub).
    pub blobs: BTreeMap<String, Vec<u8>>,
}

/// Scrub `bytes`, then hash. Hash is never computed if scrub blocks.
pub fn prepare_bytes(
    skill: &str,
    path: &std::path::Path,
    bytes: &[u8],
    allow_warnings: bool,
) -> Result<PreparedBlob, PrepareError> {
    match guard_bytes_with(skill, path, bytes, allow_warnings) {
        UploadDecision::Block { report } => Err(PrepareError::Blocked { report }),
        UploadDecision::Allow | UploadDecision::AllowWithWarnings { .. } => Ok(PreparedBlob {
            skill: skill.to_string(),
            path: path.to_path_buf(),
            hash: hash_bytes(bytes),
            bytes: bytes.to_vec(),
        }),
    }
}

/// Walk a local skill tree: scrub each file, then hash, then build `SyncRequest`.
pub fn prepare_sync(tree: &SkillTree, allow_warnings: bool) -> Result<PreparedSync, PrepareError> {
    let mut skills: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for file in &tree.files {
        let bytes = std::fs::read(&file.abs_path).map_err(|source| TreeError::Io {
            path: file.abs_path.clone(),
            source,
        })?;
        let prepared = prepare_bytes(
            &file.skill_name,
            &file.relative_path,
            &bytes,
            allow_warnings,
        )?;
        let rel = slash_path(&file.relative_path);
        skills
            .entry(file.skill_name.clone())
            .or_default()
            .insert(rel, prepared.hash.clone());
        blobs.insert(prepared.hash, prepared.bytes);
    }

    let mut request = SyncRequest::default();
    for (name, files) in skills {
        let tree_hash = tree_hash_from_map(&files);
        request.skills.insert(name, SkillState { tree_hash, files });
    }

    Ok(PreparedSync { request, blobs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_tree::{hash_bytes, scan_tree};
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    fn write_file(path: &std::path::Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn hashes_only_after_scrub_passes() {
        let clean = b"# ok\n";
        let prepared = prepare_bytes("demo", Path::new("SKILL.md"), clean, false).unwrap();
        assert_eq!(prepared.hash, hash_bytes(clean));
        assert_eq!(prepared.bytes, clean);
    }

    #[test]
    fn secret_bytes_are_not_hashed() {
        let pem = b"-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n-----END OPENSSH PRIVATE KEY-----\n";
        let err = prepare_bytes("demo", Path::new("id"), pem, true).unwrap_err();
        assert!(matches!(err, PrepareError::Blocked { .. }));
    }

    #[test]
    fn prepare_sync_builds_request_from_scrubbed_hashes() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(&tmp.path().join("demo/SKILL.md"), "# demo\n");
        let tree = scan_tree(tmp.path()).unwrap();
        let prepared = prepare_sync(&tree, false).unwrap();
        let skill = &prepared.request.skills["demo"];
        let file_hash = &skill.files["SKILL.md"];
        assert_eq!(file_hash, &hash_bytes(b"# demo\n"));
        assert_eq!(skill.tree_hash, tree_hash_from_map(&skill.files));
        assert_eq!(prepared.blobs[file_hash], b"# demo\n");
    }
}
