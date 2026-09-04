//! End-to-end hash sync against `/v1` contracts.
//!
//! POST /v1/sync → scrub + PUT /v1/blobs/:hash → keep-local/keep-remote →
//! PUT /v1/skills/:name/tree → GET /v1/blobs/:hash → re-POST if resolved.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::SystemTime;

use crate::api::{ApiClient, SkillTreePut, SyncRequest, SyncResponse};
use crate::config::Paths;
use crate::error::{Result, SklError};
use crate::hooks::conflict::{ConflictChoice, ConflictMode, ConflictResolution};
use crate::hooks::{conflict, scrub};
use crate::local::db::{LocalDb, SyncSummary};
use crate::local::skills::{default_pull_root, hash_bytes, write_blob_file};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOptions {
    pub conflict: ConflictMode,
    pub allow_warnings: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            conflict: ConflictMode::Defer,
            allow_warnings: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub uploaded: Vec<String>,
    pub downloaded: Vec<String>,
    pub pushed: Vec<String>,
    pub conflicts: usize,
    pub missing_skills: Vec<String>,
}

pub async fn run(api_base: String, opts: SyncOptions) -> Result<()> {
    let token = crate::auth::load_device_token()?;
    let paths = Paths::resolve()?;
    let home = crate::config::home_dir()?;
    run_with_opts(&api_base, &token, &paths, &home, opts).await?;
    Ok(())
}

pub async fn run_with(
    api_base: &str,
    token: &str,
    paths: &Paths,
    home: &Path,
) -> Result<SyncOutcome> {
    run_with_opts(api_base, token, paths, home, SyncOptions::default()).await
}

pub async fn run_with_opts(
    api_base: &str,
    token: &str,
    paths: &Paths,
    home: &Path,
    opts: SyncOptions,
) -> Result<SyncOutcome> {
    if !paths.db_file.exists() {
        return Err(SklError::LocalState(
            "no local skill index; run `skl init` first".into(),
        ));
    }

    let db = LocalDb::open(&paths.db_file)?;
    let body = db.sync_request()?;

    scrub::scrub_before_upload(&db, &body, opts.allow_warnings)?;

    let client = ApiClient::new(api_base)?.with_token(token);
    eprintln!("POST {api_base}/v1/sync  ({} skill(s))", body.skills.len());
    let plan = client.sync(&body).await?;

    eprintln!("upload:         {} blob(s)", plan.upload.len());
    eprintln!("download:       {} blob(s)", plan.download.len());
    eprintln!("conflicts:      {}", plan.conflicts.len());
    eprintln!("missing_skills: {}", plan.missing_skills.len());

    let mtimes = local_mtimes(&db, &plan)?;
    let resolutions = conflict::resolve_conflicts(&plan.conflicts, opts.conflict, &mtimes)?;
    for resolution in &resolutions {
        match resolution.choice {
            ConflictChoice::KeepLocal => eprintln!("keep-local: {}", resolution.skill),
            ConflictChoice::KeepRemote => eprintln!("keep-remote: {}", resolution.skill),
            ConflictChoice::Unresolved => {}
        }
    }
    let keep_local = names_with(&resolutions, ConflictChoice::KeepLocal);
    let keep_remote = names_with(&resolutions, ConflictChoice::KeepRemote);

    let uploaded = upload_blobs(&client, &db, &plan, &keep_remote, opts.allow_warnings).await?;
    let pushed = push_trees(&client, &body, &plan, &keep_local).await?;
    let downloaded = download_blobs(&client, &db, home, &plan, &keep_remote).await?;

    let mut remaining_conflicts = plan
        .conflicts
        .iter()
        .filter(|c| !keep_local.contains(&c.skill) && !keep_remote.contains(&c.skill))
        .count();

    if !keep_local.is_empty() || !keep_remote.is_empty() {
        refresh_local_index(&db, home)?;
        let retry = db.sync_request()?;
        eprintln!(
            "re-POST {api_base}/v1/sync  ({} skill(s) after keep-local/keep-remote)",
            retry.skills.len()
        );
        let plan2 = client.sync(&retry).await?;
        remaining_conflicts = plan2.conflicts.len();
        eprintln!("re-POST conflicts: {}", plan2.conflicts.len());
    }

    refresh_local_index(&db, home)?;
    let summary = SyncSummary {
        uploaded: uploaded.len(),
        downloaded: downloaded.len(),
        pushed: pushed.len(),
        conflicts: remaining_conflicts,
        missing_skills: plan.missing_skills.len(),
    };
    db.record_sync_summary(&summary)?;

    eprintln!();
    eprintln!(
        "sync done  uploaded={}  downloaded={}  pushed={}  conflicts={}",
        summary.uploaded, summary.downloaded, summary.pushed, summary.conflicts
    );

    Ok(SyncOutcome {
        uploaded,
        downloaded,
        pushed,
        conflicts: remaining_conflicts,
        missing_skills: plan.missing_skills.clone(),
    })
}

