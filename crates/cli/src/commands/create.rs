//! `skl create` — start a personal-library skill and open it in `$EDITOR`.
//!
//! Canonical library: `{Paths.data_dir}/skills/<name>/`
//! (default `~/.local/share/skl/skills/`, override with `SKL_DATA_DIR`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Paths;
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;
use crate::local::library;
use crate::local::linker;
use crate::local::skills::{self, DiscoveredSkill};

/// Indexed `source` for skills stored in the personal library.
pub const LIBRARY_SOURCE: &str = library::LIBRARY_SOURCE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOutcome {
    pub name: String,
    pub library_path: PathBuf,
    pub skill_md: PathBuf,
}

/// Frontmatter-only starter. Body is left blank for the editor.
pub fn skill_template(name: &str) -> String {
    format!("---\nname: {name}\ndescription: \n---\n\n")
}

pub async fn run(name: &str, api_base: &str) -> Result<()> {
    let paths = Paths::resolve()?;
    run_with(name, &paths, api_base).await
}

/// Local create + editor + fail-soft piggyback. Auto-sync errors never fail the verb.
pub async fn run_with(name: &str, paths: &Paths, api_base: &str) -> Result<()> {
    let out = create_library_skill(name, paths)?;
    eprintln!("created {}  ({})", out.name, out.library_path.display());
    crate::editor::open(&out.skill_md)?;
    reindex(&out, paths)?;
    let _ = crate::auto_sync::maybe_run(api_base, paths, "create").await;
    Ok(())
}

/// Re-hash after the editor returns so `state.db` matches the written tree.
pub fn reindex(out: &CreateOutcome, paths: &Paths) -> Result<()> {
    index_library(&out.name, &out.library_path, paths)
}

/// Write `{data_dir}/skills/<name>/SKILL.md` and index it. Does not open an editor.
///
/// Never prompts. Name clash is an error (edit the existing skill instead).
pub fn create_library_skill(name: &str, paths: &Paths) -> Result<CreateOutcome> {
    linker::validate_skill_name(name)?;
    paths.ensure()?;
    fs::create_dir_all(paths.library_dir())?;
    let library_path = paths.library_skill(name);
    if path_exists(&library_path) {
        return Err(SklError::LocalState(format!(
            "skill `{name}` already exists in the personal library at {}",
            library_path.display()
        )));
    }

    fs::create_dir_all(&library_path)?;
    let skill_md = library_path.join("SKILL.md");
    fs::write(&skill_md, skill_template(name))?;
    index_library(name, &library_path, paths)?;

    Ok(CreateOutcome {
        name: name.to_string(),
        library_path,
        skill_md,
    })
}

fn index_library(name: &str, library_path: &Path, paths: &Paths) -> Result<()> {
    let tree = skills::hash_skill_dir(library_path)?;
    let db = LocalDb::open(&paths.db_file)?;
    db.upsert_skill(&DiscoveredSkill {
        name: name.to_string(),
        source: LIBRARY_SOURCE.to_string(),
        path: library_path.to_path_buf(),
        tree,
    })?;
    Ok(())
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_paths(tmp: &Path) -> Paths {
        let data_dir = tmp.join("data");
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            db_file: data_dir.join("state.db"),
            data_dir,
        }
    }

    #[test]
    fn template_is_name_and_description_frontmatter() {
        let body = skill_template("greeter");
        assert_eq!(body, "---\nname: greeter\ndescription: \n---\n\n");
        assert!(body.starts_with("---\n"));
        assert!(body.contains("name: greeter"));
        assert!(body.contains("description: "));
        assert!(!body.contains("disable-model-invocation"));
    }

    #[test]
    fn create_writes_library_skill_and_indexes() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());

        let out = create_library_skill("greeter", &paths).unwrap();
        assert_eq!(out.name, "greeter");
        assert_eq!(out.library_path, paths.library_skill("greeter"));
        assert_eq!(
            out.skill_md,
            paths.library_skill("greeter").join("SKILL.md")
        );
        assert_eq!(
            fs::read_to_string(&out.skill_md).unwrap(),
            skill_template("greeter")
        );

        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "greeter");
        assert_eq!(listed[0].source, LIBRARY_SOURCE);
        assert_eq!(listed[0].path, out.library_path);
        assert!(listed[0].tree.files.contains_key("SKILL.md"));
    }

    #[test]
    fn create_library_is_data_dir_skills_not_home_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let out = create_library_skill("notes", &paths).unwrap();
        let rendered = out.library_path.to_string_lossy();
        assert!(
            rendered.contains("data/skills/notes"),
            "library must be under data_dir: {rendered}"
        );
        assert!(
            !rendered.contains(".agents/skills"),
            "create must not write ~/.agents/skills: {rendered}"
        );
    }

    #[test]
    fn clash_without_overwrite_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        create_library_skill("greeter", &paths).unwrap();
        let err = create_library_skill("greeter", &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            skill_template("greeter")
        );
    }

    #[test]
    fn rejects_unsafe_name() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let err = create_library_skill("../etc", &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid skill name"), "{err}");
        assert!(
            !paths.library_dir().exists()
                || paths.library_dir().read_dir().unwrap().next().is_none()
        );
    }

    #[tokio::test]
    async fn fail_soft_dead_api_does_not_fail_create() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        create_library_skill("greeter", &paths).unwrap();
        assert!(paths.library_skill("greeter").join("SKILL.md").is_file());
        let sync = crate::auto_sync::maybe_run("http://127.0.0.1:1", &paths, "create").await;
        assert!(
            matches!(
                sync,
                crate::auto_sync::AutoSyncResult::Skipped { .. }
                    | crate::auto_sync::AutoSyncResult::FailedSoft { .. }
            ),
            "{sync:?}"
        );
    }

    #[test]
    fn upsert_does_not_wipe_other_indexed_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let other = tmp.path().join("other/alpha");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("SKILL.md"), "# alpha\n").unwrap();
        let tree = skills::hash_skill_dir(&other).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.upsert_skill(&DiscoveredSkill {
            name: "alpha".into(),
            source: "claude".into(),
            path: other,
            tree,
        })
        .unwrap();

        create_library_skill("greeter", &paths).unwrap();

        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        let names: Vec<_> = listed
            .iter()
            .map(|s| (s.source.as_str(), s.name.as_str()))
            .collect();
        assert!(names.contains(&("claude", "alpha")));
        assert!(names.contains(&(LIBRARY_SOURCE, "greeter")));
    }
}
