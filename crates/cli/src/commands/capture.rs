//! `skl capture` — promote a project-local skill into the personal library.
//!
//! Canonical library: `{Paths.data_dir}/skills/<name>/`
//! (default `~/.local/share/skl/skills/`, override with `SKL_DATA_DIR`).
//!
//! `.agents/skills` is the project link destination (and a home discovery
//! root for `skl init`). It is **not** the personal library.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;
use crate::local::linker;
use crate::local::skills::{self, DiscoveredSkill};

use super::use_cmd::{resolve_activation_extras, resolve_project};

/// Indexed `source` for skills stored in the personal library.
pub const LIBRARY_SOURCE: &str = "agents";

#[derive(Debug, Clone, Default)]
pub struct CaptureOpts {
    pub force: bool,
    pub as_name: Option<String>,
    pub keep_copy: bool,
    pub project: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureAction {
    Captured,
    Forced,
    KeepCopy,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    pub name: String,
    pub source_path: PathBuf,
    pub library_path: PathBuf,
    pub action: CaptureAction,
}

pub async fn run(path: PathBuf, opts: CaptureOpts, api_base: &str) -> Result<()> {
    let paths = Paths::resolve()?;
    run_with(path, opts, &paths, api_base).await
}

/// Local capture + fail-soft piggyback. Auto-sync errors never fail the verb.
pub async fn run_with(
    path: PathBuf,
    opts: CaptureOpts,
    paths: &Paths,
    api_base: &str,
) -> Result<()> {
    paths.ensure()?;
    let out = capture(&path, &opts, paths)?;
    report(&out);
    let _ = crate::auto_sync::maybe_run(api_base, paths, "capture").await;
    Ok(())
}

fn report(out: &CaptureOutcome) {
    match out.action {
        CaptureAction::Noop => eprintln!(
            "captured {}  already linked to {}",
            out.name,
            out.library_path.display()
        ),
        CaptureAction::KeepCopy => eprintln!(
            "captured {}  ({})  (kept project copy)",
            out.name,
            out.library_path.display()
        ),
        CaptureAction::Forced => eprintln!(
            "captured {}  ({})  (overwrote library)",
            out.name,
            out.library_path.display()
        ),
        CaptureAction::Captured => {
            eprintln!("captured {}  ({})", out.name, out.library_path.display())
        }
    }
}

/// Copy a project skill into `{data_dir}/skills/<name>/`.
///
/// Never prompts (TTY or not). Name clash without `--force` / `--as` is an error.
pub fn capture(input: &Path, opts: &CaptureOpts, paths: &Paths) -> Result<CaptureOutcome> {
    let project = resolve_project(opts.project.clone())?;
    let source_path = resolve_source(input, &project, paths)?;
    if !source_path_is_dir(&source_path) {
        return Err(SklError::LocalState(format!(
            "capture path is not a skill directory: {}",
            source_path.display()
        )));
    }
    if !source_path.join("SKILL.md").is_file() {
        return Err(SklError::LocalState(format!(
            "skill directory {} is missing SKILL.md",
            source_path.display()
        )));
    }

    let name = match opts.as_name.as_deref() {
        Some(as_name) => {
            linker::validate_skill_name(as_name)?;
            as_name.to_string()
        }
        None => skill_name_from_path(&source_path)?,
    };
    linker::validate_skill_name(&name)?;

    fs::create_dir_all(paths.library_dir())?;
    let library_path = paths.library_skill(&name);

    if is_symlink(&source_path) && points_at(&source_path, &library_path) {
        if library_path.is_dir() && library_path.join("SKILL.md").is_file() {
            index_library(&name, &library_path, paths)?;
            record_portable_manifest(&project, &name, linker::LINK_MODE)?;
            return Ok(CaptureOutcome {
                name,
                source_path,
                library_path,
                action: CaptureAction::Noop,
            });
        }
        return Err(SklError::LocalState(format!(
            "project skill {} already links at {} but that library skill is missing",
            source_path.display(),
            library_path.display()
        )));
    }

    let existed = path_exists(&library_path);
    if existed && !opts.force {
        return Err(SklError::LocalState(format!(
            "skill `{name}` already exists in the personal library at {}; pass --force to overwrite or --as <name>",
            library_path.display()
        )));
    }

    linker::copy_skill_tree(&source_path, &library_path)?;
    if !library_path.join("SKILL.md").is_file() {
        return Err(SklError::LocalState(format!(
            "library copy at {} is missing SKILL.md",
            library_path.display()
        )));
    }

    if !opts.keep_copy {
        replace_with_library_symlink(&source_path, &library_path)?;
    }

    index_library(&name, &library_path, paths)?;
    record_portable_manifest(&project, &name, if opts.keep_copy {
        linker::COPY_MODE
    } else {
        linker::LINK_MODE
    })?;

    let action = if opts.keep_copy {
        CaptureAction::KeepCopy
    } else if existed {
        CaptureAction::Forced
    } else {
        CaptureAction::Captured
    };
    Ok(CaptureOutcome {
        name,
        source_path,
        library_path,
        action,
    })
}

/// Keep `skills.toml` names-only after capture (never write a host path).
fn record_portable_manifest(project: &Path, name: &str, mode: &str) -> Result<()> {
    let mut manifest = linker::load_manifest(project)?;
    let entry = linker::ActivatedSkill::portable(name, mode);
    if let Some(existing) = manifest.skills.iter_mut().find(|skill| skill.name == name) {
        *existing = entry;
    } else {
        manifest.skills.push(entry);
    }
    linker::save_manifest(project, &manifest)
}

fn resolve_source(input: &Path, project: &Path, paths: &Paths) -> Result<PathBuf> {
    if input.as_os_str().is_empty() {
        return Err(SklError::LocalState("capture path is empty".into()));
    }
    if input.exists() {
        return absolute(input);
    }
    let under_project = project.join(input);
    if under_project.exists() {
        return absolute(&under_project);
    }

    let name = input
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|_| input.components().count() == 1)
        .ok_or_else(|| {
            SklError::LocalState(format!(
                "skill not found: {} (not a path; pass a skill name or a directory with SKILL.md)",
                input.display()
            ))
        })?;
    linker::validate_skill_name(name)?;

