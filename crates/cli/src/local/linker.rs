//! Project multi-agent linker: symlink skills into agent dirs + `skills.toml`.
//!
//! Canonical dest is **`.agents/skills`**. Legacy `.claude` / `.cursor` (and
//! optional `.codex`) are still linked by default until `skl migrate targets`.
//! Default is **symlink**. On EPERM / ENOTSUP / Windows privilege errors,
//! fall back to a copy on every dest. Codex is optional if present.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::SkillRoot;
use crate::error::{Result, SklError};
use crate::local::skills::{hash_skill_dir, is_safe_file_path, DiscoveredSkill};

pub const LINK_MODE: &str = "symlink";
pub const COPY_MODE: &str = "copy";
pub const MANIFEST_NAME: &str = "skills.toml";

/// `skl doctor` Windows note — directory symlinks need a privilege.
pub const WINDOWS_SYMLINK_NOTE: &str =
    "directory symlinks need Developer Mode or SeCreateSymbolicLink; use copies on EPERM";

const MANIFEST_HEADER: &str =
    "# Managed by `skl use` / `skl unuse`. Prefer symlink; copy if the filesystem refuses.\n\n";

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static FORCE_SYMLINK_FAIL: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTargetKind {
    Canonical,
    Legacy,
    OptIn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    pub id: String,
    pub path: PathBuf,
    pub kind: LinkTargetKind,
}

fn default_canonical_ids() -> Vec<String> {
    vec!["agents".to_string()]
}

fn default_extra_ids() -> Vec<String> {
    vec!["claude".to_string(), "cursor".to_string()]
}

/// `[targets]` in `skills.toml` — ids only (`agents` / `claude` / `cursor` / `codex`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestTargets {
    #[serde(default = "default_canonical_ids")]
    pub canonical: Vec<String>,
    #[serde(default = "default_extra_ids")]
    pub extra: Vec<String>,
}

