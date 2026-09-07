//! `skl edit` — open a personal-library skill in `$VISUAL` / `$EDITOR`.
//!
//! The library is canonical. This command never opens a project projection.

use std::path::{Path, PathBuf};

use crate::commands::create::{self, CreateOutcome};
use crate::commands::use_cmd;
use crate::config::Paths;
use crate::error::{Result, SklError};
use crate::local::library;
use crate::local::linker;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    pub name: String,
    pub library_path: PathBuf,
    pub skill_md: PathBuf,
}

pub async fn run(name: &str, api_base: &str) -> Result<()> {
    let paths = Paths::resolve()?;
    run_with(name, &paths, api_base).await
}

pub async fn run_with(name: &str, paths: &Paths, api_base: &str) -> Result<()> {
    let out = resolve_library_edit(name, paths)?;
    eprintln!("editing {}  ({})", out.name, out.skill_md.display());
    crate::editor::open_required(&out.skill_md)?;
    if let Err(err) = create::reindex(
        &CreateOutcome {
            name: out.name.clone(),
            library_path: out.library_path.clone(),
            skill_md: out.skill_md.clone(),
        },
        paths,
    ) {
        eprintln!("{}", create::stale_index_hint(&err));
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(hint) = refresh_hint(&out.name, &cwd) {
            eprintln!("{hint}");
        }
    }
    let _ = crate::auto_sync::maybe_run(api_base, paths, "edit").await;
    Ok(())
}

/// Resolve `<name>` to the library `SKILL.md`. Never a project dest.
pub fn resolve_library_edit(name: &str, paths: &Paths) -> Result<EditOutcome> {
    linker::validate_skill_name(name)?;
    let skill = use_cmd::resolve_skill(name, Path::new(""), Some(&paths.db_file))?;
    if !library::is_under_library(&skill.path, &paths.library_dir()) {
        return Err(SklError::LocalState(format!(
            "skill `{name}` is not in the personal library on this machine. Run `skl init` or `skl capture` to import it, or `skl sync` to pull the library."
        )));
    }
    let skill_md = skill.path.join("SKILL.md");
    if !skill_md.is_file() {
        return Err(SklError::LocalState(format!(
            "library skill `{name}` is missing SKILL.md at {}",
            skill.path.display()
        )));
    }
    Ok(EditOutcome {
        name: skill.name,
        library_path: skill.path,
        skill_md,
    })
}

/// After a library edit, nudge `skl use --all` when cwd/git root already manages this skill.
pub fn refresh_hint(skill: &str, start: &Path) -> Option<String> {
    let project = managed_project_root(start)?;
    let manifest = linker::load_manifest(&project).ok()?;
    if !manifest.skills.iter().any(|entry| entry.name == skill) {
        return None;
    }
    Some(format!(
        "hint: `{skill}` is activated here; run `skl use --all` to refresh projections"
    ))
}

/// Git root when it has `skills.toml`; otherwise `start` when it does.
pub fn managed_project_root(start: &Path) -> Option<PathBuf> {
    if let Some(root) = find_git_root(start) {
        if linker::manifest_path(&root).is_file() {
            return Some(root);
        }
    }
    if linker::manifest_path(start).is_file() {
        return Some(start.to_path_buf());
    }
    None
}

pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let git = dir.join(".git");
        if git.is_dir() || git.is_file() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::db::LocalDb;
    use crate::local::skills::{hash_skill_dir, DiscoveredSkill};
    use std::fs;

    fn isolated_paths(tmp: &Path) -> Paths {
        let data_dir = tmp.join("data");
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            db_file: data_dir.join("state.db"),
            data_dir,
        }
    }

    fn plant_library(paths: &Paths, name: &str, body: &str) {
        let dir = paths.library_skill(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn resolves_library_skill_md_not_a_project_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        plant_library(&paths, "greeter", "# library\n");
        let project = tmp.path().join("proj/.agents/skills/greeter");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("SKILL.md"), "# project\n").unwrap();

        let out = resolve_library_edit("greeter", &paths).unwrap();
        assert_eq!(out.name, "greeter");
        assert_eq!(out.library_path, paths.library_skill("greeter"));
        assert_eq!(
            out.skill_md,
            paths.library_skill("greeter").join("SKILL.md")
        );
        assert!(out.skill_md.starts_with(&paths.data_dir));
        assert!(!out.skill_md.starts_with(tmp.path().join("proj")));
        assert_eq!(fs::read_to_string(&out.skill_md).unwrap(), "# library\n");
    }

    #[test]
    fn missing_library_skill_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let err = resolve_library_edit("ghost", &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("personal library"), "{err}");
        assert!(!err.contains("proj"), "{err}");
    }

    #[test]
    fn rejects_unsafe_name() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let err = resolve_library_edit("../etc", &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid skill name"), "{err}");
    }

    #[test]
    fn indexed_harness_path_is_not_editable() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        paths.ensure().unwrap();
        let foreign = tmp.path().join("elsewhere/greeter");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("SKILL.md"), "# foreign\n").unwrap();
        let tree = hash_skill_dir(&foreign).unwrap();
        LocalDb::open(&paths.db_file)
            .unwrap()
            .upsert_skill(&DiscoveredSkill {
                name: "greeter".into(),
                source: "claude".into(),
                path: foreign,
                tree,
            })
            .unwrap();

        let err = resolve_library_edit("greeter", &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("personal library"), "{err}");
    }

    #[test]
    fn hint_when_git_root_lists_the_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            linker::manifest_path(&root),
            "[[skills]]\nname = \"greeter\"\nmode = \"copy\"\n",
        )
        .unwrap();

        let hint = refresh_hint("greeter", &nested).expect("hint");
        assert!(hint.contains("skl use --all"), "{hint}");
        assert!(hint.contains("greeter"), "{hint}");
        assert!(refresh_hint("other", &nested).is_none());
    }

    #[test]
    fn hint_when_cwd_has_manifest_without_git() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            linker::manifest_path(&project),
            "[[skills]]\nname = \"notes\"\nmode = \"copy\"\n",
        )
        .unwrap();
        assert!(refresh_hint("notes", &project)
            .unwrap()
            .contains("skl use --all"));
        assert!(refresh_hint("notes", tmp.path()).is_none());
    }

    #[tokio::test]
    async fn editor_unset_fails_before_mutating() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        plant_library(&paths, "greeter", "# keep\n");
        let _iso = crate::editor::IsolatedEditorEnv::enter();

        let err = run_with("greeter", &paths, "http://127.0.0.1:1")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("$VISUAL") && err.contains("$EDITOR"), "{err}");
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# keep\n"
        );
    }

    #[tokio::test]
    async fn editor_success_reindexes() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        plant_library(&paths, "greeter", "# before\n");
        let before = hash_skill_dir(&paths.library_skill("greeter")).unwrap();
        LocalDb::open(&paths.db_file)
            .unwrap()
            .upsert_skill(&DiscoveredSkill {
                name: "greeter".into(),
                source: library::LIBRARY_SOURCE.into(),
                path: paths.library_skill("greeter"),
                tree: before.clone(),
            })
            .unwrap();

        let _iso = crate::editor::IsolatedEditorEnv::enter();
        // `true` exits 0 and ignores the path; mutate first so reindex sees new bytes.
        fs::write(paths.library_skill("greeter").join("SKILL.md"), "# after\n").unwrap();
        std::env::set_var("EDITOR", "true");

        run_with("greeter", &paths, "http://127.0.0.1:1")
            .await
            .expect("edit must succeed when $EDITOR exits 0");
        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "greeter");
        assert_ne!(listed[0].tree.tree_hash, before.tree_hash);
    }
}
