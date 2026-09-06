//! `skl delete` — unmanage a skill remotely and remove the local library copy.

use std::fs;
use std::path::Path;

use crate::api::ApiClient;
use crate::auth;
use crate::config::Paths;
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;
use crate::local::linker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDelete {
    pub removed_library: bool,
    pub unindexed: bool,
}

impl LocalDelete {
    pub fn changed(self) -> bool {
        self.removed_library || self.unindexed
    }
}

pub async fn run(names: &[String], api_base: &str) -> Result<()> {
    if names.is_empty() {
        return Err(SklError::LocalState(
            "specify at least one skill: `skl delete <skill>`".into(),
        ));
    }
    for name in names {
        linker::validate_skill_name(name)?;
    }

    let token = auth::load_device_token()?;
    let client = ApiClient::new(api_base)?.with_token(token);
    let paths = Paths::resolve()?;

    for name in names {
        let remote_missing = match client.delete_skill(name).await {
            Ok(()) => false,
            Err(SklError::Api { status: 404, .. }) => true,
            Err(err) => return Err(err),
        };
        let local = remove_local(name, &paths)?;
        if remote_missing && !local.changed() {
            return Err(SklError::LocalState(format!(
                "skill `{name}` is not managed by SKL"
            )));
        }
        eprintln!("deleted {name}");
    }
    Ok(())
}

/// Remove `{data_dir}/skills/<name>/` and drop every `state.db` row for `name`.
/// Home discovery dirs (e.g. `~/.claude/skills/<name>`) and project dests stay.
pub fn remove_local(name: &str, paths: &Paths) -> Result<LocalDelete> {
    linker::validate_skill_name(name)?;
    let removed_library = remove_library_skill(&paths.library_skill(name), &paths.library_dir())?;
    let unindexed = if paths.db_file.exists() {
        LocalDb::open(&paths.db_file)?.remove_skill(name)?
    } else {
        false
    };
    Ok(LocalDelete {
        removed_library,
        unindexed,
    })
}

fn remove_library_skill(path: &Path, library_dir: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if !is_inside_library(path, library_dir) {
        return Err(SklError::LocalState(format!(
            "refusing to delete {} (not under the personal library)",
            path.display()
        )));
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || meta.is_file() {
        fs::remove_file(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }
    Ok(true)
}

fn is_inside_library(path: &Path, library_dir: &Path) -> bool {
    let Ok(lib) = library_dir.canonicalize() else {
        return false;
    };
    let Ok(skill) = path.canonicalize() else {
        return false;
    };
    skill.starts_with(&lib) && skill != lib
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::skills::{hash_skill_dir, DiscoveredSkill};

    fn isolated_paths(tmp: &Path) -> Paths {
        let data_dir = tmp.join("data");
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            db_file: data_dir.join("state.db"),
            data_dir,
        }
    }

    fn plant(dir: &Path, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn remove_local_deletes_library_and_index() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let lib = paths.library_skill("greeter");
        plant(&lib, "# hello\n");
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.upsert_skill(&DiscoveredSkill {
            name: "greeter".into(),
            source: "agents".into(),
            path: lib.clone(),
            tree: hash_skill_dir(&lib).unwrap(),
        })
        .unwrap();

        let out = remove_local("greeter", &paths).unwrap();
        assert!(out.removed_library);
        assert!(out.unindexed);
        assert!(!lib.exists());
        assert!(db.find_skill("greeter").unwrap().is_none());
    }

    #[test]
    fn remove_local_leaves_home_discovery_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home/.claude/skills/greeter");
        plant(&home, "# home\n");
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.upsert_skill(&DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: home.clone(),
            tree: hash_skill_dir(&home).unwrap(),
        })
        .unwrap();

        let out = remove_local("greeter", &paths).unwrap();
        assert!(!out.removed_library);
        assert!(out.unindexed);
        assert!(home.join("SKILL.md").is_file());
        assert!(db.find_skill("greeter").unwrap().is_none());
    }

    #[test]
    fn remove_local_rejects_invalid_name() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let err = remove_local("../etc", &paths).unwrap_err();
        assert!(err.to_string().contains("invalid skill name"), "{err}");
    }
}
