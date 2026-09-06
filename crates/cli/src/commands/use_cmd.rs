//! `skl use` — materialize (copy) a library skill into the project's agent dirs.

use std::path::{Path, PathBuf};

use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;
use crate::local::library;
use crate::local::linker::{self, ActivateOpts, LinkAction, ProjectionMode};
use crate::local::skills::{self, DiscoveredSkill};

pub struct UseOpts {
    pub all: bool,
    pub link: bool,
    pub force: bool,
}

pub async fn run(
    names: &[String],
    project: Option<PathBuf>,
    agents: &[String],
    opts: UseOpts,
    api_base: &str,
) -> Result<()> {
    let all = opts.all;
    let project = resolve_project(project)?;
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let db_file = paths.as_ref().map(|p| p.db_file.as_path());
    let extras = resolve_activation_extras(paths.as_ref(), agents)?;
    let mode = resolve_projection_mode(paths.as_ref(), opts.link)?;

    if all && !names.is_empty() {
        return Err(SklError::LocalState(
            "use either `skl use --all` or `skl use <skill>`".into(),
        ));
    }

    if all {
        let outs = restore_all(&project, &home, db_file, &extras, mode, opts.force)?;
        if outs.is_empty() {
            eprintln!(
                "(no skills listed in {}; nothing to restore)",
                linker::manifest_path(&project).display()
            );
        } else {
            for out in &outs {
                eprintln!(
                    "using {}  ({}  {})",
                    out.skill,
                    out.source,
                    out.source_path.display()
                );
                for link in &out.links {
                    eprintln!(
                        "  {:<8} {:<8} {}",
                        action_label(link.action),
                        link.agent,
                        link.path.display()
                    );
                }
            }
            eprintln!("  updated  {}", linker::manifest_path(&project).display());
        }
        if let Some(paths) = paths.as_ref() {
            let _ = crate::auto_sync::maybe_run(api_base, paths, "use").await;
        }
        return Ok(());
    }

    if names.is_empty() {
        list_activated(&project)?;
        if let Some(paths) = paths.as_ref() {
            let _ = crate::auto_sync::maybe_run(api_base, paths, "use").await;
        }
        return Ok(());
    }

    for name in names {
        let skill = resolve_skill(name, &home, db_file)?;
        let out = linker::activate_with(
            &project,
            &home,
            &skill,
            ActivateOpts {
                extras: &extras,
                mode,
                force: opts.force,
            },
        )?;
        eprintln!(
            "using {}  ({}  {})",
            out.skill,
            out.source,
            out.source_path.display()
        );
        for link in &out.links {
            eprintln!(
                "  {:<8} {:<8} {}",
                action_label(link.action),
                link.agent,
                link.path.display()
            );
        }
        eprintln!("  updated  {}", out.manifest.display());
    }
    if let Some(paths) = paths.as_ref() {
        let _ = crate::auto_sync::maybe_run(api_base, paths, "use").await;
    }
    Ok(())
}

