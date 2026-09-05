//! `skl migrate targets` — explicit M0 → canonical `.agents/skills`.
//!
//! Doctor warns; `skl use` does not call this. Destination paths come from
//! furnace `project_link_targets` / `ensure_link` / `remove_managed_link`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::use_cmd::{resolve_project, resolve_skill};
use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::local::linker::{
    self, is_m0_layout, load_manifest, manifest_path, project_link_targets, save_manifest,
    ActivatedSkill, LinkAction, LinkChange, LinkTargetKind, ManifestTargets,
};
use crate::local::skills::DiscoveredSkill;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateOutcome {
    pub project: PathBuf,
    pub manifest: PathBuf,
    pub m0: bool,
    pub prune_old: bool,
    pub skills: Vec<String>,
    pub extras: Vec<String>,
    pub links: Vec<LinkChange>,
    pub already_canonical: bool,
}

pub fn run(project: Option<PathBuf>, prune_old: bool) -> Result<()> {
    let project = resolve_project(project)?;
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let db_file = paths.as_ref().map(|p| p.db_file.as_path());
    let out = migrate_targets(&project, &home, db_file, prune_old)?;
    print_outcome(&out);
    Ok(())
}

pub fn migrate_targets(
    project: &Path,
    home: &Path,
    db_file: Option<&Path>,
    prune_old: bool,
) -> Result<MigrateOutcome> {
    let mut manifest = load_manifest(project)?;
    let m0 = is_m0_layout(project);
    let names = collect_skill_names(project, home, &manifest);
    if names.is_empty() {
        return Err(SklError::LocalState(format!(
            "no project skills to migrate under {} (expected M0 links in .claude/.cursor)",
            project.display()
        )));
    }

    let extras = if prune_old {
        Vec::new()
    } else {
        detected_extra_ids(project, home)
    };

    let canonical = project_link_targets(project, home)
        .into_iter()
        .find(|target| target.kind == LinkTargetKind::Canonical)
        .ok_or_else(|| SklError::LocalState("linker has no canonical dest".into()))?;

    let mut links = Vec::new();
    let mut resolved = Vec::new();
    for name in &names {
        let skill = resolve_migrating_skill(name, project, home, db_file, &manifest)?;
        let dest = canonical.path.join(&skill.name);
        let prior_copy = manifest
            .skills
            .iter()
            .any(|s| s.name == skill.name && s.mode == linker::COPY_MODE);
        let source = fs::canonicalize(&skill.path).unwrap_or_else(|_| skill.path.clone());
        let placed = linker::ensure_link(&source, &dest, prior_copy)?;
        links.push(LinkChange {
            agent: canonical.id.clone(),
            path: dest,
            action: placed.action,
        });
        upsert_skill(&mut manifest, &skill, &source, placed.mode);
        resolved.push((skill.name, prior_copy));
    }

    manifest.targets = ManifestTargets {
        canonical: vec![canonical.id.clone()],
        extra: extras.clone(),
    };
    save_manifest(project, &manifest)?;

    if prune_old {
        for (name, prior_copy) in &resolved {
            for target in project_link_targets(project, home) {
                if target.kind == LinkTargetKind::Canonical {
                    continue;
                }
                let dest = target.path.join(name);
                let action = linker::remove_managed_link(&dest, *prior_copy)?;
                links.push(LinkChange {
                    agent: target.id,
                    path: dest,
                    action,
                });
            }
        }
    }

    let already_canonical = !m0
        && links
            .iter()
            .filter(|l| l.agent == canonical.id)
            .all(|l| l.action == LinkAction::Unchanged);

    Ok(MigrateOutcome {
        project: project.to_path_buf(),
        manifest: manifest_path(project),
        m0,
        prune_old,
        skills: names,
        extras,
        links,
        already_canonical,
    })
}

fn collect_skill_names(
    project: &Path,
    home: &Path,
    manifest: &linker::SkillsManifest,
) -> Vec<String> {
    let mut names: Vec<String> = manifest.skills.iter().map(|s| s.name.clone()).collect();
    for target in project_link_targets(project, home) {
        names.extend(skill_names_in(&target.path));
    }
    names.sort();
    names.dedup();
    names
}

fn detected_extra_ids(project: &Path, home: &Path) -> Vec<String> {
    let ids: Vec<String> = project_link_targets(project, home)
        .into_iter()
        .filter(|target| target.kind == LinkTargetKind::OptIn)
        .filter(|target| !skill_names_in(&target.path).is_empty())
        .map(|target| target.id)
        .collect();
    crate::local::linker::filter_extra_ids(&ids)
}

