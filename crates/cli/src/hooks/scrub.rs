//! Secret-scrub hook — called from `crate::sync` before POST and each blob PUT.
//!
//! Refuses dirty bytes. Does not auto-redact (hash would no longer match).

use std::path::Path;

use crate::api::SyncRequest;
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;
use crate::scrub::{guard_bytes_with, print_report, UploadDecision};

/// Inspect local skill bytes before they leave the machine (pre-POST).
pub fn scrub_before_upload(
    db: &LocalDb,
    request: &SyncRequest,
    allow_warnings: bool,
) -> Result<()> {
    for (name, tree) in &request.skills {
        let Some(skill) = db.find_skill(name)? else {
            continue;
        };
        for rel in tree.files.keys() {
            let path = skill.path.join(rel);
            let bytes = std::fs::read(&path).map_err(|err| {
                SklError::BlockedSecrets(format!(
                    "cannot read {} / {} for secret scan: {err}",
                    name,
                    path.display()
                ))
            })?;
            refuse_if_dirty(name, Path::new(rel.as_str()), &bytes, allow_warnings)?;
        }
    }
    Ok(())
}

/// Per-blob hook immediately before PUT /v1/blobs/:hash.
pub fn scrub_blob_before_upload(hash: &str, bytes: &[u8], allow_warnings: bool) -> Result<()> {
    refuse_if_dirty("upload", Path::new(hash), bytes, allow_warnings)
}

fn refuse_if_dirty(skill: &str, path: &Path, bytes: &[u8], allow_warnings: bool) -> Result<()> {
    match guard_bytes_with(skill, path, bytes, allow_warnings) {
        UploadDecision::Allow | UploadDecision::AllowWithWarnings { .. } => Ok(()),
        UploadDecision::Block { report } => {
            let mut stderr = std::io::stderr().lock();
            let _ = print_report(&report, &mut stderr);
            Err(SklError::BlockedSecrets(format!(
                "secret findings in {} / {} — not hashing or PUT /v1/blobs",
                skill,
                path.display()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_hook_blocks_private_key() {
        let pem = b"-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n-----END OPENSSH PRIVATE KEY-----\n";
        let err = scrub_blob_before_upload("abc", pem, true).unwrap_err();
        assert!(matches!(err, SklError::BlockedSecrets(_)));
    }

    #[test]
    fn blob_hook_allows_clean_markdown() {
        scrub_blob_before_upload("abc", b"# hello\n", false).unwrap();
    }

    #[test]
    fn unread_indexed_file_blocks_upload() {
        use crate::api::types::SkillTree;
        use crate::local::db::LocalDb;
        use crate::local::skills::DiscoveredSkill;
        use std::collections::BTreeMap;

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let gone = skill_dir.join("SKILL.md");
        std::fs::write(&gone, "# hi\n").unwrap();

        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), "abc".into());
        let db = LocalDb::open(&tmp.path().join("state.db")).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "demo".into(),
            source: "claude".into(),
            path: skill_dir,
            tree: SkillTree {
                tree_hash: "abc".into(),
                files: files.clone(),
            },
        }])
        .unwrap();
        std::fs::remove_file(&gone).unwrap();

        let mut request = crate::api::SyncRequest::default();
        request.skills.insert(
            "demo".into(),
            SkillTree {
                tree_hash: "abc".into(),
                files,
            },
        );
        let err = scrub_before_upload(&db, &request, false).unwrap_err();
        assert!(matches!(err, SklError::BlockedSecrets(_)));
        assert!(err.to_string().contains("cannot read"));
    }
}
