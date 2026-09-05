//! Vendored harness catalog from vercel-labs/skills `src/agents.ts`.
//!
//! Universal agents share project `.agents/skills`. Custom-project agents
//! (e.g. `claude-code` → `.claude/skills`) are the only valid `skl use -a`
//! / sticky extras. `cursor` and `codex` are universal — never extras.
//!
//! OpenClaw's dynamic global is baked to `{home}/.openclaw/skills` (agents.ts
//! also probes `~/.clawdbot` and `~/.moltbot`). Env-overridable homes
//! (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, …) use their default `~/.name` paths.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

/// Project dest shared by cursor, codex, amp, and other universal readers.
pub const UNIVERSAL_PROJECT_DIR: &str = ".agents/skills";
pub const CLAUDE_CODE_ID: &str = "claude-code";
pub const CLAUDE_ALIAS: &str = "claude";
/// Stable source id for `~/.agents/skills` (shared by several catalog agents).
pub const AGENTS_HOME_SOURCE: &str = "agents";
/// Stable source id for `~/.config/agents/skills`.
pub const XDG_AGENTS_SOURCE: &str = "xdg-agents";

/// Soft-prompt never dumps the full catalog (~50+ custom ids).
pub const SOFT_PROMPT_CAP: usize = 12;

const CATALOG_JSON: &str = include_str!("../data/agents-catalog.json");

#[derive(Debug, Clone, Deserialize)]
struct CatalogFile {
    agents: Vec<RawAgent>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAgent {
    id: String,
    name: Option<String>,
    project_skills_dir: String,
    #[serde(default)]
    global_skills_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    pub id: &'static str,
    pub name: Option<&'static str>,
    pub project_skills_dir: &'static str,
    pub global_skills_dir: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRoot {
    pub source: &'static str,
    pub path: PathBuf,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

pub fn agents() -> &'static [AgentEntry] {
    static AGENTS: OnceLock<Vec<AgentEntry>> = OnceLock::new();
    AGENTS.get_or_init(|| {
        let file: CatalogFile =
            serde_json::from_str(CATALOG_JSON).expect("embedded agents-catalog.json is valid");
        file.agents
            .into_iter()
            .map(|raw| AgentEntry {
                id: leak(raw.id),
                name: raw.name.map(leak),
                project_skills_dir: leak(raw.project_skills_dir),
                global_skills_dir: raw.global_skills_dir.map(leak),
            })
            .collect()
    })
}

/// `claude` → `claude-code`; other ids unchanged (case preserved for lookup).
pub fn canonicalize_id(id: &str) -> &str {
    if id.eq_ignore_ascii_case(CLAUDE_ALIAS) {
        CLAUDE_CODE_ID
    } else {
        id
    }
}

pub fn get(id: &str) -> Option<&'static AgentEntry> {
    let id = canonicalize_id(id);
    agents()
        .iter()
        .find(|entry| entry.id.eq_ignore_ascii_case(id))
}

pub fn intern_id(id: &str) -> Option<&'static str> {
    get(id).map(|entry| entry.id)
}

pub fn is_universal(id: &str) -> bool {
    get(id)
        .map(|entry| entry.project_skills_dir == UNIVERSAL_PROJECT_DIR)
        .unwrap_or(false)
}

/// Custom iff catalog `project_skills_dir` ≠ `.agents/skills`.
pub fn is_custom_project(id: &str) -> bool {
    get(id)
        .map(|entry| entry.project_skills_dir != UNIVERSAL_PROJECT_DIR)
        .unwrap_or(false)
}

pub fn custom_project_ids() -> Vec<&'static str> {
    agents()
        .iter()
        .filter(|entry| entry.project_skills_dir != UNIVERSAL_PROJECT_DIR)
        .map(|entry| entry.id)
        .collect()
}

pub fn resolve_global_dir(entry: &AgentEntry, home: &Path) -> Option<PathBuf> {
    entry
        .global_skills_dir
        .map(|template| expand_template(template, home))
}

pub fn expand_template(template: &str, home: &Path) -> PathBuf {
    let home_s = home.to_string_lossy();
    let xdg = xdg_config_dir(home);
    let xdg_s = xdg.to_string_lossy();
    PathBuf::from(
        template
            .replace("{home}", home_s.as_ref())
            .replace("{xdg_config}", xdg_s.as_ref()),
    )
}

/// XDG config relative to `home` (`$HOME/.config`) so tests stay isolated.
pub fn xdg_config_dir(home: &Path) -> PathBuf {
    home.join(".config")
}

pub fn agents_home_path(home: &Path) -> PathBuf {
    home.join(".agents").join("skills")
}

pub fn xdg_agents_path(home: &Path) -> PathBuf {
    xdg_config_dir(home).join("agents").join("skills")
}

/// Unique catalog globals, plus `~/.agents/skills` and `~/.config/agents/skills`.
/// Deduped by path. Those two stores keep source ids `agents` / `xdg-agents`.
pub fn unique_global_roots(home: &Path) -> Vec<SkillRoot> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    push_root(
        &mut out,
        &mut seen,
        AGENTS_HOME_SOURCE,
        agents_home_path(home),
    );
    push_root(
        &mut out,
        &mut seen,
        XDG_AGENTS_SOURCE,
        xdg_agents_path(home),
    );

    for entry in agents() {
        let Some(path) = resolve_global_dir(entry, home) else {
            continue;
        };
        if !seen.insert(norm_key(&path)) {
            continue;
        }
        out.push(SkillRoot {
            source: entry.id,
            path,
        });
    }
    out
}

