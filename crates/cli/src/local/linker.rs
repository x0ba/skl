//! Project multi-agent linker: symlink skills into agent dirs + `skills.toml`.
//!
//! Default (only) mode is **symlink**, not copy. Codex is optional if present.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::SkillRoot;
use crate::error::{Result, SklError};
use crate::local::skills::{is_safe_file_path, DiscoveredSkill};

pub const LINK_MODE: &str = "symlink";
pub const MANIFEST_NAME: &str = "skills.toml";

const MANIFEST_HEADER: &str =
    "# Managed by `skl use` / `skl unuse`. Mode is symlink (not copy).\n\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SkillsManifest {
    #[serde(default)]
    pub skills: Vec<ActivatedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivatedSkill {
    pub name: String,
    pub source: String,
    pub path: String,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    LINK_MODE.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkAction {
    Created,
    Replaced,
    Unchanged,
    Removed,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkChange {
    pub agent: String,
    pub path: PathBuf,
    pub action: LinkAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateOutcome {
    pub skill: String,
    pub source: String,
    pub source_path: PathBuf,
    pub links: Vec<LinkChange>,
    pub manifest: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeactivateOutcome {
    pub skill: String,
    pub links: Vec<LinkChange>,
    pub manifest: PathBuf,
    pub was_listed: bool,
}

pub fn validate_skill_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('.')
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || !is_safe_file_path(trimmed)
    {
        return Err(SklError::LocalState(format!("invalid skill name `{name}`")));
    }
    Ok(())
}

/// Project agent dirs that receive symlinks.
///
/// Claude and Cursor always. Codex only if home `~/.codex/skills` exists
/// or the project already has `.codex`.
pub fn project_link_roots(project: &Path, home: &Path) -> Vec<SkillRoot> {
    let mut roots = vec![
        SkillRoot {
            source: "claude",
            path: project.join(".claude").join("skills"),
        },
        SkillRoot {
            source: "cursor",
            path: project.join(".cursor").join("skills"),
        },
    ];
    let home_codex = home.join(".codex").join("skills");
    let project_codex = project.join(".codex");
    if home_codex.is_dir() || project_codex.exists() {
        roots.push(SkillRoot {
            source: "codex",
            path: project.join(".codex").join("skills"),
        });
    }
    roots
}

pub fn manifest_path(project: &Path) -> PathBuf {
    project.join(MANIFEST_NAME)
}

pub fn load_manifest(project: &Path) -> Result<SkillsManifest> {
    let path = manifest_path(project);
    if !path.exists() {
        return Ok(SkillsManifest::default());
    }
    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(SkillsManifest::default());
    }
    toml::from_str(&raw).map_err(|err| SklError::Config(format!("{MANIFEST_NAME}: {err}")))
}

pub fn save_manifest(project: &Path, manifest: &SkillsManifest) -> Result<()> {
    let mut sorted = manifest.clone();
    sorted.skills.sort_by(|a, b| a.name.cmp(&b.name));
    let body = toml::to_string_pretty(&sorted).map_err(|err| SklError::Config(err.to_string()))?;
    fs::write(manifest_path(project), format!("{MANIFEST_HEADER}{body}"))?;
    Ok(())
}

pub fn activate(project: &Path, home: &Path, skill: &DiscoveredSkill) -> Result<ActivateOutcome> {
    validate_skill_name(&skill.name)?;
    if !skill.path.is_dir() {
        return Err(SklError::LocalState(format!(
            "skill `{}` path is not a directory: {}",
            skill.name,
            skill.path.display()
        )));
    }

    let source = canonicalize_dir(&skill.path)?;
    let mut links = Vec::new();
    for root in project_link_roots(project, home) {
        let dest = root.path.join(&skill.name);
        let action = ensure_symlink(&source, &dest)?;
        links.push(LinkChange {
            agent: root.source.to_string(),
            path: dest,
            action,
        });
    }

    let mut manifest = load_manifest(project)?;
    let entry = ActivatedSkill {
        name: skill.name.clone(),
        source: skill.source.clone(),
        path: source.to_string_lossy().into_owned(),
        mode: LINK_MODE.to_string(),
    };
    if let Some(existing) = manifest.skills.iter_mut().find(|s| s.name == skill.name) {
        *existing = entry;
    } else {
        manifest.skills.push(entry);
    }
    save_manifest(project, &manifest)?;

    Ok(ActivateOutcome {
        skill: skill.name.clone(),
        source: skill.source.clone(),
        source_path: source,
        links,
        manifest: manifest_path(project),
    })
}

pub fn deactivate(project: &Path, home: &Path, name: &str) -> Result<DeactivateOutcome> {
    validate_skill_name(name)?;

    let mut manifest = load_manifest(project)?;
    let was_listed = manifest.skills.iter().any(|s| s.name == name);
    manifest.skills.retain(|s| s.name != name);

    let mut links = Vec::new();
    for root in project_link_roots(project, home) {
        let dest = root.path.join(name);
        let action = remove_managed_link(&dest)?;
        links.push(LinkChange {
            agent: root.source.to_string(),
            path: dest,
            action,
        });
    }

    if !was_listed && links.iter().all(|l| l.action == LinkAction::Absent) {
        return Err(SklError::LocalState(format!(
            "skill `{name}` is not activated in {}",
            manifest_path(project).display()
        )));
    }

    save_manifest(project, &manifest)?;

    Ok(DeactivateOutcome {
        skill: name.to_string(),
        links,
        manifest: manifest_path(project),
        was_listed,
    })
}

fn canonicalize_dir(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path)
        .map_err(|err| SklError::LocalState(format!("cannot resolve {}: {err}", path.display())))
}

fn ensure_symlink(target: &Path, link: &Path) -> Result<LinkAction> {
    match dest_kind(link)? {
        DestKind::Missing => {
            if let Some(parent) = link.parent() {
                fs::create_dir_all(parent)?;
            }
            create_symlink(target, link)?;
            Ok(LinkAction::Created)
        }
        DestKind::Symlink => {
            let current = fs::read_link(link)?;
            if same_path(&current, target, link) {
                Ok(LinkAction::Unchanged)
            } else {
                fs::remove_file(link)?;
                create_symlink(target, link)?;
                Ok(LinkAction::Replaced)
            }
        }
        DestKind::Other => Err(SklError::LocalState(format!(
            "{} exists and is not a symlink; refusing to overwrite (skl use is symlink-only)",
            link.display()
        ))),
    }
}

fn remove_managed_link(link: &Path) -> Result<LinkAction> {
    match dest_kind(link)? {
        DestKind::Missing => Ok(LinkAction::Absent),
        DestKind::Symlink => {
            fs::remove_file(link)?;
            Ok(LinkAction::Removed)
        }
        DestKind::Other => Err(SklError::LocalState(format!(
            "{} exists and is not a symlink; refusing to delete",
            link.display()
        ))),
    }
}

enum DestKind {
    Missing,
    Symlink,
    Other,
}

fn dest_kind(path: &Path) -> Result<DestKind> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(DestKind::Symlink),
        Ok(_) => Ok(DestKind::Other),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(DestKind::Missing),
        Err(err) => Err(SklError::from(err)),
    }
}

