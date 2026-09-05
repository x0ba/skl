//! `skl use` — symlink a home skill into the project's agent dirs.

use std::path::{Path, PathBuf};

use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::local::db::LocalDb;
use crate::local::linker::{self, LinkAction};
use crate::local::skills::{self, DiscoveredSkill};

pub async fn run(
    names: &[String],
    project: Option<PathBuf>,
    agents: &[String],
    api_base: &str,
) -> Result<()> {
    let project = resolve_project(project)?;
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let db_file = paths.as_ref().map(|p| p.db_file.as_path());
    let extras = resolve_activation_extras(paths.as_ref(), agents)?;

    if names.is_empty() {
        list_activated(&project)?;
        if let Some(paths) = paths.as_ref() {
            let _ = crate::auto_sync::maybe_run(api_base, paths, "use").await;
        }
        return Ok(());
    }

    for name in names {
        let skill = resolve_skill(name, &home, db_file)?;
        let out = linker::activate_with_extras(&project, &home, &skill, &extras)?;
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
    // Fail-soft: never fail `skl use` because auto-sync failed.
    if let Some(paths) = paths.as_ref() {
        let _ = crate::auto_sync::maybe_run(api_base, paths, "use").await;
    }
    Ok(())
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
    println!("{:<24} {:<8} {:<8} {}", "name", "source", "mode", "path");
    for skill in &manifest.skills {
        println!(
            "{:<24} {:<8} {:<8} {}",
            skill.name, skill.source, skill.mode, skill.path
        );
    }
    Ok(())
}

pub fn resolve_skill(name: &str, home: &Path, db_file: Option<&Path>) -> Result<DiscoveredSkill> {
    linker::validate_skill_name(name)?;

    let discovered = skills::discover_from_home(home)?;
    if let Some(skill) = discovered.into_iter().find(|skill| skill.name == name) {
        return Ok(skill);
    }

    if let Some(db_file) = db_file {
        if db_file.exists() {
            let db = LocalDb::open(db_file)?;
            let mut matches: Vec<_> = db
                .list_skills()?
                .into_iter()
                .filter(|skill| skill.name == name)
                .collect();
            let order = ["claude", "cursor", "codex"];
            matches.sort_by_key(|skill| {
                order
                    .iter()
                    .position(|src| *src == skill.source.as_str())
                    .unwrap_or(99)
            });
            if let Some(skill) = matches.into_iter().next() {
                if skill.path.is_dir() {
                    return Ok(skill);
                }
                return Err(SklError::LocalState(format!(
                    "skill `{name}` is indexed at {} but that directory is missing",
                    skill.path.display()
                )));
            }
        }
    }

    Err(SklError::LocalState(format!(
        "skill `{name}` not found under ~/.claude/skills, ~/.cursor/skills, or ~/.codex/skills"
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
        LinkAction::Created => "symlink",
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
    fn resolves_from_home_before_db() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();

        let found = resolve_skill("greeter", &home, None).unwrap();
        assert_eq!(found.source, "claude");
        assert_eq!(found.name, "greeter");
    }

    #[test]
    fn resolves_from_db_when_home_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let skill_dir = tmp.path().join("elsewhere/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();
        let tree = hash_skill_dir(&skill_dir).unwrap();
        let db_file = tmp.path().join("state.db");
        let db = LocalDb::open(&db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "cursor".into(),
            path: skill_dir.clone(),
            tree,
        }])
        .unwrap();

        let found = resolve_skill("greeter", &home, Some(&db_file)).unwrap();
        assert_eq!(found.source, "cursor");
        assert_eq!(found.path, skill_dir);
    }

    #[test]
    fn missing_skill_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_skill("nope", tmp.path(), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "{err}");
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
        crate::config::add_sticky_extras(&paths, &["cursor".into()]).unwrap();
        let extras = resolve_activation_extras(Some(&paths), &["claude".into()]).unwrap();
        assert_eq!(extras, ["claude", "cursor"]);
        assert!(resolve_activation_extras(None, &[]).unwrap().is_empty());
        assert!(resolve_activation_extras(None, &["agents".into()]).is_err());
    }
}
