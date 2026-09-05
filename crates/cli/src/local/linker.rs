//! Project multi-agent linker: symlink skills into agent dirs + `skills.toml`.
//!
//! Canonical dest is **`.agents/skills`**. Extra dests (custom catalog project
//! dirs such as `.claude/skills`) are created only when opted in via sticky
//! config, project `skills.toml` `[targets].extra`, or `skl use -a`. Universal
//! agents (cursor, codex, amp, …) already read `.agents/skills` — never extras.
//! Default is **symlink**.
//! On EPERM / ENOTSUP / Windows privilege errors, fall back to a copy on
//! every dest that is active.

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
    "# Managed by `skl use` / `skl unuse` / `skl migrate targets`. Prefer symlink; copy if the filesystem refuses.\n\n";

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

pub const CANONICAL_TARGET_ID: &str = "agents";
/// Example custom extra only — do not dump the full catalog in help text.
pub const EXTRA_TARGET_EXAMPLES: &[&str] = &["claude-code"];

fn default_canonical_ids() -> Vec<String> {
    vec![CANONICAL_TARGET_ID.to_string()]
}

fn default_extra_ids() -> Vec<String> {
    Vec::new()
}

/// `[targets]` in `skills.toml` — ids only (`agents` + custom catalog extras).
/// Missing table → canonical=`[agents]`, extra=`[]`.
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

pub fn is_canonical_id(id: &str) -> bool {
    id.eq_ignore_ascii_case(CANONICAL_TARGET_ID)
}

pub fn intern_extra_id(id: &str) -> Option<&'static str> {
    let id = crate::catalog::canonicalize_id(id);
    if crate::catalog::is_custom_project(id) {
        crate::catalog::intern_id(id)
    } else {
        None
    }
}

/// Validate extra dest ids (custom catalog only). Rejects `agents` and universal ids.
pub fn normalize_extra_ids(ids: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for raw in ids {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_canonical_id(trimmed) {
            return Err(SklError::LocalState(
                "`agents` is always the canonical dest; extras are custom catalog ids (e.g. claude-code)".into(),
            ));
        }
        let canon = crate::catalog::canonicalize_id(trimmed);
        if crate::catalog::is_universal(canon) {
            return Err(SklError::LocalState(format!(
                "`{trimmed}` uses project `.agents/skills` (already the canonical dest); extras are only for custom project dirs (e.g. claude-code)"
            )));
        }
        let Some(id) = intern_extra_id(trimmed) else {
            return Err(SklError::LocalState(format!(
                "unknown target `{trimmed}` (custom catalog ids, e.g. claude-code)"
            )));
        };
        if !out.iter().any(|existing: &String| existing == id) {
            out.push(id.to_string());
        }
    }
    Ok(sort_extra_ids(out))
}

/// Rename `claude` → `claude-code`; drop cursor/codex/other universal extras.
pub fn migrate_extra_ids(ids: &[String]) -> (Vec<String>, Vec<String>) {
    let mut out = Vec::new();
    let mut warns = Vec::new();
    for raw in ids {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_canonical_id(trimmed) {
            continue;
        }
        if trimmed.eq_ignore_ascii_case(crate::catalog::CLAUDE_ALIAS) {
            warns.push("renamed extra `claude` → `claude-code`".into());
            if !out
                .iter()
                .any(|existing| existing == crate::catalog::CLAUDE_CODE_ID)
            {
                out.push(crate::catalog::CLAUDE_CODE_ID.to_string());
            }
            continue;
        }
        if let Some(id) = intern_extra_id(trimmed) {
            if !out.iter().any(|existing| existing == id) {
                out.push(id.to_string());
            }
            continue;
        }
        if crate::catalog::is_universal(trimmed)
            || matches!(trimmed.to_ascii_lowercase().as_str(), "cursor" | "codex")
        {
            warns.push(format!(
                "removed extra `{trimmed}` (project dir is already .agents/skills; no-op)"
            ));
            continue;
        }
        warns.push(format!("ignored unknown extra `{trimmed}`"));
    }
    (sort_extra_ids(out), warns)
}

/// Keep known extras; drop unknown ids (sticky config may be hand-edited).
pub fn filter_extra_ids(ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for raw in ids {
        if let Some(id) = intern_extra_id(raw.trim()) {
            if !out.iter().any(|existing: &String| existing == id) {
                out.push(id.to_string());
            }
        }
    }
    sort_extra_ids(out)
}

pub fn merge_extra_ids(layers: &[&[String]]) -> Vec<String> {
    let mut combined = Vec::new();
    for layer in layers {
        combined.extend(filter_extra_ids(layer));
    }
    filter_extra_ids(&combined)
}

