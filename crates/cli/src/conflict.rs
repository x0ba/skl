use std::collections::{BTreeMap, HashSet};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::skill_tree::{format_mtime, SkillTree};

/// One skill in `POST /v1/sync` request `skills`.
/// Mirrors `ClientSkillState` in `apps/api/src/contracts.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillState {
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
}

/// `POST /v1/sync` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SyncRequest {
    pub skills: BTreeMap<String, SkillState>,
}

/// `PUT /v1/skills/:name/tree` body (`PutSkillTreeRequest`).
/// Server rejects if a blob hash is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PutSkillTree {
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
}

impl From<SkillState> for PutSkillTree {
    fn from(value: SkillState) -> Self {
        Self {
            tree_hash: value.tree_hash,
            files: value.files,
        }
    }
}

/// `PUT /v1/skills/:name/tree` response (`PutSkillTreeResponse`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PutSkillTreeResponse {
    pub name: String,
    pub tree_hash: String,
    pub updated_at: String,
}

/// One `download[]` item (`SyncDownloadBlob`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncDownloadBlob {
    pub hash: String,
    pub skills: Vec<String>,
    pub paths: Vec<String>,
}

/// One row of `POST /v1/sync` → `conflicts[]`.
///
/// Exact wire shape from `contracts.ts`. Same skill name, different
/// `tree_hash`, neither side a strict ancestor. No auto-merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub skill: String,
    pub local_tree_hash: String,
    pub remote_tree_hash: String,
    /// ISO-8601 from the remote skill row's `updated_at`.
    pub remote_updated_at: String,
    /// Local mtime if furnace has it on disk. Never serialized.
    #[serde(default, skip)]
    pub local_mtime: Option<SystemTime>,
}

/// `POST /v1/sync` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SyncResponse {
    /// Blob hashes to PUT (after scrub-then-hash).
    #[serde(default)]
    pub upload: Vec<String>,
    /// Blobs to GET, with the skills/paths that reference each hash.
    #[serde(default)]
    pub download: Vec<SyncDownloadBlob>,
    /// Same name, different tree_hash, neither a strict ancestor. No auto-merge.
    #[serde(default)]
    pub conflicts: Vec<SyncConflict>,
    /// Skill names that exist only on the server.
    #[serde(default)]
    pub missing_skills: Vec<String>,
}

/// `GET /v1/skills` item (`SkillSummary`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSummary {
    pub name: String,
    pub tree_hash: String,
    pub updated_at: String,
}

/// `GET /v1/skills` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SkillsListResponse {
    pub skills: Vec<SkillSummary>,
}

/// `GET /v1/skills/:name` response (`SkillDetailResponse`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDetail {
    pub name: String,
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
    pub updated_at: String,
}

impl SyncConflict {
    /// Build from a `conflicts[]` row. Hashes are content-addressed SHA-256 hex.
    pub fn from_wire(
        skill: impl Into<String>,
        local_tree_hash: impl Into<String>,
        remote_tree_hash: impl Into<String>,
        remote_updated_at: impl Into<String>,
    ) -> Self {
        Self {
            skill: skill.into(),
            local_tree_hash: local_tree_hash.into(),
            remote_tree_hash: remote_tree_hash.into(),
            remote_updated_at: remote_updated_at.into(),
            local_mtime: None,
        }
    }

    pub fn with_local_mtime(mut self, ts: Option<SystemTime>) -> Self {
        self.local_mtime = ts;
        self
    }

    pub fn hashes_differ(&self) -> bool {
        self.local_tree_hash != self.remote_tree_hash
    }
}

/// Client choice. No auto-merge. Furnace applies via rename or overwrite,
/// then re-`POST /v1/sync`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    KeepLocal,
    KeepRemote,
}

/// Apply hint. This module does not write files or re-POST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    /// keep-local: overwrite remote, then re-PUT `/v1/skills/:name/tree`.
    OverwriteRemote,
    /// keep-remote: take remote; rename or skip local before retrying `POST /v1/sync`.
    TakeRemote,
}