fn skill_names_in(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            let is_skill = path.is_dir()
                || path
                    .symlink_metadata()
                    .map(|meta| meta.file_type().is_symlink())
                    .unwrap_or(false);
            is_skill.then_some(name)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

fn resolve_migrating_skill(
    name: &str,
    project: &Path,
    home: &Path,
    db_file: Option<&Path>,
    _manifest: &linker::SkillsManifest,
) -> Result<DiscoveredSkill> {
    // Ignore legacy absolute `path` — resolve by name from this machine.

    if let Ok(skill) = resolve_skill(name, home, db_file) {
        return Ok(skill);
    }

    for target in project_link_targets(project, home) {
        let dest = target.path.join(name);
        if dest.is_dir() {
            let source = read_link_if_symlink(&dest)
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| dest.clone());
            return Ok(DiscoveredSkill {
                name: name.to_string(),
                source: target.id,
                path: source,
                tree: crate::local::skills::hash_skill_dir(&dest)?,
            });
        }
    }

    Err(SklError::LocalState(format!(
        "skill `{name}` is linked in the project but not found in the home library"
    )))
}

fn upsert_skill(
    manifest: &mut linker::SkillsManifest,
    skill: &DiscoveredSkill,
    _source: &Path,
    mode: &str,
) {
    let entry = ActivatedSkill::portable(&skill.name, mode);
    if let Some(existing) = manifest.skills.iter_mut().find(|s| s.name == skill.name) {
        *existing = entry;
    } else {
        manifest.skills.push(entry);
    }
}

fn read_link_if_symlink(path: &Path) -> Option<PathBuf> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let current = fs::read_link(path).ok()?;
    if current.is_absolute() {
        Some(current)
    } else {
        path.parent().map(|parent| parent.join(current))
    }
}