fn sort_extra_ids(mut ids: Vec<String>) -> Vec<String> {
    let order = crate::catalog::custom_project_ids();
    ids.sort_by_key(|id| {
        order
            .iter()
            .position(|known| *known == id.as_str())
            .unwrap_or(99)
    });
    ids
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

/// Catalog of known dests. Callers that write must go through
/// [`destinations_for`] so unused harness dirs are not created.
///
/// Canonical `.agents/skills` first, then optional extras. `home` is unused
/// (kept so existing call sites stay stable).
pub fn project_link_targets(project: &Path, _home: &Path) -> Vec<LinkTarget> {
    let mut out = vec![LinkTarget {
        id: CANONICAL_TARGET_ID.to_string(),
        path: project.join(".agents").join("skills"),
        kind: LinkTargetKind::Canonical,
    }];
    for entry in crate::catalog::agents() {
        if entry.project_skills_dir == crate::catalog::UNIVERSAL_PROJECT_DIR {
            continue;
        }
        out.push(LinkTarget {
            id: entry.id.to_string(),
            path: project.join(entry.project_skills_dir),
            kind: LinkTargetKind::OptIn,
        });
    }
    // M0 litter: old universal project dirs. Prune/scan only — never extras.
    for (id, rel) in [("cursor", ".cursor/skills"), ("codex", ".codex/skills")] {
        let path = project.join(rel);
        if out.iter().any(|target| target.path == path) {
            continue;
        }
        out.push(LinkTarget {
            id: id.to_string(),
            path,
            kind: LinkTargetKind::Legacy,
        });
    }
    out
}

fn intern_target_id(id: &str) -> &'static str {
    if is_canonical_id(id) {
        return CANONICAL_TARGET_ID;
    }
    crate::catalog::intern_id(id).unwrap_or(CANONICAL_TARGET_ID)
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
/// Always includes canonical `.agents`. Extra dests only when listed in
/// `targets.extra` (custom catalog ids). Legacy cursor/codex dirs are never written.
pub fn destinations_for(project: &Path, home: &Path, targets: &ManifestTargets) -> Vec<LinkTarget> {
    let extras = filter_extra_ids(&targets.extra);
    project_link_targets(project, home)
        .into_iter()
        .filter(|target| match target.kind {
            LinkTargetKind::Canonical => true,
            LinkTargetKind::OptIn => extras.iter().any(|id| id == &target.id),
            LinkTargetKind::Legacy => false,
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

pub fn is_m0_layout(project: &Path) -> bool {
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
    let mut manifest: SkillsManifest =
        toml::from_str(&raw).map_err(|err| SklError::Config(format!("{MANIFEST_NAME}: {err}")))?;
    let (extras, warns) = migrate_extra_ids(&manifest.targets.extra);
    if extras != manifest.targets.extra {
        for warn in &warns {
            eprintln!("warn: {warn}");
        }
        manifest.targets.extra = extras;
    }
    Ok(manifest)
}

pub fn save_manifest(project: &Path, manifest: &SkillsManifest) -> Result<()> {
    let mut sorted = manifest.clone();
    sorted.skills.sort_by(|a, b| a.name.cmp(&b.name));
    let body = toml::to_string_pretty(&sorted).map_err(|err| SklError::Config(err.to_string()))?;
    fs::write(manifest_path(project), format!("{MANIFEST_HEADER}{body}"))?;
    Ok(())
}

pub fn activate(project: &Path, home: &Path, skill: &DiscoveredSkill) -> Result<ActivateOutcome> {
    activate_with_extras(project, home, skill, &[])
}

/// Activate into canonical `.agents/skills` plus `extras` (merged with any
/// extras already stored in the project `skills.toml`).
pub fn activate_with_extras(
    project: &Path,
    home: &Path,
    skill: &DiscoveredSkill,
    extras: &[String],
) -> Result<ActivateOutcome> {
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
    let extras = merge_extra_ids(&[&manifest.targets.extra, extras]);
    manifest.targets.canonical = default_canonical_ids();
    manifest.targets.extra = extras;
    let prior_copy = manifest
        .skills
        .iter()
        .any(|s| s.name == skill.name && s.mode == COPY_MODE);

    let targets = destinations_for(project, home, &manifest.targets);
    preflight_dests(&targets, &skill.name, prior_copy)?;

    let mut links = Vec::new();
    let mut used_copy = false;
    let mut fallback = None;
    for target in targets {
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
    deactivate_with_extras(project, home, name, &[])
}

pub fn deactivate_with_extras(
    project: &Path,
    home: &Path,
    name: &str,
    extras: &[String],
) -> Result<DeactivateOutcome> {
    validate_skill_name(name)?;

    let mut manifest = load_manifest(project)?;
    let listed = manifest.skills.iter().find(|s| s.name == name).cloned();
    let was_listed = listed.is_some();
    let allow_copy_dir = listed
        .as_ref()
        .map(|s| s.mode == COPY_MODE)
        .unwrap_or(false);
    manifest.skills.retain(|s| s.name != name);
    manifest.targets.extra = merge_extra_ids(&[&manifest.targets.extra, extras]);

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

pub struct Placed {
    pub action: LinkAction,
    pub mode: &'static str,
    pub fallback: Option<String>,
}

fn preflight_dests(targets: &[LinkTarget], skill: &str, managed_copy: bool) -> Result<()> {
    for target in targets {
        let dest = target.path.join(skill);
        if dest_would_conflict(&dest, managed_copy)? {
            return Err(SklError::LocalState(format!(
                "{} exists and is not a symlink; refusing to overwrite (skl use will not clobber a real directory)",
                dest.display()
            )));
        }
    }
    Ok(())
}

fn dest_would_conflict(dest: &Path, managed_copy: bool) -> Result<bool> {
    match dest_kind(dest)? {
        DestKind::Missing | DestKind::Symlink => Ok(false),
        DestKind::Directory if managed_copy => Ok(false),
        DestKind::Directory | DestKind::Other => Ok(true),
    }
}

pub fn ensure_link(target: &Path, dest: &Path, managed_copy: bool) -> Result<Placed> {
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

pub fn remove_managed_link(dest: &Path, allow_copy_dir: bool) -> Result<LinkAction> {
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
    fn use_default_writes_only_agents_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        let out = activate(&project, &home, &skill).unwrap();
        assert_eq!(out.skill, "greeter");
        assert_eq!(out.links.len(), 1);
        assert_eq!(out.links[0].agent, "agents");
        assert_eq!(out.links[0].action, LinkAction::Created);
        assert_eq!(out.mode, LINK_MODE);
        assert!(out.fallback.is_none());

        let agents = project.join(".agents/skills/greeter");
        assert!(agents.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&agents).unwrap(), out.source_path);
        assert_eq!(
            fs::read_to_string(agents.join("SKILL.md")).unwrap(),
            "# greeter\n"
        );
        assert!(!project.join(".claude").exists());
        assert!(!project.join(".cursor").exists());
        assert!(!project.join(".codex").exists());

        let manifest = load_manifest(&project).unwrap();
        assert_eq!(manifest.targets.canonical, ["agents"]);
        assert!(manifest.targets.extra.is_empty());
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
        assert!(!project.join(".claude").exists());
        assert!(!project.join(".cursor").exists());
    }

    #[test]
    fn extra_claude_also_creates_claude_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        let out = activate_with_extras(&project, &home, &skill, &["claude-code".into()]).unwrap();
        assert_eq!(
            out.links
                .iter()
                .map(|l| l.agent.as_str())
                .collect::<Vec<_>>(),
            ["agents", "claude-code"]
        );
        assert!(project.join(".agents/skills/greeter").exists());
        assert!(project.join(".claude/skills/greeter").exists());
        assert!(!project.join(".cursor").exists());
        assert_eq!(
            load_manifest(&project).unwrap().targets.extra,
            ["claude-code"]
        );
    }

    #[test]
    fn use_alone_writes_only_agents_skills_covering_cursor_codex() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        let out = activate(&project, &home, &skill).unwrap();
        assert_eq!(
            out.links
                .iter()
                .map(|l| l.agent.as_str())
                .collect::<Vec<_>>(),
            ["agents"]
        );
        assert!(project.join(".agents/skills/greeter").exists());
        assert!(!project.join(".claude").exists());
        assert!(!project.join(".cursor").exists());
        assert!(!project.join(".codex").exists());
    }

    #[test]
    fn project_link_targets_canonical_first_custom_not_universal_extras() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();

        let targets = project_link_targets(&project, &home);
        assert_eq!(targets[0].id, "agents");
        assert_eq!(targets[0].kind, LinkTargetKind::Canonical);
        assert!(targets
            .iter()
            .any(|t| t.id == "claude-code" && t.kind == LinkTargetKind::OptIn));
        assert!(targets
            .iter()
            .any(|t| t.id == "cursor" && t.kind == LinkTargetKind::Legacy));
        assert!(targets
            .iter()
            .any(|t| t.id == "codex" && t.kind == LinkTargetKind::Legacy));

        let dests = destinations_for(&project, &home, &ManifestTargets::default());
        assert_eq!(
            dests.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["agents"]
        );

        let roots = project_link_roots(&project, &home);
        let sources: Vec<_> = roots.iter().map(|r| r.source).collect();
        assert!(sources.contains(&"agents"));
        assert!(sources.contains(&"claude-code"));
    }

    #[test]
    fn missing_targets_table_defaults_to_agents_only() {
        let raw = r#"
[[skills]]
name = "greeter"
source = "claude"
path = "/tmp/greeter"
mode = "symlink"
"#;
        let manifest: SkillsManifest = toml::from_str(raw).unwrap();
        assert_eq!(manifest.targets.canonical, ["agents"]);
        assert!(manifest.targets.extra.is_empty());
        assert_eq!(manifest.skills.len(), 1);
    }

    #[test]
    fn use_is_idempotent_and_includes_codex_when_opted_in() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");

        activate_with_extras(&project, &home, &skill, &["claude-code".into()]).unwrap();
        let again = activate(&project, &home, &skill).unwrap();
        assert!(again
            .links
            .iter()
            .all(|l| l.action == LinkAction::Unchanged));
        assert_eq!(again.links.len(), 2);
        assert_eq!(
            again
                .links
                .iter()
                .map(|l| l.agent.as_str())
                .collect::<Vec<_>>(),
            ["agents", "claude-code"]
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
        assert!(!project.join(".cursor").exists());
        assert!(!project.join(".codex").exists());
        assert_eq!(load_manifest(&project).unwrap().skills.len(), 1);
    }

    #[test]
    fn unuse_removes_symlinks_and_manifest_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let skill = demo_skill(&home, "greeter");
        activate_with_extras(&project, &home, &skill, &["claude-code".into()]).unwrap();

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

        let err = activate_with_extras(&project, &home, &skill, &["claude-code".into()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a symlink"), "{err}");
        assert_eq!(
            fs::read_to_string(project.join(".claude/skills/greeter/SKILL.md")).unwrap(),
            "mine"
        );
        assert!(
            !project.join(".agents").exists(),
            "preflight must not create .agents when a later dest conflicts"
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

        let extras = vec!["claude-code".into()];
        let out =
            with_forced_symlink_fail(|| activate_with_extras(&project, &home, &skill, &extras))
                .unwrap();
        assert_eq!(out.mode, COPY_MODE);
        assert!(out.fallback.as_ref().unwrap().contains("EPERM"), "{out:?}");
        assert!(out.links.iter().all(|l| l.action == LinkAction::Copied));

        let agents = project.join(".agents/skills/greeter");
        let claude = project.join(".claude/skills/greeter");
        assert!(agents.is_dir());
        assert!(!agents.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(claude.is_dir());
        assert!(!claude.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(!project.join(".cursor").exists());
        assert_eq!(
            fs::read_to_string(agents.join("SKILL.md")).unwrap(),
            "# greeter\n"
        );

        let manifest = load_manifest(&project).unwrap();
        assert_eq!(manifest.skills[0].mode, COPY_MODE);
        assert!(fs::read_to_string(manifest_path(&project))
            .unwrap()
            .contains("copy"));

        let again =
            with_forced_symlink_fail(|| activate_with_extras(&project, &home, &skill, &extras))
                .unwrap();
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
        with_forced_symlink_fail(|| {
            activate_with_extras(&project, &home, &skill, &["claude-code".into()])
        })
        .unwrap();
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

        let err = deactivate_with_extras(&project, &home, "greeter", &["claude-code".into()])
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
    fn normalize_extra_ids_rejects_agents_and_universal() {
        assert_eq!(
            normalize_extra_ids(&["Claude".into()]).unwrap(),
            ["claude-code"]
        );
        assert_eq!(
            normalize_extra_ids(&["windsurf".into()]).unwrap(),
            ["windsurf"]
        );
        assert!(normalize_extra_ids(&["agents".into()]).is_err());
        let cursor_err = normalize_extra_ids(&["cursor".into()])
            .unwrap_err()
            .to_string();
        assert!(cursor_err.contains(".agents/skills"), "{cursor_err}");
        assert!(normalize_extra_ids(&["codex".into()]).is_err());
        assert!(normalize_extra_ids(&["amp".into()]).is_err());
        assert!(normalize_extra_ids(&["not-an-agent".into()]).is_err());
    }

    #[test]
    fn migrate_extra_ids_renames_claude_and_drops_cursor_codex() {
        let (ids, warns) = migrate_extra_ids(&[
            "claude".into(),
            "cursor".into(),
            "codex".into(),
            "windsurf".into(),
        ]);
        assert_eq!(ids, ["claude-code", "windsurf"]);
        assert!(warns.iter().any(|w| w.contains("claude-code")));
        assert!(warns.iter().any(|w| w.contains("cursor")));
        assert!(warns.iter().any(|w| w.contains("codex")));
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
                    extra: vec!["claude-code".into()],
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
            ["agents", "claude-code"]
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
