//! End-to-end hash sync against `/v1` contracts.
//!
//! POST /v1/sync → PUT /v1/blobs/:hash → PUT /v1/skills/:name/tree →
//! GET /v1/blobs/:hash. Conflicts are printed only; hammer owns resolve/scrub.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::api::{ApiClient, SkillTreePut, SyncRequest, SyncResponse};
use crate::config::Paths;
use crate::error::{Result, SklError};
use crate::hooks::{conflict, scrub};
use crate::local::db::{LocalDb, SyncSummary};
use crate::local::skills::{default_pull_root, hash_bytes, write_blob_file};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub uploaded: Vec<String>,
    pub downloaded: Vec<String>,
    pub pushed: Vec<String>,
    pub conflicts: usize,
    pub missing_skills: Vec<String>,
}

pub async fn run(api_base: String) -> Result<()> {
    let token = crate::auth::load_device_token()?;
    let paths = Paths::resolve()?;
    let home = crate::config::home_dir()?;
    run_with(&api_base, &token, &paths, &home).await?;
    Ok(())
}

pub async fn run_with(
    api_base: &str,
    token: &str,
    paths: &Paths,
    home: &Path,
) -> Result<SyncOutcome> {
    if !paths.db_file.exists() {
        return Err(SklError::LocalState(
            "no local skill index; run `skl init` first".into(),
        ));
    }

    let db = LocalDb::open(&paths.db_file)?;
    let body = db.sync_request()?;

    // TODO(hammer/secret-scrub): call site — inspect inventory before POST /v1/sync.
    scrub::scrub_before_upload(&body)?;

    let client = ApiClient::new(api_base)?.with_token(token);
    eprintln!("POST {api_base}/v1/sync  ({} skill(s))", body.skills.len());
    let plan = client.sync(&body).await?;

    eprintln!("upload:         {} blob(s)", plan.upload.len());
    eprintln!("download:       {} blob(s)", plan.download.len());
    eprintln!("conflicts:      {}", plan.conflicts.len());
    eprintln!("missing_skills: {}", plan.missing_skills.len());

    let uploaded = upload_blobs(&client, &db, &plan).await?;
    let pushed = push_trees(&client, &body, &plan).await?;
    let downloaded = download_blobs(&client, &db, home, &plan).await?;

    // TODO(hammer/conflict): call site — keep-local / keep-remote only.
    let _resolutions = conflict::resolve_conflicts(&plan.conflicts);

    refresh_local_index(&db, home)?;
    let summary = SyncSummary {
        uploaded: uploaded.len(),
        downloaded: downloaded.len(),
        pushed: pushed.len(),
        conflicts: plan.conflicts.len(),
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
        conflicts: plan.conflicts.len(),
        missing_skills: plan.missing_skills.clone(),
    })
}

async fn upload_blobs(
    client: &ApiClient,
    db: &LocalDb,
    plan: &SyncResponse,
) -> Result<Vec<String>> {
    let mut uploaded = Vec::new();
    for hash in &plan.upload {
        let (skill_dir, rel) = db.find_file_by_hash(hash)?.ok_or_else(|| {
            SklError::LocalState(format!("no local file for upload hash {hash}"))
        })?;
        let path = skill_dir.join(&rel);
        let bytes = std::fs::read(&path)?;
        if hash_bytes(&bytes) != *hash {
            return Err(SklError::LocalState(format!(
                "local file {} hash mismatch for {hash}",
                path.display()
            )));
        }
        // TODO(hammer/secret-scrub): call site — inspect bytes before PUT /v1/blobs/:hash.
        scrub::scrub_blob_before_upload(hash, &bytes)?;
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

async fn push_trees(
    client: &ApiClient,
    body: &SyncRequest,
    plan: &SyncResponse,
) -> Result<Vec<String>> {
    let conflicted: BTreeSet<&str> = plan.conflicts.iter().map(|c| c.skill.as_str()).collect();
    let mut pushed = Vec::new();
    for (name, tree) in &body.skills {
        if conflicted.contains(name.as_str()) {
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
) -> Result<Vec<String>> {
    if plan.download.is_empty() {
        return Ok(Vec::new());
    }

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

    let pull_root = default_pull_root(home);
    for name in &skill_names {
        if plan.conflicts.iter().any(|c| c.skill == *name) {
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
}