impl Default for ManifestTargets {
    fn default() -> Self {
        Self {
            canonical: default_canonical_ids(),
            extra: default_extra_ids(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SkillsManifest {
    #[serde(default)]
    pub targets: ManifestTargets,
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
    Copied,
    CopyReplaced,
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
    pub mode: String,
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeactivateOutcome {
    pub skill: String,
    pub links: Vec<LinkChange>,
    pub manifest: PathBuf,
    pub was_listed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymlinkProbe {
    pub ok: bool,
    pub detail: String,
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

/// Project dests that receive links (symlink, or copy on fallback).
///
/// Canonical `.agents/skills` first, then legacy Claude/Cursor. Codex only if
/// home `~/.codex/skills` exists or the project already has `.codex`.
pub fn project_link_targets(project: &Path, home: &Path) -> Vec<LinkTarget> {
    let mut targets = vec![
        LinkTarget {
            id: "agents".to_string(),
            path: project.join(".agents").join("skills"),
            kind: LinkTargetKind::Canonical,
        },
        LinkTarget {
            id: "claude".to_string(),
            path: project.join(".claude").join("skills"),
            kind: LinkTargetKind::Legacy,
        },
        LinkTarget {
            id: "cursor".to_string(),
            path: project.join(".cursor").join("skills"),
            kind: LinkTargetKind::Legacy,
        },
    ];
    if include_codex(project, home) {
        targets.push(LinkTarget {
            id: "codex".to_string(),
            path: project.join(".codex").join("skills"),
            kind: LinkTargetKind::OptIn,
        });
    }
    targets
}

fn include_codex(project: &Path, home: &Path) -> bool {
    home.join(".codex").join("skills").is_dir() || project.join(".codex").exists()
}

fn intern_target_id(id: &str) -> &'static str {
    match id {
        "agents" => "agents",
        "claude" => "claude",
        "cursor" => "cursor",
        "codex" => "codex",
        _ => "agents",
    }
}

/// Thin wrapper over [`project_link_targets`] for callers that still want
/// [`SkillRoot`]. Prefer `project_link_targets`.
pub fn project_link_roots(project: &Path, home: &Path) -> Vec<SkillRoot> {
    project_link_targets(project, home)
        .into_iter()
        .map(|target| SkillRoot {
            source: intern_target_id(&target.id),
            path: target.path,
        })
        .collect()
}

/// Destinations written by `skl use` / removed by `skl unuse`.
///
/// Always includes canonical `.agents`. Also includes configured extras
/// (default `claude` + `cursor`) and opt-in `codex` when present.
pub fn destinations_for(project: &Path, home: &Path, targets: &ManifestTargets) -> Vec<LinkTarget> {
    let extras_default = targets.extra == default_extra_ids();
    project_link_targets(project, home)
        .into_iter()
        .filter(|target| match target.kind {
            LinkTargetKind::Canonical => true,
            LinkTargetKind::Legacy => targets.extra.iter().any(|id| id == &target.id),
            LinkTargetKind::OptIn => {
                extras_default || targets.extra.iter().any(|id| id == &target.id)
            }
        })
        .collect()
}

/// M0 layout: `skills.toml` / activated skills under `.claude`/`.cursor`, but
/// `.agents/skills` is missing. Doctor warns; does not mutate.
pub fn m0_targets_warning(project: &Path) -> Option<String> {
    if !is_m0_layout(project) {
        return None;
    }
    Some(
        "M0 layout (.claude/.cursor links, no .agents/skills); run `skl migrate targets`"
            .to_string(),
    )
}

fn is_m0_layout(project: &Path) -> bool {
    if project.join(".agents").join("skills").exists() {
        return false;
    }
    if !manifest_path(project).is_file() {
        return false;
    }
    let Ok(manifest) = load_manifest(project) else {
        return false;
    };
    if !manifest.skills.is_empty() {
        return true;
    }
    has_skill_dirs(&project.join(".claude").join("skills"))
        || has_skill_dirs(&project.join(".cursor").join("skills"))
}

fn has_skill_dirs(root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.filter_map(|entry| entry.ok()).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        !name.starts_with('.') && entry.path().is_dir()
    })
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
    let mut manifest = load_manifest(project)?;
    let prior_copy = manifest
        .skills
        .iter()
        .any(|s| s.name == skill.name && s.mode == COPY_MODE);

    let mut links = Vec::new();
    let mut used_copy = false;
    let mut fallback = None;
    for target in destinations_for(project, home, &manifest.targets) {
        let dest = target.path.join(&skill.name);
        let placed = ensure_link(&source, &dest, prior_copy)?;
        if placed.mode == COPY_MODE {
            used_copy = true;
            if fallback.is_none() {
                fallback = placed.fallback;
            }
        }
        links.push(LinkChange {
            agent: target.id,
            path: dest,
            action: placed.action,
        });
    }

    let mode = if used_copy {
        COPY_MODE.to_string()
    } else {
        LINK_MODE.to_string()
    };
    let entry = ActivatedSkill {
        name: skill.name.clone(),
        source: skill.source.clone(),
        path: source.to_string_lossy().into_owned(),
        mode: mode.clone(),
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
        mode,
        fallback,
    })
}

pub fn deactivate(project: &Path, home: &Path, name: &str) -> Result<DeactivateOutcome> {
    validate_skill_name(name)?;

    let mut manifest = load_manifest(project)?;
    let listed = manifest.skills.iter().find(|s| s.name == name).cloned();
    let was_listed = listed.is_some();
    let allow_copy_dir = listed
        .as_ref()
        .map(|s| s.mode == COPY_MODE)
        .unwrap_or(false);
    manifest.skills.retain(|s| s.name != name);

    let mut links = Vec::new();
    for target in destinations_for(project, home, &manifest.targets) {
        let dest = target.path.join(name);
        let action = remove_managed_link(&dest, allow_copy_dir)?;
        links.push(LinkChange {
            agent: target.id,
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

/// Probe whether `in_dir` can hold a directory symlink (temp name, always cleaned up).
pub fn probe_symlink(in_dir: &Path) -> SymlinkProbe {
    if !in_dir.is_dir() {
        return SymlinkProbe {
            ok: false,
            detail: "not a directory".into(),
        };
    }
    let link = in_dir.join(format!(".skl-link-probe-{}", std::process::id()));
    let _ = fs::remove_file(&link);
    let result = match create_symlink_io(in_dir, &link) {
        Ok(()) => {
            let _ = fs::remove_file(&link);
            SymlinkProbe {
                ok: true,
                detail: "ok".into(),
            }
        }
        Err(err) => {
            let _ = fs::remove_file(&link);
            SymlinkProbe {
                ok: false,
                detail: err.to_string(),
            }
        }
    };
    result
}

pub fn is_symlink_fallback_error(err: &std::io::Error) -> bool {
    if matches!(
        err.kind(),
        ErrorKind::PermissionDenied | ErrorKind::Unsupported
    ) {
        return true;
    }
    // Windows privilege / unsupported FS; Unix EPERM/EACCES/ENOTSUP/EOPNOTSUPP.
    matches!(
        err.raw_os_error(),
        Some(1) | Some(5) | Some(13) | Some(45) | Some(95) | Some(102) | Some(1314)
    )
}

fn canonicalize_dir(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path)
        .map_err(|err| SklError::LocalState(format!("cannot resolve {}: {err}", path.display())))
}

struct Placed {
    action: LinkAction,
    mode: &'static str,
    fallback: Option<String>,
}

fn ensure_link(target: &Path, dest: &Path, managed_copy: bool) -> Result<Placed> {
    match dest_kind(dest)? {
        DestKind::Missing => {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            place_link(target, dest, false)
        }
        DestKind::Symlink => {
            let current = fs::read_link(dest)?;
            if same_path(&current, target, dest) {
                Ok(Placed {
                    action: LinkAction::Unchanged,
                    mode: LINK_MODE,
                    fallback: None,
                })
            } else {
                fs::remove_file(dest)?;
                place_link(target, dest, true)
            }
        }
        DestKind::Directory if managed_copy => refresh_copy(target, dest),
        DestKind::Directory | DestKind::Other => Err(SklError::LocalState(format!(
            "{} exists and is not a symlink; refusing to overwrite (skl use will not clobber a real directory)",
            dest.display()
        ))),
    }
}

fn place_link(target: &Path, dest: &Path, replacing: bool) -> Result<Placed> {
    match create_symlink_io(target, dest) {
        Ok(()) => Ok(Placed {
            action: if replacing {
                LinkAction::Replaced
            } else {
                LinkAction::Created
            },
            mode: LINK_MODE,
            fallback: None,
        }),
        Err(err) if is_symlink_fallback_error(&err) => {
            let reason = err.to_string();
            copy_skill_tree(target, dest)?;
            Ok(Placed {
                action: if replacing {
                    LinkAction::CopyReplaced
                } else {
                    LinkAction::Copied
                },
                mode: COPY_MODE,
                fallback: Some(reason),
            })
        }
        Err(err) => Err(SklError::from(err)),
    }
}

fn refresh_copy(target: &Path, dest: &Path) -> Result<Placed> {
    if same_skill_tree(target, dest) {
        return Ok(Placed {
            action: LinkAction::Unchanged,
            mode: COPY_MODE,
            fallback: None,
        });
    }
    copy_skill_tree(target, dest)?;
    Ok(Placed {
        action: LinkAction::CopyReplaced,
        mode: COPY_MODE,
        fallback: None,
    })
}

fn remove_managed_link(dest: &Path, allow_copy_dir: bool) -> Result<LinkAction> {
    match dest_kind(dest)? {
        DestKind::Missing => Ok(LinkAction::Absent),
        DestKind::Symlink => {
            fs::remove_file(dest)?;
            Ok(LinkAction::Removed)
        }
        DestKind::Directory if allow_copy_dir => {
            fs::remove_dir_all(dest)?;
            Ok(LinkAction::Removed)
        }
        DestKind::Directory | DestKind::Other => Err(SklError::LocalState(format!(
            "{} exists and is not a symlink; refusing to delete",
            dest.display()
        ))),
    }
}

enum DestKind {
    Missing,
    Symlink,
    Directory,
    Other,
}

fn dest_kind(path: &Path) -> Result<DestKind> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(DestKind::Symlink),
        Ok(meta) if meta.file_type().is_dir() => Ok(DestKind::Directory),
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

fn same_skill_tree(a: &Path, b: &Path) -> bool {
    match (hash_skill_dir(a), hash_skill_dir(b)) {
        (Ok(left), Ok(right)) => left.tree_hash == right.tree_hash,
        _ => false,
    }
}

fn copy_skill_tree(src: &Path, dest: &Path) -> Result<()> {
    match dest_kind(dest) {
        Ok(DestKind::Directory) => fs::remove_dir_all(dest)?,
        Ok(DestKind::Symlink) | Ok(DestKind::Other) => {
            fs::remove_file(dest)?;
        }
        Ok(DestKind::Missing) | Err(_) => {}
    }
    fs::create_dir_all(dest)?;
    for entry in walkdir::WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != ".DS_Store" && name != "node_modules"
        })
    {
        let entry =
            entry.map_err(|err| SklError::LocalState(format!("copy {}: {err}", src.display())))?;
        if entry.path() == src {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(src)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() || !is_safe_file_path(&rel) {
            continue;
        }
        let to = dest.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if entry.file_type().is_dir() {
            fs::create_dir_all(&to)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

fn create_symlink_io(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    {
        if FORCE_SYMLINK_FAIL.with(Cell::get) {
            return Err(std::io::Error::new(
                ErrorKind::PermissionDenied,
                "EPERM: operation not permitted",
            ));
        }
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(
            ErrorKind::Unsupported,
            "symlinks are not supported on this platform",
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

    fn with_forced_symlink_fail<T>(f: impl FnOnce() -> T) -> T {
        FORCE_SYMLINK_FAIL.with(|cell| cell.set(true));
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        FORCE_SYMLINK_FAIL.with(|cell| cell.set(false));
        match out {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    #[test]
    fn use_symlinks_agents_then_claude_and_cursor_and_writes_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        let out = activate(&project, &home, &skill).unwrap();
        assert_eq!(out.skill, "greeter");
        assert_eq!(out.links.len(), 3);
        assert_eq!(
            out.links
                .iter()
                .map(|l| l.agent.as_str())
                .collect::<Vec<_>>(),
            ["agents", "claude", "cursor"]
        );
        assert!(out.links.iter().all(|l| l.action == LinkAction::Created));
        assert_eq!(out.mode, LINK_MODE);
        assert!(out.fallback.is_none());

        let agents = project.join(".agents/skills/greeter");
        let claude = project.join(".claude/skills/greeter");
        let cursor = project.join(".cursor/skills/greeter");
        assert!(agents.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(claude.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(cursor.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&agents).unwrap(), out.source_path);
        assert_eq!(fs::read_link(&claude).unwrap(), out.source_path);
        assert_eq!(
            fs::read_to_string(agents.join("SKILL.md")).unwrap(),
            "# greeter\n"
        );
        assert!(!project.join(".codex").exists());

        let manifest = load_manifest(&project).unwrap();
        assert_eq!(manifest.targets.canonical, ["agents"]);
        assert_eq!(manifest.targets.extra, ["claude", "cursor"]);
        assert_eq!(manifest.skills.len(), 1);
        assert_eq!(manifest.skills[0].name, "greeter");
        assert_eq!(manifest.skills[0].mode, LINK_MODE);
        let raw = fs::read_to_string(manifest_path(&project)).unwrap();
        assert!(raw.contains("symlink"));
        assert!(raw.contains("[targets]"));
        assert!(raw.contains("agents"));
    }

    #[test]
    fn activate_creates_agents_skills_first() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        let out = activate(&project, &home, &skill).unwrap();
        assert_eq!(out.links[0].agent, "agents");
        assert_eq!(
            out.links[0].path,
            project.join(".agents").join("skills").join("greeter")
        );
        assert!(project
            .join(".agents/skills/greeter")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(project
            .join(".claude/skills/greeter")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(project
            .join(".cursor/skills/greeter")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn project_link_targets_canonical_first_codex_opt_in() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();

        let ids: Vec<_> = project_link_targets(&project, &home)
            .into_iter()
            .map(|t| (t.id, t.kind))
            .collect();
        assert_eq!(
            ids,
            [
                ("agents".into(), LinkTargetKind::Canonical),
                ("claude".into(), LinkTargetKind::Legacy),
                ("cursor".into(), LinkTargetKind::Legacy),
            ]
        );

        fs::create_dir_all(home.join(".codex/skills")).unwrap();
        let with_codex = project_link_targets(&project, &home);
        assert_eq!(with_codex.last().unwrap().id, "codex");
        assert_eq!(with_codex.last().unwrap().kind, LinkTargetKind::OptIn);
        assert_eq!(
            with_codex.last().unwrap().path,
            project.join(".codex").join("skills")
        );

        let roots = project_link_roots(&project, &home);
        assert_eq!(
            roots.iter().map(|r| r.source).collect::<Vec<_>>(),
            ["agents", "claude", "cursor", "codex"]
        );
    }

    #[test]
    fn missing_targets_table_defaults_to_agents_plus_legacy() {
        let raw = r#"
[[skills]]
name = "greeter"
source = "claude"
path = "/tmp/greeter"
mode = "symlink"
"#;
        let manifest: SkillsManifest = toml::from_str(raw).unwrap();
        assert_eq!(manifest.targets.canonical, ["agents"]);
        assert_eq!(manifest.targets.extra, ["claude", "cursor"]);
        assert_eq!(manifest.skills.len(), 1);
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
        assert_eq!(again.links.len(), 4);
        assert_eq!(
            again
                .links
                .iter()
                .map(|l| l.agent.as_str())
                .collect::<Vec<_>>(),
            ["agents", "claude", "cursor", "codex"]
        );
        assert!(project
            .join(".agents/skills/greeter")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
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
        assert!(!project.join(".agents/skills/greeter").exists());
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

    #[test]
    fn falls_back_to_copy_when_symlink_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        let out = with_forced_symlink_fail(|| activate(&project, &home, &skill)).unwrap();
        assert_eq!(out.mode, COPY_MODE);
        assert!(out.fallback.as_ref().unwrap().contains("EPERM"), "{out:?}");
        assert!(out.links.iter().all(|l| l.action == LinkAction::Copied));

        let agents = project.join(".agents/skills/greeter");
        let claude = project.join(".claude/skills/greeter");
        let cursor = project.join(".cursor/skills/greeter");
        assert!(agents.is_dir());
        assert!(!agents.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(claude.is_dir());
        assert!(!claude.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(cursor.is_dir());
        assert_eq!(
            fs::read_to_string(agents.join("SKILL.md")).unwrap(),
            "# greeter\n"
        );

        let manifest = load_manifest(&project).unwrap();
        assert_eq!(manifest.skills[0].mode, COPY_MODE);
        assert!(fs::read_to_string(manifest_path(&project))
            .unwrap()
            .contains("copy"));

        let again = with_forced_symlink_fail(|| activate(&project, &home, &skill)).unwrap();
        assert!(again
            .links
            .iter()
            .all(|l| l.action == LinkAction::Unchanged));
        assert_eq!(again.mode, COPY_MODE);
    }

    #[test]
    fn unuse_removes_copy_mode_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        with_forced_symlink_fail(|| activate(&project, &home, &skill)).unwrap();
        assert!(project.join(".claude/skills/greeter").is_dir());

        let out = deactivate(&project, &home, "greeter").unwrap();
        assert!(out.was_listed);
        assert!(out.links.iter().all(|l| l.action == LinkAction::Removed));
        assert!(!project.join(".agents/skills/greeter").exists());
        assert!(!project.join(".claude/skills/greeter").exists());
        assert!(!project.join(".cursor/skills/greeter").exists());
        assert!(load_manifest(&project).unwrap().skills.is_empty());
    }

    #[test]
    fn unuse_still_refuses_unmanaged_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(project.join(".claude/skills/greeter")).unwrap();
        fs::write(project.join(".claude/skills/greeter/SKILL.md"), "mine").unwrap();

        let err = deactivate(&project, &home, "greeter")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a symlink"), "{err}");
        assert_eq!(
            fs::read_to_string(project.join(".claude/skills/greeter/SKILL.md")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn fallback_error_matches_eperm_enotsup_privilege() {
        assert!(is_symlink_fallback_error(&std::io::Error::new(
            ErrorKind::PermissionDenied,
            "EPERM"
        )));
        assert!(is_symlink_fallback_error(&std::io::Error::new(
            ErrorKind::Unsupported,
            "ENOTSUP"
        )));
        assert!(is_symlink_fallback_error(
            &std::io::Error::from_raw_os_error(1314)
        ));
        assert!(!is_symlink_fallback_error(&std::io::Error::new(
            ErrorKind::AlreadyExists,
            "exists"
        )));
    }

    #[test]
    fn probe_symlink_reports_ok_in_temp() {
        let tmp = tempfile::tempdir().unwrap();
        let probe = probe_symlink(tmp.path());
        assert!(probe.ok, "{probe:?}");
        assert_eq!(probe.detail, "ok");
    }

    #[test]
    fn probe_symlink_unavailable_when_forced() {
        let tmp = tempfile::tempdir().unwrap();
        let probe = with_forced_symlink_fail(|| probe_symlink(tmp.path()));
        assert!(!probe.ok, "{probe:?}");
        assert!(probe.detail.contains("EPERM"), "{probe:?}");
    }

    #[test]
    fn extras_can_omit_cursor_but_always_writes_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        save_manifest(
            &project,
            &SkillsManifest {
                targets: ManifestTargets {
                    canonical: vec!["agents".into()],
                    extra: vec!["claude".into()],
                },
                skills: Vec::new(),
            },
        )
        .unwrap();
        let skill = demo_skill(&home, "greeter");
        let out = activate(&project, &home, &skill).unwrap();
        assert_eq!(
            out.links
                .iter()
                .map(|l| l.agent.as_str())
                .collect::<Vec<_>>(),
            ["agents", "claude"]
        );
        assert!(project.join(".agents/skills/greeter").exists());
        assert!(project.join(".claude/skills/greeter").exists());
        assert!(!project.join(".cursor").exists());
    }

    #[test]
    fn doctor_warns_on_m0_layout_without_mutating() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        assert!(m0_targets_warning(&project).is_none());

        let skill = demo_skill(&home, "greeter");
        let dest = project.join(".claude/skills/greeter");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&skill.path, &dest).unwrap();
        save_manifest(
            &project,
            &SkillsManifest {
                targets: ManifestTargets::default(),
                skills: vec![ActivatedSkill {
                    name: "greeter".into(),
                    source: "claude".into(),
                    path: skill.path.to_string_lossy().into_owned(),
                    mode: LINK_MODE.into(),
                }],
            },
        )
        .unwrap();

        let warning = m0_targets_warning(&project).expect("M0 warning");
        assert!(warning.contains("skl migrate targets"), "{warning}");
        assert!(!project.join(".agents").exists());
        assert!(project.join(".claude/skills/greeter").exists());
        assert_eq!(load_manifest(&project).unwrap().skills.len(), 1);

        fs::create_dir_all(project.join(".agents/skills")).unwrap();
        assert!(
            m0_targets_warning(&project).is_none(),
            "presence of .agents/skills is enough"
        );
    }
}