impl From<Resolution> for SyncAction {
    fn from(value: Resolution) -> Self {
        match value {
            Resolution::KeepLocal => SyncAction::OverwriteRemote,
            Resolution::KeepRemote => SyncAction::TakeRemote,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedConflict {
    pub conflict: SyncConflict,
    pub resolution: Resolution,
    pub action: SyncAction,
}

/// Compare two on-disk trees by skill name + tree hash.
///
/// Production path is `POST /v1/sync` → [`SyncResponse::conflicts`]. Use this
/// only in tests or when furnace already has both trees locally.
pub fn detect_conflicts(local: &SkillTree, remote: &SkillTree) -> Vec<SyncConflict> {
    let mut names: HashSet<String> = local.skill_names().into_iter().collect();
    names.extend(remote.skill_names());
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort();

    let mut out = Vec::new();
    for skill in names {
        if !local.has_skill(&skill) || !remote.has_skill(&skill) {
            continue;
        }
        let local_hash = local.tree_hash(&skill);
        let remote_hash = remote.tree_hash(&skill);
        if local_hash == remote_hash {
            continue;
        }
        let remote_updated_at = match remote.latest_mtime(&skill) {
            Some(ts) => format_mtime(Some(ts)),
            None => String::new(),
        };
        out.push(
            SyncConflict::from_wire(skill.clone(), local_hash, remote_hash, remote_updated_at)
                .with_local_mtime(local.latest_mtime(&skill)),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_tree::scan_tree;
    use std::fs;
    use std::io::Write;

    fn write_file(path: &std::path::Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn wire_json_is_exactly_four_fields() {
        let json = r#"{"skill":"research","local_tree_hash":"aaa","remote_tree_hash":"bbb","remote_updated_at":"2024-01-01T00:00:00Z"}"#;
        let c: SyncConflict = serde_json::from_str(json).unwrap();
        assert_eq!(c.skill, "research");
        assert_eq!(c.local_tree_hash, "aaa");
        assert_eq!(c.remote_tree_hash, "bbb");
        assert_eq!(c.remote_updated_at, "2024-01-01T00:00:00Z");
        assert!(c.hashes_differ());
        assert!(c.local_mtime.is_none());
        let back = serde_json::to_value(&c).unwrap();
        let mut keys: Vec<_> = back.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "local_tree_hash",
                "remote_tree_hash",
                "remote_updated_at",
                "skill"
            ]
        );
    }

    #[test]
    fn sync_response_conflicts_array() {
        let json = r#"{"conflicts":[{"skill":"demo","local_tree_hash":"aa","remote_tree_hash":"bb","remote_updated_at":"2024-06-01T12:00:00Z"}]}"#;
        let resp: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.conflicts.len(), 1);
        assert_eq!(resp.conflicts[0].skill, "demo");
        assert_eq!(resp.conflicts[0].remote_updated_at, "2024-06-01T12:00:00Z");
    }

    #[test]
    fn sync_request_shape() {
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), "abc".into());
        let mut skills = BTreeMap::new();
        skills.insert(
            "demo".into(),
            SkillState {
                tree_hash: "def".into(),
                files,
            },
        );
        let v = serde_json::to_value(SyncRequest { skills }).unwrap();
        assert_eq!(v["skills"]["demo"]["tree_hash"], "def");
        assert_eq!(v["skills"]["demo"]["files"]["SKILL.md"], "abc");
    }

    #[test]
    fn sync_response_full_shape() {
        let json = r#"{
            "upload":["u1"],
            "download":[{"hash":"d1","skills":["demo"],"paths":["SKILL.md"]}],
            "conflicts":[{"skill":"demo","local_tree_hash":"aa","remote_tree_hash":"bb","remote_updated_at":"2024-01-01T00:00:00Z"}],
            "missing_skills":["other"]
        }"#;
        let resp: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.conflicts[0].skill, "demo");
        assert_eq!(resp.conflicts[0].remote_updated_at, "2024-01-01T00:00:00Z");
        assert_eq!(resp.missing_skills, ["other"]);
        assert_eq!(resp.download[0].hash, "d1");
        assert_eq!(resp.download[0].skills, ["demo"]);
        assert_eq!(resp.download[0].paths, ["SKILL.md"]);
        assert_eq!(resp.upload, ["u1"]);
    }

    #[test]
    fn skill_detail_includes_updated_at() {
        let json = r#"{
            "name":"demo",
            "tree_hash":"abc",
            "files":{"SKILL.md":"def"},
            "updated_at":"2024-03-15T08:30:00Z"
        }"#;
        let detail: SkillDetail = serde_json::from_str(json).unwrap();
        assert_eq!(detail.name, "demo");
        assert_eq!(detail.updated_at, "2024-03-15T08:30:00Z");
        assert_eq!(detail.files["SKILL.md"], "def");
    }

    #[test]
    fn skills_list_includes_updated_at() {
        let json =
            r#"{"skills":[{"name":"demo","tree_hash":"abc","updated_at":"2024-03-15T08:30:00Z"}]}"#;
        let list: SkillsListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(list.skills[0].updated_at, "2024-03-15T08:30:00Z");
    }

    #[test]
    fn matching_trees_are_quiet() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("alpha/SKILL.md"), "# a\n");
        let local = scan_tree(dir.path()).unwrap();
        let remote = scan_tree(dir.path()).unwrap();
        assert!(detect_conflicts(&local, &remote).is_empty());
    }

    #[test]
    fn tree_hash_mismatch_is_a_conflict() {
        let local_dir = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        write_file(&local_dir.path().join("alpha/SKILL.md"), "local\n");
        write_file(&remote_dir.path().join("alpha/SKILL.md"), "remote\n");
        let local = scan_tree(local_dir.path()).unwrap();
        let remote = scan_tree(remote_dir.path()).unwrap();
        let conflicts = detect_conflicts(&local, &remote);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].skill, "alpha");
        assert_ne!(conflicts[0].local_tree_hash, conflicts[0].remote_tree_hash);
        assert!(!conflicts[0].remote_updated_at.is_empty());
    }

    #[test]
    fn unpaired_skill_is_not_a_hash_conflict() {
        let local_dir = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        write_file(&local_dir.path().join("alpha/SKILL.md"), "local\n");
        write_file(&remote_dir.path().join("beta/SKILL.md"), "remote\n");
        let local = scan_tree(local_dir.path()).unwrap();
        let remote = scan_tree(remote_dir.path()).unwrap();
        assert!(detect_conflicts(&local, &remote).is_empty());
    }

    #[test]
    fn keep_local_overwrites_remote() {
        assert_eq!(
            SyncAction::from(Resolution::KeepLocal),
            SyncAction::OverwriteRemote
        );
        assert_eq!(
            SyncAction::from(Resolution::KeepRemote),
            SyncAction::TakeRemote
        );
    }
}