    let sticky = resolve_activation_extras(Some(paths), &[])?;
    let home = config::home_dir().unwrap_or_else(|_| project.to_path_buf());
    let mut targets = linker::load_manifest(project)?.targets;
    targets.extra = linker::merge_extra_ids(&[&targets.extra, &sticky]);
    let dests = linker::destinations_for(project, &home, &targets);
    for dest in dests {
        let candidate = dest.path.join(name);
        if candidate.exists() {
            return absolute(&candidate);
        }
    }

    Err(SklError::LocalState(format!(
        "skill `{name}` not found under {} or extra project dests",
        project.join(".agents").join("skills").display()
    )))
}

fn skill_name_from_path(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SklError::LocalState(format!("invalid skill path {}", path.display())))?;
    linker::validate_skill_name(name)?;
    Ok(name.to_string())
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

fn replace_with_library_symlink(source: &Path, library: &Path) -> Result<()> {
    remove_path(source)?;
    linker::ensure_link(library, source, false)?;
    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(SklError::from(err)),
    };
    if meta.file_type().is_symlink() || meta.file_type().is_file() {
        fs::remove_file(path)?;
    } else if meta.file_type().is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn source_path_is_dir(path: &Path) -> bool {
    path.is_dir()
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn points_at(link: &Path, target: &Path) -> bool {
    let Ok(current) = fs::read_link(link) else {
        return false;
    };
    if current == target {
        return true;
    }
    let resolved = if current.is_absolute() {
        fs::canonicalize(current).ok()
    } else {
        link.parent()
            .and_then(|parent| fs::canonicalize(parent.join(current)).ok())
    };
    match (resolved, fs::canonicalize(target).ok()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto_sync::{maybe_run, AutoSyncResult};

    fn isolated_paths(tmp: &Path) -> Paths {
        let data_dir = tmp.join("data");
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            db_file: data_dir.join("state.db"),
            data_dir,
        }
    }

    fn plant_skill(dir: &Path, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    fn project_skill(tmp: &Path, name: &str, body: &str) -> (PathBuf, PathBuf) {
        let project = tmp.join("proj");
        let skill = project.join(".agents").join("skills").join(name);
        plant_skill(&skill, body);
        (project, skill)
    }

    fn opts_for(project: &Path) -> CaptureOpts {
        CaptureOpts {
            project: Some(project.to_path_buf()),
            ..CaptureOpts::default()
        }
    }

    #[test]
    fn capture_library_is_data_dir_skills_not_home_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        assert_eq!(paths.library_dir(), tmp.path().join("data/skills"));
        assert_eq!(
            paths.library_skill("demo"),
            tmp.path().join("data/skills/demo")
        );
        let library_dir = paths.library_dir();
        let rendered = library_dir.to_string_lossy();
        assert!(
            !rendered.contains(".agents/skills"),
            "library must be under data_dir, not ~/.agents/skills: {rendered}"
        );
    }

    #[test]
    fn default_copies_into_library_and_replaces_project_with_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let (project, skill) = project_skill(tmp.path(), "greeter", "# hello\n");

        let out = capture(&skill, &opts_for(&project), &paths).unwrap();
        assert_eq!(out.name, "greeter");
        assert_eq!(out.action, CaptureAction::Captured);
        assert_eq!(out.library_path, paths.library_skill("greeter"));
        assert_eq!(
            fs::read_to_string(out.library_path.join("SKILL.md")).unwrap(),
            "# hello\n"
        );
        assert!(is_symlink(&skill));
        assert!(points_at(&skill, &out.library_path));
        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            "# hello\n"
        );

        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "greeter");
        assert_eq!(listed[0].source, LIBRARY_SOURCE);
        assert_eq!(listed[0].path, out.library_path);

        let raw = fs::read_to_string(linker::manifest_path(&project)).unwrap();
        assert!(raw.contains("greeter"), "{raw}");
        assert!(!raw.contains("path ="), "{raw}");
        assert!(raw.contains("library"), "{raw}");
    }

    #[test]
    fn symlink_already_pointing_at_library_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let (project, skill) = project_skill(tmp.path(), "greeter", "# hello\n");
        capture(&skill, &opts_for(&project), &paths).unwrap();
        assert!(is_symlink(&skill));

        let again = capture(&skill, &opts_for(&project), &paths).unwrap();
        assert_eq!(again.action, CaptureAction::Noop);
        assert!(is_symlink(&skill));
        assert!(points_at(&skill, &paths.library_skill("greeter")));
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# hello\n"
        );
    }

    #[test]
    fn clash_without_force_or_as_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        plant_skill(&paths.library_skill("greeter"), "# library\n");
        let (project, skill) = project_skill(tmp.path(), "greeter", "# project\n");

        let err = capture(&skill, &opts_for(&project), &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "{err}");
        assert!(err.contains("--force"), "{err}");
        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            "# project\n"
        );
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# library\n"
        );
        assert!(!is_symlink(&skill));
    }

    #[test]
    fn force_overwrites_library_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        plant_skill(&paths.library_skill("greeter"), "# old\n");
        let (project, skill) = project_skill(tmp.path(), "greeter", "# new\n");

        let mut opts = opts_for(&project);
        opts.force = true;
        let out = capture(&skill, &opts, &paths).unwrap();
        assert_eq!(out.action, CaptureAction::Forced);
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# new\n"
        );
        assert!(is_symlink(&skill));
        assert!(points_at(&skill, &paths.library_skill("greeter")));
    }

    #[test]
    fn as_renames_library_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        plant_skill(&paths.library_skill("greeter"), "# clash\n");
        let (project, skill) = project_skill(tmp.path(), "greeter", "# notes\n");

        let mut opts = opts_for(&project);
        opts.as_name = Some("notes".into());
        let out = capture(&skill, &opts, &paths).unwrap();
        assert_eq!(out.name, "notes");
        assert_eq!(out.action, CaptureAction::Captured);
        assert_eq!(out.library_path, paths.library_skill("notes"));
        assert_eq!(
            fs::read_to_string(paths.library_skill("notes").join("SKILL.md")).unwrap(),
            "# notes\n"
        );
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# clash\n"
        );
        assert!(is_symlink(&skill));
        assert!(points_at(&skill, &paths.library_skill("notes")));
    }

    #[test]
    fn keep_copy_leaves_project_as_real_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let (project, skill) = project_skill(tmp.path(), "greeter", "# hello\n");

        let mut opts = opts_for(&project);
        opts.keep_copy = true;
        let out = capture(&skill, &opts, &paths).unwrap();
        assert_eq!(out.action, CaptureAction::KeepCopy);
        assert!(!is_symlink(&skill));
        assert!(skill.is_dir());
        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            "# hello\n"
        );
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# hello\n"
        );
    }

    #[test]
    fn missing_skill_md_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let project = tmp.path().join("proj");
        let skill = project.join(".agents/skills/greeter");
        fs::create_dir_all(&skill).unwrap();

        let err = capture(&skill, &opts_for(&project), &paths)
            .unwrap_err()
            .to_string();
        assert!(err.contains("SKILL.md"), "{err}");
        assert!(!paths.library_skill("greeter").exists());
    }

    #[test]
    fn rejects_unsafe_as_name() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let (project, skill) = project_skill(tmp.path(), "greeter", "# hello\n");
        let mut opts = opts_for(&project);
        opts.as_name = Some("../etc".into());
        let err = capture(&skill, &opts, &paths).unwrap_err().to_string();
        assert!(err.contains("invalid skill name"), "{err}");
    }

    #[test]
    fn resolves_skill_name_under_project_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let (project, _) = project_skill(tmp.path(), "greeter", "# hello\n");
        let out = capture(Path::new("greeter"), &opts_for(&project), &paths).unwrap();
        assert_eq!(out.name, "greeter");
        assert_eq!(out.action, CaptureAction::Captured);
        assert!(paths.library_skill("greeter").join("SKILL.md").is_file());
    }

    #[test]
    fn resolves_name_under_sticky_extra_dest() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        crate::config::add_sticky_extras(&paths, &["claude".into()]).unwrap();
        let project = tmp.path().join("proj");
        let skill = project.join(".claude/skills/greeter");
        plant_skill(&skill, "# extra\n");

        let out = capture(Path::new("greeter"), &opts_for(&project), &paths).unwrap();
        assert_eq!(out.action, CaptureAction::Captured);
        assert!(is_symlink(&skill));
        assert!(points_at(&skill, &paths.library_skill("greeter")));
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# extra\n"
        );
    }

    #[test]
    fn upsert_does_not_wipe_other_indexed_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let other = tmp.path().join("other/alpha");
        plant_skill(&other, "# alpha\n");
        let tree = skills::hash_skill_dir(&other).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.upsert_skill(&DiscoveredSkill {
            name: "alpha".into(),
            source: "claude".into(),
            path: other,
            tree,
        })
        .unwrap();

        let (project, skill) = project_skill(tmp.path(), "greeter", "# hello\n");
        capture(&skill, &opts_for(&project), &paths).unwrap();

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

    #[tokio::test]
    async fn fail_soft_dead_api_does_not_fail_capture() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let (project, skill) = project_skill(tmp.path(), "greeter", "# hello\n");

        run_with(
            skill.clone(),
            opts_for(&project),
            &paths,
            "http://127.0.0.1:1",
        )
        .await
        .unwrap();

        assert!(paths.library_skill("greeter").join("SKILL.md").is_file());
        let sync = maybe_run("http://127.0.0.1:1", &paths, "capture").await;
        assert!(
            matches!(
                sync,
                AutoSyncResult::Skipped { .. } | AutoSyncResult::FailedSoft { .. }
            ),
            "{sync:?}"
        );
    }
}