fn same_path(current: &Path, target: &Path, link: &Path) -> bool {
    if current == target {
        return true;
    }
    let resolved = if current.is_absolute() {
        fs::canonicalize(current).ok()
    } else {
        link.parent()
            .and_then(|parent| fs::canonicalize(parent.join(current)).ok())
    };
    match resolved {
        Some(path) => path == target,
        None => false,
    }
}

fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)?;
        Ok(())
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)?;
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(SklError::LocalState(
            "symlinks are not supported on this platform".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::SkillTree;
    use std::collections::BTreeMap;

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

    #[test]
    fn use_symlinks_claude_and_cursor_and_writes_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        let out = activate(&project, &home, &skill).unwrap();
        assert_eq!(out.skill, "greeter");
        assert_eq!(out.links.len(), 2);
        assert!(out.links.iter().all(|l| l.action == LinkAction::Created));

        let claude = project.join(".claude/skills/greeter");
        let cursor = project.join(".cursor/skills/greeter");
        assert!(claude.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(cursor.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&claude).unwrap(), out.source_path);
        assert_eq!(
            fs::read_to_string(claude.join("SKILL.md")).unwrap(),
            "# greeter\n"
        );
        assert!(!project.join(".codex").exists());

        let manifest = load_manifest(&project).unwrap();
        assert_eq!(manifest.skills.len(), 1);
        assert_eq!(manifest.skills[0].name, "greeter");
        assert_eq!(manifest.skills[0].mode, LINK_MODE);
        assert!(fs::read_to_string(manifest_path(&project))
            .unwrap()
            .contains("symlink"));
    }

    #[test]
    fn use_is_idempotent_and_includes_codex_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(home.join(".codex/skills")).unwrap();
        let skill = demo_skill(&home, "greeter");

        activate(&project, &home, &skill).unwrap();
        let again = activate(&project, &home, &skill).unwrap();
        assert!(again
            .links
            .iter()
            .all(|l| l.action == LinkAction::Unchanged));
        assert_eq!(again.links.len(), 3);
        assert!(project
            .join(".codex/skills/greeter")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(load_manifest(&project).unwrap().skills.len(), 1);
    }

    #[test]
    fn unuse_removes_symlinks_and_manifest_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        activate(&project, &home, &skill).unwrap();

        let out = deactivate(&project, &home, "greeter").unwrap();
        assert!(out.was_listed);
        assert!(out.links.iter().all(|l| l.action == LinkAction::Removed));
        assert!(!project.join(".claude/skills/greeter").exists());
        assert!(!project.join(".cursor/skills/greeter").exists());
        assert!(load_manifest(&project).unwrap().skills.is_empty());
    }

    #[test]
    fn refuses_to_clobber_real_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        let skill = demo_skill(&home, "greeter");
        fs::create_dir_all(project.join(".claude/skills/greeter")).unwrap();
        fs::write(project.join(".claude/skills/greeter/SKILL.md"), "mine").unwrap();

        let err = activate(&project, &home, &skill).unwrap_err().to_string();
        assert!(err.contains("not a symlink"), "{err}");
        assert_eq!(
            fs::read_to_string(project.join(".claude/skills/greeter/SKILL.md")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn unuse_unknown_skill_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let home = tmp.path().join("home");
        fs::create_dir_all(&project).unwrap();
        let err = deactivate(&project, &home, "missing")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not activated"), "{err}");
    }

    #[test]
    fn rejects_unsafe_skill_names() {
        assert!(validate_skill_name("greeter").is_ok());
        assert!(validate_skill_name("../etc").is_err());
        assert!(validate_skill_name("a/b").is_err());
        assert!(validate_skill_name(".hidden").is_err());
        assert!(validate_skill_name("").is_err());
    }
}