fn print_outcome(out: &MigrateOutcome) {
    if out.already_canonical && !out.prune_old {
        eprintln!(
            "already using canonical .agents/skills  ({})",
            out.manifest.display()
        );
    } else if out.m0 {
        eprintln!("migrating M0 targets → .agents/skills");
    } else {
        eprintln!("ensuring canonical .agents/skills");
    }
    for link in &out.links {
        eprintln!(
            "  {:<8} {:<8} {}",
            action_label(link.action),
            link.agent,
            link.path.display()
        );
    }
    if out.prune_old {
        eprintln!("  pruned   legacy dests (extras cleared)");
    } else if !out.extras.is_empty() {
        eprintln!(
            "  extras   {} (opt-in; old links kept unless --prune-old)",
            out.extras.join(", ")
        );
    }
    eprintln!("  updated  {}", out.manifest.display());
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
    use crate::api::types::SkillTree;
    use crate::local::linker::{
        self, destinations_for, load_manifest, LinkAction, ManifestTargets, LINK_MODE,
    };
    use crate::local::skills::DiscoveredSkill;
    use std::collections::BTreeMap;
    use std::fs;

    fn demo_skill(home: &Path, name: &str) -> DiscoveredSkill {
        let dir = home.join(".claude").join("skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
        DiscoveredSkill {
            name: name.into(),
            source: "claude".into(),
            path: dir,
            tree: SkillTree {
                tree_hash: "x".into(),
                files: BTreeMap::new(),
            },
        }
    }

    /// Deliberate M0 fixture: skills.toml + .claude/.cursor links, no .agents.
    fn plant_m0(project: &Path, skill: &DiscoveredSkill) {
        for rel in [".claude/skills", ".cursor/skills"] {
            let dest_dir = project.join(rel);
            fs::create_dir_all(&dest_dir).unwrap();
            let dest = dest_dir.join(&skill.name);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&skill.path, &dest).unwrap();
            #[cfg(not(unix))]
            {
                fs::create_dir_all(&dest).unwrap();
                fs::copy(skill.path.join("SKILL.md"), dest.join("SKILL.md")).unwrap();
            }
        }
        linker::save_manifest(
            project,
            &linker::SkillsManifest {
                targets: ManifestTargets::default(),
                skills: vec![linker::ActivatedSkill {
                    name: skill.name.clone(),
                    source: Some(skill.source.clone()),
                    path: Some(skill.path.to_string_lossy().into_owned()),
                    mode: LINK_MODE.to_string(),
                }],
            },
        )
        .unwrap();
    }

    #[test]
    fn migrate_ensures_agents_and_keeps_old_links() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        plant_m0(&project, &skill);
        assert!(is_m0_layout(&project));
        assert!(linker::m0_targets_warning(&project)
            .unwrap()
            .contains("skl migrate targets"));
        assert!(!project.join(".agents/skills/greeter").exists());

        let out = migrate_targets(&project, &home, None, false).unwrap();
        assert!(out.m0);
        assert!(!out.prune_old);
        assert_eq!(out.skills, vec!["greeter"]);
        assert_eq!(out.extras, vec!["claude-code"]);
        assert_eq!(out.links[0].agent, "agents");
        assert_eq!(out.links[0].action, LinkAction::Created);
        assert_eq!(
            out.links[0].path,
            project.join(".agents").join("skills").join("greeter")
        );

        let agents = project.join(".agents/skills/greeter");
        assert!(agents.exists());
        assert_eq!(
            fs::read_to_string(agents.join("SKILL.md")).unwrap(),
            "# greeter\n"
        );
        assert!(project.join(".claude/skills/greeter").exists());
        assert!(project.join(".cursor/skills/greeter").exists());

        let manifest = load_manifest(&project).unwrap();
        assert_eq!(manifest.targets.canonical, ["agents"]);
        assert_eq!(manifest.targets.extra, ["claude-code"]);
        assert!(manifest.skills[0].path.is_none());
        assert_eq!(
            manifest.skills[0].source.as_deref(),
            Some(linker::PORTABLE_SOURCE)
        );
        let raw = fs::read_to_string(linker::manifest_path(&project)).unwrap();
        assert!(!raw.contains("path ="), "{raw}");
        assert!(!is_m0_layout(&project));
        assert!(linker::m0_targets_warning(&project).is_none());
    }

    #[test]
    fn migrate_prune_old_removes_legacy_via_remove_managed_link() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        plant_m0(&project, &skill);

        let out = migrate_targets(&project, &home, None, true).unwrap();
        assert!(out.prune_old);
        assert!(out.extras.is_empty());
        assert!(project.join(".agents/skills/greeter").exists());
        assert!(!project.join(".claude/skills/greeter").exists());
        assert!(!project.join(".cursor/skills/greeter").exists());
        assert!(out
            .links
            .iter()
            .any(|l| l.agent == "claude-code" && l.action == LinkAction::Removed));
        assert!(out
            .links
            .iter()
            .any(|l| l.agent == "cursor" && l.action == LinkAction::Removed));
        assert_eq!(
            load_manifest(&project).unwrap().targets.extra,
            [] as [&str; 0]
        );
    }

    #[test]
    fn migrate_is_idempotent_on_canonical_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        plant_m0(&project, &skill);
        migrate_targets(&project, &home, None, false).unwrap();

        let again = migrate_targets(&project, &home, None, false).unwrap();
        assert!(!again.m0);
        assert!(again.already_canonical);
        assert!(again
            .links
            .iter()
            .filter(|l| l.agent == "agents")
            .all(|l| l.action == LinkAction::Unchanged));
        assert!(project.join(".claude/skills/greeter").exists());
    }

    #[test]
    fn migrate_empty_project_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project).unwrap();
        let err = migrate_targets(&project, &home, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no project skills"), "{err}");
    }

    #[test]
    fn doctor_warning_does_not_mutate_m0_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        plant_m0(&project, &skill);
        let warning = linker::m0_targets_warning(&project).expect("warn");
        assert!(warning.contains("skl migrate targets"), "{warning}");
        assert!(!project.join(".agents").exists());
        assert!(project.join(".claude/skills/greeter").exists());
    }

    #[test]
    fn migrate_resolves_by_name_when_manifest_path_is_foreign() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        plant_m0(&project, &skill);
        fs::write(
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

        let out = migrate_targets(&project, &home, None, false).unwrap();
        assert_eq!(out.skills, vec!["greeter"]);
        assert!(project.join(".agents/skills/greeter").exists());
        let raw = fs::read_to_string(linker::manifest_path(&project)).unwrap();
        assert!(!raw.contains("path ="), "{raw}");
        assert!(!raw.contains("/Users/other"), "{raw}");
    }

    #[test]
    fn destinations_for_after_migrate_keeps_extras_until_prune() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        plant_m0(&project, &skill);
        migrate_targets(&project, &home, None, false).unwrap();
        let ids: Vec<_> =
            destinations_for(&project, &home, &load_manifest(&project).unwrap().targets)
                .into_iter()
                .map(|t| t.id)
                .collect();
        assert_eq!(ids, ["agents", "claude-code"]);

        migrate_targets(&project, &home, None, true).unwrap();
        let pruned: Vec<_> =
            destinations_for(&project, &home, &load_manifest(&project).unwrap().targets)
                .into_iter()
                .map(|t| t.id)
                .collect();
        assert_eq!(pruned, ["agents"]);
    }
}