fn norm_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn push_root(
    out: &mut Vec<SkillRoot>,
    seen: &mut HashSet<String>,
    source: &'static str,
    path: PathBuf,
) {
    if seen.insert(norm_key(&path)) {
        out.push(SkillRoot { source, path });
    }
}

/// Soft-prompt ids: detected installed custom agents ∪ `{claude-code}`
/// (if not already sticky). Never universal. Capped — never 50+.
pub fn soft_prompt_candidates(home: &Path, sticky: &[String]) -> Vec<&'static str> {
    let sticky_l: HashSet<String> = sticky
        .iter()
        .map(|id| canonicalize_id(id.trim()).to_ascii_lowercase())
        .collect();
    let mut out = Vec::new();

    if !sticky_l.contains(CLAUDE_CODE_ID) {
        out.push(CLAUDE_CODE_ID);
    }

    for entry in agents() {
        if out.len() >= SOFT_PROMPT_CAP {
            break;
        }
        if entry.project_skills_dir == UNIVERSAL_PROJECT_DIR {
            continue;
        }
        if entry.id == CLAUDE_CODE_ID {
            continue;
        }
        if sticky_l.contains(&entry.id.to_ascii_lowercase()) {
            continue;
        }
        if !custom_global_looks_installed(entry, home) {
            continue;
        }
        out.push(entry.id);
    }
    out
}

fn custom_global_looks_installed(entry: &AgentEntry, home: &Path) -> bool {
    let Some(path) = resolve_global_dir(entry, home) else {
        return false;
    };
    path.exists() || path.parent().is_some_and(|parent| parent.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_supported_agents() {
        assert!(
            agents().len() >= 70,
            "expected ~77 agents, got {}",
            agents().len()
        );
        assert!(get("claude-code").is_some());
        assert!(get("cursor").is_some());
        assert!(get("codex").is_some());
        assert!(get("openclaw").is_some());
        assert_eq!(
            get("openclaw").unwrap().global_skills_dir,
            Some("{home}/.openclaw/skills")
        );
    }

    #[test]
    fn universal_vs_custom_partition() {
        assert!(is_universal("cursor"));
        assert!(is_universal("codex"));
        assert!(is_universal("amp"));
        assert!(is_universal("CURSOR"));
        assert!(!is_custom_project("cursor"));
        assert!(!is_custom_project("codex"));
        assert!(is_custom_project("claude-code"));
        assert!(is_custom_project("claude"));
        assert!(is_custom_project("windsurf"));
        assert!(!is_universal("claude-code"));
        assert!(!is_universal("not-an-agent"));
    }

    #[test]
    fn resolve_global_dir_expands_placeholders() {
        let home = Path::new("/tmp/skl-home");
        let windsurf = get("windsurf").unwrap();
        assert_eq!(
            resolve_global_dir(windsurf, home).unwrap(),
            PathBuf::from("/tmp/skl-home/.codeium/windsurf/skills")
        );
        let amp = get("amp").unwrap();
        assert_eq!(
            resolve_global_dir(amp, home).unwrap(),
            PathBuf::from("/tmp/skl-home/.config/agents/skills")
        );
        assert!(resolve_global_dir(get("eve").unwrap(), home).is_none());
    }

    #[test]
    fn unique_global_roots_includes_ensured_and_non_trio() {
        let home = Path::new("/tmp/skl-home");
        let roots = unique_global_roots(home);
        let sources: Vec<_> = roots.iter().map(|r| r.source).collect();
        assert!(sources.contains(&"agents"));
        assert!(sources.contains(&"xdg-agents"));
        assert!(sources.contains(&"claude-code"));
        assert!(sources.contains(&"cursor"));
        assert!(sources.contains(&"codex"));
        assert!(
            sources.contains(&"windsurf"),
            "non-trio catalog global missing: {sources:?}"
        );
        assert!(
            sources.contains(&"goose"),
            "xdg catalog global missing: {sources:?}"
        );
        assert!(!sources.contains(&"eve"));
        assert_eq!(
            roots.iter().find(|r| r.source == "agents").unwrap().path,
            home.join(".agents").join("skills")
        );
        assert_eq!(
            roots
                .iter()
                .find(|r| r.source == "xdg-agents")
                .unwrap()
                .path,
            home.join(".config").join("agents").join("skills")
        );
        let paths: Vec<_> = roots.iter().map(|r| r.path.clone()).collect();
        let mut dedup = paths.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(paths.len(), dedup.len(), "global roots must be unique");
        assert!(roots.len() > 5);
    }

    #[test]
    fn soft_prompt_never_includes_cursor_or_codex_and_stays_small() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        for rel in [
            ".cursor/skills",
            ".codex/skills",
            ".claude/skills",
            ".windsurf/skills",
            ".codeium/windsurf/skills",
            ".goose/skills",
            ".config/goose/skills",
        ] {
            std::fs::create_dir_all(home.join(rel)).unwrap();
        }
        let candidates = soft_prompt_candidates(home, &[]);
        assert!(candidates.contains(&CLAUDE_CODE_ID));
        assert!(!candidates
            .iter()
            .any(|id| *id == "cursor" || *id == "codex"));
        assert!(
            candidates.len() <= SOFT_PROMPT_CAP,
            "soft-prompt dumped {} ids",
            candidates.len()
        );
        let with_sticky = soft_prompt_candidates(home, &["claude-code".into()]);
        assert!(!with_sticky.contains(&CLAUDE_CODE_ID));
        assert!(!with_sticky.iter().any(|id| *id == "cursor"));
    }
}