fn names_with(resolutions: &[ConflictResolution], choice: ConflictChoice) -> BTreeSet<String> {
    resolutions
        .iter()
        .filter(|r| r.choice == choice)
        .map(|r| r.skill.clone())
        .collect()
}

fn local_mtimes(
    db: &LocalDb,
    plan: &SyncResponse,
) -> Result<BTreeMap<String, Option<SystemTime>>> {
    let mut out = BTreeMap::new();
    for conflict in &plan.conflicts {
        let ts = match db.find_skill(&conflict.skill)? {
            Some(skill) => latest_mtime(&skill.path),
            None => None,
        };
        out.insert(conflict.skill.clone(), ts);
    }
    Ok(out)
}

fn latest_mtime(dir: &Path) -> Option<SystemTime> {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

async fn upload_blobs(
    client: &ApiClient,
    db: &LocalDb,
    plan: &SyncResponse,
    skip_skills: &BTreeSet<String>,
    allow_warnings: bool,
) -> Result<Vec<String>> {
    let mut uploaded = Vec::new();
    for hash in &plan.upload {
        let (skill_dir, rel) = db.find_file_by_hash(hash)?.ok_or_else(|| {
            SklError::LocalState(format!("no local file for upload hash {hash}"))
        })?;
        if let Some(skill) = skill_name_for_dir(db, &skill_dir)? {
            if skip_skills.contains(&skill) {
                continue;
            }
        }
        let path = skill_dir.join(&rel);
        let bytes = std::fs::read(&path)?;
        if hash_bytes(&bytes) != *hash {
            return Err(SklError::LocalState(format!(
                "local file {} hash mismatch for {hash}",
                path.display()
            )));
        }
        scrub::scrub_blob_before_upload(hash, &bytes, allow_warnings)?;
        eprintln!("PUT /v1/blobs/{hash}  ({} bytes)", bytes.len());
        let put = client.put_blob(hash, bytes).await?;
        if put.hash != *hash {
            return Err(SklError::LocalState(format!(
                "API returned hash {} for uploaded {hash}",
                put.hash
            )));
        }
        uploaded.push(hash.clone());
    }
    Ok(uploaded)
}

fn skill_name_for_dir(db: &LocalDb, skill_dir: &Path) -> Result<Option<String>> {
    Ok(db
        .list_skills()?
        .into_iter()
        .find(|s| s.path == skill_dir)
        .map(|s| s.name))
}

async fn push_trees(
    client: &ApiClient,
    body: &SyncRequest,
    plan: &SyncResponse,
    keep_local: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let mut skip: BTreeSet<&str> = plan.conflicts.iter().map(|c| c.skill.as_str()).collect();
    for name in keep_local {
        skip.remove(name.as_str());
    }
    let mut pushed = Vec::new();
    for (name, tree) in &body.skills {
        if skip.contains(name.as_str()) {
            continue;
        }
        let commit = SkillTreePut {
            tree_hash: tree.tree_hash.clone(),
            files: tree.files.clone(),
        };
        eprintln!("PUT /v1/skills/{name}/tree");
        let res = client.put_skill_tree(name, &commit).await?;
        if res.tree_hash != tree.tree_hash {
            return Err(SklError::LocalState(format!(
                "tree commit mismatch for {name}: local={} remote={}",
                tree.tree_hash, res.tree_hash
            )));
        }
        pushed.push(name.clone());
    }
    Ok(pushed)
}

async fn download_blobs(
    client: &ApiClient,
    db: &LocalDb,
    home: &Path,
    plan: &SyncResponse,
    keep_remote: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let mut blob_cache: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for item in &plan.download {
        eprintln!(
            "GET /v1/blobs/{}  skills={:?}  paths={:?}",
            item.hash, item.skills, item.paths
        );
        let bytes = client.get_blob(&item.hash).await?;
        if hash_bytes(&bytes) != item.hash {
            return Err(SklError::LocalState(format!(
                "downloaded blob {} failed hash check",
                item.hash
            )));
        }
        blob_cache.insert(item.hash.clone(), bytes);
    }

    // download.skills / download.paths are sets, not pairs. Place files from
    // GET /v1/skills/:name so each path maps to the correct hash.
    let mut skill_names: BTreeSet<String> = BTreeSet::new();
    for item in &plan.download {
        skill_names.extend(item.skills.iter().cloned());
    }
    skill_names.extend(plan.missing_skills.iter().cloned());
    skill_names.extend(keep_remote.iter().cloned());

    let pull_root = default_pull_root(home);
    for name in &skill_names {
        let conflicted = plan.conflicts.iter().any(|c| c.skill == *name);
        if conflicted && !keep_remote.contains(name) {
            continue;
        }
        let detail = client.get_skill(name).await?;
        let dest = match db.find_skill(name)? {
            Some(existing) => existing.path,
            None => pull_root.join(name),
        };
        std::fs::create_dir_all(&dest)?;
        for (rel, hash) in &detail.files {
            let bytes = if let Some(cached) = blob_cache.get(hash) {
                cached.clone()
            } else {
                let fetched = client.get_blob(hash).await?;
                if hash_bytes(&fetched) != *hash {
                    return Err(SklError::LocalState(format!(
                        "downloaded blob {hash} failed hash check"
                    )));
                }
                blob_cache.insert(hash.clone(), fetched.clone());
                fetched
            };
            write_blob_file(&dest, rel, &bytes)?;
        }
        eprintln!("wrote skill {name} → {}", dest.display());
    }

    Ok(blob_cache.keys().cloned().collect())
}

fn refresh_local_index(db: &LocalDb, home: &Path) -> Result<()> {
    let discovered = crate::local::skills::discover_from_home(home)?;
    db.replace_import(&discovered)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::SkillTree;
    use crate::config::Paths;
    use crate::local::skills::{tree_hash, DiscoveredSkill};
    use serde_json::json;
    use std::collections::BTreeMap;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn paths_for(tmp: &Path) -> Paths {
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            data_dir: tmp.join("data"),
            db_file: tmp.join("data/state.db"),
        }
    }

    #[tokio::test]
    async fn push_then_pull_loop() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# hello\n").unwrap();

        let bytes = b"# hello\n".to_vec();
        let blob_hash = hash_bytes(&bytes);
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), blob_hash.clone());
        let tree = tree_hash(&files);

        let paths = paths_for(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: skill_dir.clone(),
            tree: SkillTree {
                tree_hash: tree.clone(),
                files: files.clone(),
            },
        }])
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .and(header("authorization", "Bearer dev:alice"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [blob_hash],
                "download": [],
                "conflicts": [],
                "missing_skills": []
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path(format!("/v1/blobs/{blob_hash}")))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "hash": blob_hash,
                "size": bytes.len()
            })))
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/skills/greeter/tree"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "greeter",
                "tree_hash": tree,
                "updated_at": "2026-09-04T08:00:00.000Z"
            })))
            .mount(&server)
            .await;

        let outcome = run_with(&server.uri(), "dev:alice", &paths, &home)
            .await
            .unwrap();
        assert_eq!(outcome.uploaded, vec![blob_hash.clone()]);
        assert_eq!(outcome.pushed, vec!["greeter".to_string()]);
        assert_eq!(outcome.conflicts, 0);
    }

    #[tokio::test]
    async fn pull_writes_missing_skill() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join(".claude/skills")).unwrap();

        let bytes = b"# remote\n".to_vec();
        let blob_hash = hash_bytes(&bytes);
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), blob_hash.clone());
        let tree = tree_hash(&files);

        let paths = paths_for(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        LocalDb::open(&paths.db_file).unwrap().replace_import(&[]).unwrap();

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [{
                    "hash": blob_hash,
                    "skills": ["greeter"],
                    "paths": ["SKILL.md"]
                }],
                "conflicts": [],
                "missing_skills": ["greeter"]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/v1/blobs/{blob_hash}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(bytes.clone()),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/skills/greeter"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "greeter",
                "tree_hash": tree,
                "files": { "SKILL.md": blob_hash },
                "updated_at": "2026-09-04T08:00:00.000Z"
            })))
            .mount(&server)
            .await;

        let outcome = run_with(&server.uri(), "dev:alice", &paths, &home)
            .await
            .unwrap();
        assert_eq!(outcome.downloaded, vec![blob_hash]);
        assert_eq!(outcome.missing_skills, vec!["greeter".to_string()]);
        let written = home.join(".claude/skills/greeter/SKILL.md");
        assert_eq!(std::fs::read(written).unwrap(), b"# remote\n");
    }

    #[tokio::test]
    async fn conflict_skips_tree_put() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "local").unwrap();

        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), hash_bytes(b"local"));
        let tree = tree_hash(&files);

        let paths = paths_for(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: skill_dir,
            tree: SkillTree {
                tree_hash: tree.clone(),
                files,
            },
        }])
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [{
                    "skill": "greeter",
                    "local_tree_hash": tree,
                    "remote_tree_hash": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "remote_updated_at": "2026-09-04T08:00:00.000Z"
                }],
                "missing_skills": []
            })))
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/skills/greeter/tree"))
            .respond_with(ResponseTemplate::new(500).set_body_string("should not put tree"))
            .mount(&server)
            .await;

        let outcome = run_with(&server.uri(), "dev:alice", &paths, &home)
            .await
            .unwrap();
        assert_eq!(outcome.conflicts, 1);
        assert!(outcome.pushed.is_empty());
    }

    #[tokio::test]
    async fn keep_local_puts_tree_and_reposts() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "local").unwrap();

        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), hash_bytes(b"local"));
        let tree = tree_hash(&files);

        let paths = paths_for(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: skill_dir,
            tree: SkillTree {
                tree_hash: tree.clone(),
                files,
            },
        }])
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [{
                    "skill": "greeter",
                    "local_tree_hash": tree,
                    "remote_tree_hash": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "remote_updated_at": "2026-09-04T08:00:00.000Z"
                }],
                "missing_skills": []
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/skills/greeter/tree"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "greeter",
                "tree_hash": tree,
                "updated_at": "2026-09-04T08:01:00.000Z"
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [],
                "missing_skills": []
            })))
            .mount(&server)
            .await;

        let outcome = run_with_opts(
            &server.uri(),
            "dev:alice",
            &paths,
            &home,
            SyncOptions {
                conflict: crate::hooks::conflict::ConflictMode::KeepLocal,
                allow_warnings: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(outcome.pushed, vec!["greeter".to_string()]);
        assert_eq!(outcome.conflicts, 0);
    }

    #[tokio::test]
    async fn keep_remote_writes_remote_files_and_reposts() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "local").unwrap();

        let local_hash = hash_bytes(b"local");
        let remote_bytes = b"# remote clash\n".to_vec();
        let remote_hash = hash_bytes(&remote_bytes);
        let mut local_files = BTreeMap::new();
        local_files.insert("SKILL.md".into(), local_hash.clone());
        let local_tree = tree_hash(&local_files);
        let mut remote_files = BTreeMap::new();
        remote_files.insert("SKILL.md".into(), remote_hash.clone());
        let remote_tree = tree_hash(&remote_files);

        let paths = paths_for(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: skill_dir.clone(),
            tree: SkillTree {
                tree_hash: local_tree.clone(),
                files: local_files,
            },
        }])
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [{
                    "skill": "greeter",
                    "local_tree_hash": local_tree,
                    "remote_tree_hash": remote_tree,
                    "remote_updated_at": "2026-09-04T08:00:00.000Z"
                }],
                "missing_skills": []
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/skills/greeter"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "greeter",
                "tree_hash": remote_tree,
                "files": { "SKILL.md": remote_hash },
                "updated_at": "2026-09-04T08:00:00.000Z"
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/v1/blobs/{remote_hash}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(remote_bytes.clone()),
            )
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [],
                "missing_skills": []
            })))
            .mount(&server)
            .await;

        let outcome = run_with_opts(
            &server.uri(),
            "dev:alice",
            &paths,
            &home,
            SyncOptions {
                conflict: crate::hooks::conflict::ConflictMode::KeepRemote,
                allow_warnings: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(outcome.conflicts, 0);
        assert_eq!(
            std::fs::read(skill_dir.join("SKILL.md")).unwrap(),
            b"# remote clash\n"
        );
    }

    #[tokio::test]
    async fn dirty_blob_is_not_put() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let pem = b"-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n-----END OPENSSH PRIVATE KEY-----\n";
        std::fs::write(skill_dir.join("SKILL.md"), pem).unwrap();

        let blob_hash = hash_bytes(pem);
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), blob_hash.clone());
        let tree = tree_hash(&files);

        let paths = paths_for(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: skill_dir,
            tree: SkillTree {
                tree_hash: tree,
                files,
            },
        }])
        .unwrap();

        let err = run_with(&server.uri(), "dev:alice", &paths, &home)
            .await
            .unwrap_err();
        assert!(matches!(err, SklError::BlockedSecrets(_)));
    }
}
