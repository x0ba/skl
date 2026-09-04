//! Stub sync: POST /v1/sync with the local inventory.
//!
//! Blob upload/download, tree commit, and conflict resolution are left for
//! hammer / the next pass (TODOs at the call sites below).

use crate::api::{ApiClient, SkillTreePut};
use crate::auth;
use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;

pub async fn run(api_base: String) -> Result<()> {
    let token = auth::load_device_token()?;
    let paths = Paths::resolve()?;
    if !paths.db_file.exists() {
        return Err(SklError::LocalState(
            "no local skill index; run `skl init` first".into(),
        ));
    }

    let db = LocalDb::open(&paths.db_file)?;
    let body = db.sync_request()?;
    let client = ApiClient::new(&api_base)?.with_token(token);

    eprintln!(
        "POST {api_base}/v1/sync  ({} skill(s))",
        body.skills.len()
    );
    let plan = client.sync(&body).await?;

    eprintln!("upload:         {} blob(s)", plan.upload.len());
    eprintln!("download:       {} blob(s)", plan.download.len());
    eprintln!("conflicts:      {}", plan.conflicts.len());
    eprintln!("missing_skills: {}", plan.missing_skills.len());

    if !plan.upload.is_empty() {
        eprintln!();
        eprintln!("TODO(hammer/next): PUT /v1/blobs/:hash for each upload hash");
        for hash in &plan.upload {
            eprintln!("  upload {hash}");
            // TODO: read local file bytes by hash and call client.put_blob(hash, bytes).await?
        }
        eprintln!("TODO(hammer/next): PUT /v1/skills/:name/tree after uploads");
        for (name, tree) in &body.skills {
            let _commit = SkillTreePut {
                tree_hash: tree.tree_hash.clone(),
                files: tree.files.clone(),
            };
            let _ = name;
            // TODO: client.put_skill_tree(name, &_commit).await?;
        }
    }

    if !plan.download.is_empty() {
        eprintln!();
        eprintln!("TODO(hammer/next): GET /v1/blobs/:hash and write files (link, do not copy, at use-time)");
        for item in &plan.download {
            eprintln!(
                "  download {}  skills={:?}  paths={:?}",
                item.hash, item.skills, item.paths
            );
            // TODO: let bytes = client.get_blob(&item.hash).await?;
        }
    }

    if !plan.conflicts.is_empty() {
        eprintln!();
        eprintln!("TODO(hammer): resolve tree-hash conflicts");
        for conflict in &plan.conflicts {
            eprintln!(
                "  {}  local={}  remote={}",
                conflict.skill, conflict.local_tree_hash, conflict.remote_tree_hash
            );
        }
    }

    if !plan.missing_skills.is_empty() {
        eprintln!("missing on remote: {}", plan.missing_skills.join(", "));
    }

    let _ = config::load(&paths);
    Ok(())
}