/// Rematerialize every skill listed in `skills.toml` from this machine's library.
/// Does not run on sync.
pub fn restore_all(
    project: &Path,
    home: &Path,
    db_file: Option<&Path>,
    extras: &[String],
    mode: ProjectionMode,
    force: bool,
) -> Result<Vec<linker::ActivateOutcome>> {
    let manifest = linker::load_manifest(project)?;
    if manifest.skills.is_empty() {
        return Ok(Vec::new());
    }

    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    for entry in &manifest.skills {
        match resolve_skill(&entry.name, home, db_file) {
            Ok(skill) => resolved.push(skill),
            Err(_) => missing.push(entry.name.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(missing_listed_skills_error(project, &missing));
    }

    let mut outs = Vec::new();
    for skill in &resolved {
        outs.push(linker::activate_with(
            project,
            home,
            skill,
            ActivateOpts {
                extras,
                mode,
                force,
            },
        )?);
    }
    Ok(outs)
}

/// CLI `--link` wins; else `project.projection` in config; else copy.
/// Unknown config values error instead of silently becoming copy.
pub fn resolve_projection_mode(paths: Option<&Paths>, link: bool) -> Result<ProjectionMode> {
    if link {
        return Ok(ProjectionMode::Link);
    }
    let Some(paths) = paths else {
        return Ok(ProjectionMode::Copy);
    };
    if !paths.config_file.exists() {
        return Ok(ProjectionMode::Copy);
    }
    crate::config::load(paths)?.projection_mode()
}

fn missing_listed_skills_error(project: &Path, missing: &[String]) -> SklError {
    let names = missing
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let noun = if missing.len() == 1 {
        "skill"
    } else {
        "skills"
    };
    let pronoun = if missing.len() == 1 { "it" } else { "them" };
    SklError::LocalState(format!(
        "{noun} {names} listed in {} but not found in the personal library on this machine. Run `skl sync` to pull {pronoun}, then `skl use --all`.",
        linker::manifest_path(project).display()
    ))
}

/// Sticky extras ∪ `-a/--agent` extras for this activation.
pub fn resolve_activation_extras(
    paths: Option<&Paths>,
    cli_agents: &[String],
) -> Result<Vec<String>> {
    let cli = linker::normalize_extra_ids(cli_agents)?;
    let sticky = paths
        .and_then(|p| crate::config::load(p).ok())
        .map(|cfg| cfg.sticky_extras())
        .unwrap_or_default();
    Ok(linker::merge_extra_ids(&[&sticky, &cli]))
}

fn list_activated(project: &Path) -> Result<()> {
    let manifest = linker::load_manifest(project)?;
    if manifest.skills.is_empty() {
        println!(
            "(no skills activated in {}; run `skl use <skill>`)",
            linker::manifest_path(project).display()
        );
        return Ok(());
    }
    println!("{:<24} {:<10} {}", "name", "source", "mode");
    for skill in &manifest.skills {
        println!(
            "{:<24} {:<10} {}",
            skill.name,
            skill.source.as_deref().unwrap_or(linker::PORTABLE_SOURCE),
            skill.mode
        );
    }
    Ok(())
}

pub fn resolve_skill(name: &str, _home: &Path, db_file: Option<&Path>) -> Result<DiscoveredSkill> {
    linker::validate_skill_name(name)?;

    if let Some(db_file) = db_file {
        if let Some(data_dir) = db_file.parent() {
            let library_dir = data_dir.join("skills");
            let lib = library_dir.join(name);
            if lib.is_dir() && lib.join("SKILL.md").is_file() {
                let tree = skills::hash_skill_dir(&lib)?;
                return Ok(DiscoveredSkill {
                    name: name.to_string(),
                    source: library::LIBRARY_SOURCE.into(),
                    path: lib,
                    tree,
                });
            }
        }
    }

    if let Some(db_file) = db_file {
        if db_file.exists() {
            if let Some(data_dir) = db_file.parent() {
                let library_dir = data_dir.join("skills");
                let db = LocalDb::open(db_file)?;
                if let Some(skill) = db.find_skill(name)? {
                    if library::is_under_library(&skill.path, &library_dir) && skill.path.is_dir() {
                        return Ok(skill);
                    }
                }
            }
        }
    }

    Err(SklError::LocalState(format!(
        "skill `{name}` is not in the personal library on this machine. Run `skl init` or `skl capture` to import it, or `skl sync` to pull the library, then `skl use --all`."
    )))
}

pub fn resolve_project(flag: Option<PathBuf>) -> Result<PathBuf> {
    let raw = match flag {
        Some(path) => path,
        None => std::env::current_dir()?,
    };
    if !raw.exists() {
        return Err(SklError::LocalState(format!(
            "project directory does not exist: {}",
            raw.display()
        )));
    }
    fs_canonicalize(&raw)
}

fn fs_canonicalize(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path)
        .map_err(|err| SklError::LocalState(format!("cannot resolve {}: {err}", path.display())))
}

fn action_label(action: LinkAction) -> &'static str {
    match action {
        LinkAction::Created => "link",
        LinkAction::Copied => "copy",
        LinkAction::Replaced => "replace",
        LinkAction::CopyReplaced => "copy*",
        LinkAction::Unchanged => "ok",
        LinkAction::Removed => "removed",
        LinkAction::Absent => "absent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::skills::hash_skill_dir;

    #[test]
    fn home_agent_dir_is_not_a_use_source() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();

        let err = resolve_skill("greeter", &home, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("personal library"), "{err}");
        assert!(!err.contains("syncing"), "{err}");
    }

    #[test]
    fn home_agents_skills_is_not_a_use_source() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".agents/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();

        let err = resolve_skill("greeter", &home, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("personal library"), "{err}");
    }

    #[test]
    fn harness_indexed_path_is_not_resolved() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let skill_dir = tmp.path().join("elsewhere/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();
        let tree = hash_skill_dir(&skill_dir).unwrap();
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_file = data_dir.join("state.db");
        let db = LocalDb::open(&db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "cursor".into(),
            path: skill_dir.clone(),
            tree,
        }])
        .unwrap();

        let err = resolve_skill("greeter", &home, Some(&db_file))
            .unwrap_err()
            .to_string();
        assert!(err.contains("personal library"), "{err}");
    }

    #[test]
    fn resolves_from_personal_library_under_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let data_dir = tmp.path().join("data");
        let skill_dir = data_dir.join("skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "lib").unwrap();
        let db_file = data_dir.join("state.db");

        let found = resolve_skill("greeter", &home, Some(&db_file)).unwrap();
        assert_eq!(found.source, "agents");
        assert_eq!(found.path, skill_dir);
    }

    #[test]
    fn missing_skill_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_skill("nope", tmp.path(), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("personal library"), "{err}");
    }

    #[test]
    fn activation_extras_merge_sticky_and_cli() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        crate::config::add_sticky_extras(&paths, &["claude-code".into()]).unwrap();
        let extras = resolve_activation_extras(Some(&paths), &["windsurf".into()]).unwrap();
        assert_eq!(extras, ["claude-code", "windsurf"]);
        assert!(resolve_activation_extras(None, &[]).unwrap().is_empty());
        assert!(resolve_activation_extras(None, &["agents".into()]).is_err());
        assert!(resolve_activation_extras(None, &["cursor".into()]).is_err());
    }

    #[test]
    fn sticky_cursor_is_migrated_away() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        paths.ensure().unwrap();
        std::fs::write(
            &paths.config_file,
            "[targets]\nextra = [\"cursor\", \"claude\"]\n",
        )
        .unwrap();
        let extras = resolve_activation_extras(Some(&paths), &[]).unwrap();
        assert_eq!(extras, ["claude-code"]);
        assert!(!extras.iter().any(|id| id == "cursor"));
    }

    #[test]
    fn restore_all_resolves_by_name_and_rewrites_portable() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let data_dir = tmp.path().join("data");
        let skill_dir = data_dir.join("skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();
        let db_file = data_dir.join("state.db");
        std::fs::write(
            linker::manifest_path(&project),
            r#"
[[skills]]
name = "greeter"
source = "claude"
path = "/Users/other/.claude/skills/greeter"
mode = "symlink"
"#,
        )
        .unwrap();

        let outs = restore_all(
            &project,
            &home,
            Some(&db_file),
            &[],
            ProjectionMode::Copy,
            false,
        )
        .unwrap();
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].skill, "greeter");
        assert!(project.join(".agents/skills/greeter").exists());
        assert!(!project
            .join(".agents/skills/greeter")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        let raw = std::fs::read_to_string(linker::manifest_path(&project)).unwrap();
        assert!(!raw.contains("path ="), "{raw}");
        assert!(!raw.contains("/Users/other"), "{raw}");
        assert!(raw.contains("library"), "{raw}");
    }

    #[test]
    fn restore_all_after_library_mutation_uses_new_content() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let data_dir = tmp.path().join("data");
        let skill_dir = data_dir.join("skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "original\n").unwrap();
        let db_file = data_dir.join("state.db");

        let skill = resolve_skill("greeter", &home, Some(&db_file)).unwrap();
        linker::activate(&project, &home, &skill).unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join(".agents/skills/greeter/SKILL.md")).unwrap(),
            "original\n"
        );

        std::fs::write(skill_dir.join("SKILL.md"), "mutated library\n").unwrap();
        let outs = restore_all(
            &project,
            &home,
            Some(&db_file),
            &[],
            ProjectionMode::Copy,
            false,
        )
        .unwrap();
        assert_eq!(outs.len(), 1);
        let dest = project.join(".agents/skills/greeter");
        assert!(dest.exists());
        assert_eq!(
            std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "mutated library\n"
        );
        assert!(dest.is_dir());
        assert!(!dest.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[test]
    fn restore_all_missing_skill_suggests_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            linker::manifest_path(&project),
            r#"
[[skills]]
name = "ghost"
mode = "symlink"
"#,
        )
        .unwrap();

        let err = restore_all(&project, &home, None, &[], ProjectionMode::Copy, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("ghost"), "{err}");
        assert!(err.contains("skl sync"), "{err}");
        assert!(err.contains("skl use --all"), "{err}");
        assert!(!project.join(".agents").exists());
    }

    /// Same committed names-only `skills.toml` on two HOMEs: B restores from B's library.
    #[test]
    fn same_portable_manifest_restores_on_second_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home_a = tmp.path().join("home-a");
        let home_b = tmp.path().join("home-b");
        let project_a = tmp.path().join("proj-a");
        let project_b = tmp.path().join("proj-b");
        std::fs::create_dir_all(&project_a).unwrap();
        std::fs::create_dir_all(&project_b).unwrap();

        let data_a = tmp.path().join("data-a");
        let skill_a = data_a.join("skills/greeter");
        std::fs::create_dir_all(&skill_a).unwrap();
        std::fs::write(skill_a.join("SKILL.md"), "from A").unwrap();
        let db_a = data_a.join("state.db");
        let discovered_a = resolve_skill("greeter", &home_a, Some(&db_a)).unwrap();
        linker::activate(&project_a, &home_a, &discovered_a).unwrap();

        let raw_a = std::fs::read_to_string(linker::manifest_path(&project_a)).unwrap();
        assert!(!raw_a.contains("path ="), "{raw_a}");
        assert!(!raw_a.contains("$HOME"), "{raw_a}");
        assert!(
            !raw_a.contains(home_a.to_string_lossy().as_ref()),
            "committed manifest must not embed HOME A: {raw_a}"
        );
        std::fs::copy(
            linker::manifest_path(&project_a),
            linker::manifest_path(&project_b),
        )
        .unwrap();

        let data_b = tmp.path().join("data-b");
        let skill_b = data_b.join("skills/greeter");
        std::fs::create_dir_all(&skill_b).unwrap();
        std::fs::write(skill_b.join("SKILL.md"), "from B after sync").unwrap();
        let db_b = data_b.join("state.db");

        let outs = restore_all(
            &project_b,
            &home_b,
            Some(&db_b),
            &[],
            ProjectionMode::Copy,
            false,
        )
        .unwrap();
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].skill, "greeter");
        let dest = project_b.join(".agents/skills/greeter");
        assert!(dest.exists());
        assert_eq!(
            std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "from B after sync"
        );
        let raw_b = std::fs::read_to_string(linker::manifest_path(&project_b)).unwrap();
        assert!(!raw_b.contains("path ="), "{raw_b}");
        assert!(!raw_b.contains("$HOME"), "{raw_b}");
        assert!(
            !raw_b.contains(home_a.to_string_lossy().as_ref()),
            "{raw_b}"
        );
        assert!(
            !raw_b.contains(home_b.to_string_lossy().as_ref()),
            "{raw_b}"
        );
        assert_eq!(
            std::fs::read_to_string(project_a.join(".agents/skills/greeter/SKILL.md")).unwrap(),
            "from A"
        );
    }

    /// Legacy absolute `path` is ignored even when that directory still exists.
    #[test]
    fn restore_all_ignores_legacy_path_when_it_still_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let home_a = tmp.path().join("home-a");
        let home_b = tmp.path().join("home-b");
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();

        let foreign = home_a.join(".claude/skills/greeter");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("SKILL.md"), "foreign machine").unwrap();

        let data_b = tmp.path().join("data-b");
        let local = data_b.join("skills/greeter");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("SKILL.md"), "this machine").unwrap();
        let db_b = data_b.join("state.db");

        std::fs::write(
            linker::manifest_path(&project),
            format!(
                r#"
[[skills]]
name = "greeter"
source = "claude"
path = "{}"
mode = "symlink"
"#,
                foreign.display()
            ),
        )
        .unwrap();

        let outs = restore_all(
            &project,
            &home_b,
            Some(&db_b),
            &[],
            ProjectionMode::Copy,
            false,
        )
        .unwrap();
        assert_eq!(outs.len(), 1);
        assert_eq!(
            std::fs::read_to_string(project.join(".agents/skills/greeter/SKILL.md")).unwrap(),
            "this machine"
        );
        let raw = std::fs::read_to_string(linker::manifest_path(&project)).unwrap();
        assert!(!raw.contains("path ="), "{raw}");
        assert!(
            !raw.contains(&foreign.to_string_lossy().to_string()),
            "{raw}"
        );
    }

    #[test]
    fn resolve_projection_prefers_link_flag_then_config() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        paths.ensure().unwrap();
        assert_eq!(
            resolve_projection_mode(None, false).unwrap(),
            ProjectionMode::Copy
        );
        assert_eq!(
            resolve_projection_mode(None, true).unwrap(),
            ProjectionMode::Link
        );
        std::fs::write(&paths.config_file, "[project]\nprojection = \"link\"\n").unwrap();
        assert_eq!(
            resolve_projection_mode(Some(&paths), false).unwrap(),
            ProjectionMode::Link
        );
        assert_eq!(
            resolve_projection_mode(Some(&paths), true).unwrap(),
            ProjectionMode::Link
        );
        std::fs::write(&paths.config_file, "[project]\nprojection = \"copy\"\n").unwrap();
        assert_eq!(
            resolve_projection_mode(Some(&paths), false).unwrap(),
            ProjectionMode::Copy
        );
        std::fs::write(&paths.config_file, "[project]\nprojection = \"coppy\"\n").unwrap();
        let err = resolve_projection_mode(Some(&paths), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown [project].projection"), "{err}");
        assert!(err.contains("coppy"), "{err}");
        assert_eq!(
            resolve_projection_mode(Some(&paths), true).unwrap(),
            ProjectionMode::Link
        );
    }

    #[test]
    fn committed_repo_skills_toml_has_no_home_or_absolute_paths() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let raw = std::fs::read_to_string(repo.join("skills.toml")).expect("repo skills.toml");
        assert!(!raw.contains("$HOME"), "{raw}");
        assert!(!raw.contains("path ="), "{raw}");
        assert!(!raw.contains("/Users/"), "{raw}");
        assert!(!raw.contains("/home/"), "{raw}");
        let loaded = linker::load_manifest(&repo).unwrap();
        for skill in &loaded.skills {
            assert!(
                !skill.has_absolute_path(),
                "{} still lists an absolute path",
                skill.name
            );
            assert!(
                skill.path.is_none(),
                "{} must not serialize a path field",
                skill.name
            );
        }
    }
}
