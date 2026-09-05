//! Hammer contract tests for furnace #20 (`crates/cli/data/agents-catalog.json`).
//!
//! Consumes the vendored catalog only — does not invent product ids.
//! Green now: JSON shape, universal/custom partition, sample custom paths,
//! unique globals, soft-prompt candidate model, sticky alias rules,
//! non-TTY prompt skip.
//!
//! Behavioral CLI wiring (`catalog.rs`, `skill_roots`, `destinations_for`,
//! `skl use -a claude-code`) is still TODO on #20. Those cases live as
//! `#[ignore]` so this crate stays green until furnace lands them.

#![cfg(test)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{self, Paths};
use crate::local::linker::{self, ManifestTargets};
use crate::local::skills;

/// Furnace vendored catalog (not a hammer-invented product list).
const CATALOG_JSON: &str = include_str!("../data/agents-catalog.json");

const CANONICAL_PROJECT_DIR: &str = ".agents/skills";
const SOFT_PROMPT_CAP: usize = 50;
const BLOCKED_EXTRA_IDS: &[&str] = &["cursor", "codex"];
const CLAUDE_ALIAS: &str = "claude";
const CLAUDE_CODE: &str = "claude-code";
const CONTINUE_ID: &str = "continue";
const OPENCLAW_ID: &str = "openclaw";

#[derive(Debug, Deserialize)]
struct CatalogFile {
    agents: Vec<CatalogAgent>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogAgent {
    id: String,
    #[allow(dead_code)]
    name: String,
    project_skills_dir: String,
    #[serde(default)]
    global_skills_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SoftPromptRow {
    id: String,
    locked: bool,
    toggleable: bool,
}

fn load_catalog() -> CatalogFile {
    serde_json::from_str(CATALOG_JSON).expect("furnace agents-catalog.json must parse")
}

fn is_universal(agent: &CatalogAgent) -> bool {
    agent.project_skills_dir == CANONICAL_PROJECT_DIR
}

fn is_custom_project(agent: &CatalogAgent) -> bool {
    !is_universal(agent)
}

fn is_blocked_extra(id: &str) -> bool {
    BLOCKED_EXTRA_IDS
        .iter()
        .any(|blocked| id.eq_ignore_ascii_case(blocked))
}

fn agent<'a>(catalog: &'a CatalogFile, id: &str) -> &'a CatalogAgent {
    catalog
        .agents
        .iter()
        .find(|agent| agent.id == id)
        .unwrap_or_else(|| panic!("furnace catalog missing `{id}`"))
}

fn expand_global(template: &str, home: &Path) -> PathBuf {
    let xdg = home.join(".config");
    let replaced = template
        .replace("{home}", &home.to_string_lossy())
        .replace("{xdg_config}", &xdg.to_string_lossy());
    PathBuf::from(replaced)
}

/// Unique catalog globals, then ensure `~/.agents/skills` + `~/.config/agents/skills`.
fn unique_global_roots(catalog: &CatalogFile, home: &Path) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for agent in &catalog.agents {
        let Some(template) = agent.global_skills_dir.as_deref() else {
            continue;
        };
        let path = expand_global(template, home);
        if seen.insert(path.clone()) {
            out.push(path);
        }
    }
    for fallback in [
        home.join(".agents").join("skills"),
        home.join(".config").join("agents").join("skills"),
    ] {
        if seen.insert(fallback.clone()) {
            out.push(fallback);
        }
    }
    out
}

/// Soft-prompt model: locked Universal row + toggleable catalog-custom ids only.
fn soft_prompt_rows(catalog: &CatalogFile, sticky: &[String]) -> Vec<SoftPromptRow> {
    let mut rows = vec![SoftPromptRow {
        id: linker::CANONICAL_TARGET_ID.to_string(),
        locked: true,
        toggleable: false,
    }];
    let mut customs: Vec<String> = catalog
        .agents
        .iter()
        .filter(|agent| is_custom_project(agent) && !is_blocked_extra(&agent.id))
        .map(|agent| agent.id.clone())
        .collect();
    if !sticky.iter().any(|id| id == CLAUDE_CODE)
        && !customs.iter().any(|id| id == CLAUDE_CODE)
    {
        customs.insert(0, CLAUDE_CODE.to_string());
    } else if let Some(idx) = customs.iter().position(|id| id == CLAUDE_CODE) {
        if !sticky.iter().any(|id| id == CLAUDE_CODE) {
            let id = customs.remove(idx);
            customs.insert(0, id);
        }
    }
    customs.retain(|id| !sticky.iter().any(|s| s == id));
    customs.truncate(SOFT_PROMPT_CAP);
    rows.extend(customs.into_iter().map(|id| SoftPromptRow {
        id,
        locked: false,
        toggleable: true,
    }));
    rows
}

fn extra_ids_allowed(catalog: &CatalogFile, requested: &[String]) -> Vec<String> {
    let custom: BTreeSet<&str> = catalog
        .agents
        .iter()
        .filter(|agent| is_custom_project(agent))
        .map(|agent| agent.id.as_str())
        .collect();
    let mut out = Vec::new();
    for raw in requested {
        let id = raw.trim();
        if id.is_empty() || linker::is_canonical_id(id) || is_blocked_extra(id) {
            continue;
        }
        if custom.contains(id) && !out.iter().any(|existing: &String| existing == id) {
            out.push(id.to_string());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StickyMigration {
    extras: Vec<String>,
    warnings: Vec<String>,
}

fn migrate_sticky_extras(catalog: &CatalogFile, extras: &[String]) -> StickyMigration {
    let custom: BTreeSet<&str> = catalog
        .agents
        .iter()
        .filter(|agent| is_custom_project(agent))
        .map(|agent| agent.id.as_str())
        .collect();
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for raw in extras {
        let id = raw.trim();
        if id.eq_ignore_ascii_case(CLAUDE_ALIAS) {
            if !out.iter().any(|existing: &String| existing == CLAUDE_CODE) {
                out.push(CLAUDE_CODE.to_string());
            }
            warnings.push(format!("{CLAUDE_ALIAS} → {CLAUDE_CODE}"));
            continue;
        }
        if is_blocked_extra(id) {
            warnings.push(format!("dropping extra `{id}` (covered by .agents/skills)"));
            continue;
        }
        if custom.contains(id) && !out.iter().any(|existing: &String| existing == id) {
            out.push(id.to_string());
        }
    }
    StickyMigration {
        extras: out,
        warnings,
    }
}

fn isolated_paths(tmp: &Path) -> Paths {
    Paths {
        config_dir: tmp.join("cfg"),
        config_file: tmp.join("cfg/config.toml"),
        data_dir: tmp.join("data"),
        db_file: tmp.join("data/state.db"),
    }
}

#[test]
fn catalog_json_is_furnace_vendored_shape() {
    let catalog = load_catalog();
    assert!(
        catalog.agents.len() >= 70,
        "expected ~77 vendored agents, got {}",
        catalog.agents.len()
    );
    for agent in &catalog.agents {
        assert!(!agent.id.is_empty(), "agent id must be non-empty");
        assert!(
            !agent.project_skills_dir.is_empty(),
            "{} missing project_skills_dir",
            agent.id
        );
        assert!(
            !agent.project_skills_dir.starts_with('/'),
            "{} project_skills_dir must be project-relative",
            agent.id
        );
        if let Some(global) = &agent.global_skills_dir {
            assert!(
                global.contains("{home}") || global.contains("{xdg_config}"),
                "{} global_skills_dir should use {{home}}/{{xdg_config}}: {global}",
                agent.id
            );
        }
    }
}

#[test]
fn partition_universal_vs_custom_from_furnace_catalog() {
    let catalog = load_catalog();
    let cursor = agent(&catalog, "cursor");
    let codex = agent(&catalog, "codex");
    assert!(
        is_universal(cursor),
        "cursor project_skills_dir must be {CANONICAL_PROJECT_DIR}, got {}",
        cursor.project_skills_dir
    );
    assert!(
        is_universal(codex),
        "codex project_skills_dir must be {CANONICAL_PROJECT_DIR}, got {}",
        codex.project_skills_dir
    );
    assert!(!is_custom_project(cursor));
    assert!(!is_custom_project(codex));

    let claude_code = agent(&catalog, CLAUDE_CODE);
    let cont = agent(&catalog, CONTINUE_ID);
    let openclaw = agent(&catalog, OPENCLAW_ID);
    assert!(is_custom_project(claude_code), "claude-code is catalog-custom");
    assert!(is_custom_project(cont), "continue is catalog-custom");
    assert!(is_custom_project(openclaw), "openclaw is catalog-custom");

    assert!(
        extra_ids_allowed(&catalog, &["cursor".into(), "codex".into()]).is_empty(),
        "cursor/codex must never be project extras"
    );
}

#[test]
fn sample_custom_paths_match_furnace_catalog() {
    let catalog = load_catalog();
    let claude_code = agent(&catalog, CLAUDE_CODE);
    assert_eq!(claude_code.project_skills_dir, ".claude/skills");
    assert_eq!(
        claude_code.global_skills_dir.as_deref(),
        Some("{home}/.claude/skills")
    );

    let cont = agent(&catalog, CONTINUE_ID);
    assert_eq!(cont.project_skills_dir, ".continue/skills");
    assert_eq!(
        cont.global_skills_dir.as_deref(),
        Some("{home}/.continue/skills")
    );

    let openclaw = agent(&catalog, OPENCLAW_ID);
    assert_eq!(openclaw.project_skills_dir, "skills");
    assert_eq!(
        openclaw.global_skills_dir.as_deref(),
        Some("{home}/.openclaw/skills")
    );
}

#[test]
fn unique_global_roots_include_catalog_globals_plus_home_fallbacks() {
    let catalog = load_catalog();
    let home = Path::new("/tmp/skl-home");
    let roots = unique_global_roots(&catalog, home);

    let mut seen = BTreeSet::new();
    for path in &roots {
        assert!(
            seen.insert(path.clone()),
            "duplicate skill root {}",
            path.display()
        );
    }

    assert!(roots.contains(&home.join(".claude").join("skills")));
    assert!(roots.contains(&home.join(".continue").join("skills")));
    assert!(roots.contains(&home.join(".openclaw").join("skills")));
    assert!(roots.contains(&home.join(".agents").join("skills")));
    assert!(roots.contains(&home.join(".config").join("agents").join("skills")));
    assert!(roots.contains(&home.join(".cursor").join("skills")));
    assert!(roots.contains(&home.join(".codex").join("skills")));
}

#[test]
fn soft_prompt_is_locked_universal_plus_custom_toggles_only() {
    let catalog = load_catalog();
    let rows = soft_prompt_rows(&catalog, &[]);

    assert_eq!(rows[0].id, linker::CANONICAL_TARGET_ID);
    assert!(rows[0].locked, "Universal row must be locked");
    assert!(!rows[0].toggleable, "Universal row must not toggle");

    let toggleable: Vec<&str> = rows
        .iter()
        .skip(1)
        .map(|row| {
            assert!(!row.locked, "{} must not be locked", row.id);
            assert!(row.toggleable, "{} must be toggleable", row.id);
            row.id.as_str()
        })
        .collect();

    assert!(
        toggleable.contains(&CLAUDE_CODE),
        "soft-prompt must always offer claude-code when not sticky"
    );
    assert!(!toggleable.contains(&"cursor"));
    assert!(!toggleable.contains(&"codex"));
    assert!(
        !toggleable.iter().any(|id| {
            catalog
                .agents
                .iter()
                .any(|agent| agent.id == *id && is_universal(agent))
        }),
        "universal catalog ids must not appear as toggles"
    );
    assert!(
        toggleable.len() <= SOFT_PROMPT_CAP,
        "soft-prompt must not dump 50+ customs (got {})",
        toggleable.len()
    );
    assert!(
        extra_ids_allowed(&catalog, &["cursor".into(), "codex".into(), CLAUDE_CODE.into()])
            == [CLAUDE_CODE],
        "-a extras: only catalog-custom ids"
    );
}

#[test]
fn soft_prompt_never_dumps_fifty_plus_even_when_catalog_is_crowded() {
    let catalog = load_catalog();
    let custom_count = catalog
        .agents
        .iter()
        .filter(|agent| is_custom_project(agent) && !is_blocked_extra(&agent.id))
        .count();
    assert!(
        custom_count > SOFT_PROMPT_CAP,
        "precondition: furnace catalog has {custom_count} customs; cap test needs > {SOFT_PROMPT_CAP}"
    );
    let rows = soft_prompt_rows(&catalog, &[]);
    let toggleable = rows.iter().filter(|row| row.toggleable).count();
    assert!(toggleable <= SOFT_PROMPT_CAP, "got {toggleable}");
    assert!(rows.iter().any(|row| row.id == CLAUDE_CODE && row.toggleable));
}

#[test]
fn sticky_claude_migrates_to_claude_code_and_drops_cursor_codex() {
    let catalog = load_catalog();
    let migrated = migrate_sticky_extras(
        &catalog,
        &["claude".into(), "cursor".into(), "codex".into()],
    );
    assert_eq!(migrated.extras, [CLAUDE_CODE]);
    assert!(
        migrated.warnings.iter().any(|w| w.contains(CLAUDE_ALIAS)
            && w.contains(CLAUDE_CODE)),
        "expected claude→claude-code warn, got {:?}",
        migrated.warnings
    );
    assert!(
        migrated.warnings.iter().any(|w| w.contains("cursor")),
        "expected cursor drop warn, got {:?}",
        migrated.warnings
    );
    assert!(
        migrated.warnings.iter().any(|w| w.contains("codex")),
        "expected codex drop warn, got {:?}",
        migrated.warnings
    );
}

#[test]
fn destinations_for_contract_always_agents_extras_only_custom() {
    let catalog = load_catalog();
    let extras = extra_ids_allowed(
        &catalog,
        &[
            "agents".into(),
            "cursor".into(),
            "codex".into(),
            CLAUDE_CODE.into(),
            CONTINUE_ID.into(),
        ],
    );
    assert_eq!(extras, [CLAUDE_CODE, CONTINUE_ID]);
}

#[test]
fn new_files_prefer_home_agents_skills_in_root_list() {
    let catalog = load_catalog();
    let home = Path::new("/tmp/skl-home");
    let roots = unique_global_roots(&catalog, home);
    assert!(
        roots.iter().any(|p| p == &home.join(".agents").join("skills")),
        "skill_roots must include ~/.agents/skills"
    );
}

#[test]
fn maybe_prompt_skips_on_skl_no_prompt_without_hanging() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = isolated_paths(tmp.path());
    paths.ensure().unwrap();
    std::env::set_var("SKL_NO_PROMPT", "1");
    let cfg = config::maybe_prompt_sticky_extras(&paths).unwrap();
    std::env::remove_var("SKL_NO_PROMPT");
    assert!(cfg.targets.extra.is_empty());
    assert!(!cfg.targets.prompted);
}

#[test]
#[ignore = "furnace #20: skill_roots = unique catalog globals + home fallbacks"]
fn init_discovers_skill_under_openclaw_catalog_global() {
    let home = tempfile::tempdir().unwrap();
    let skill = home.path().join(".openclaw/skills/greeter");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "from openclaw\n").unwrap();

    let found = skills::discover_from_home(home.path()).unwrap();
    assert!(
        found.iter().any(|s| s.name == "greeter"
            && s.path.ends_with(".openclaw/skills/greeter")),
        "init must import the openclaw catalog global, got {found:?}"
    );
}

#[test]
#[ignore = "furnace #20: destinations_for extras are catalog-custom only"]
fn destinations_for_drops_cursor_codex_keeps_claude_code() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&project).unwrap();
    let dests = linker::destinations_for(
        &project,
        &home,
        &ManifestTargets {
            canonical: vec![linker::CANONICAL_TARGET_ID.into()],
            extra: vec![
                "cursor".into(),
                "codex".into(),
                CLAUDE_CODE.into(),
            ],
        },
    );
    let ids: Vec<_> = dests.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["agents", CLAUDE_CODE]);
    assert!(!project.join(".cursor").exists());
    assert!(!project.join(".codex").exists());
}

#[test]
#[ignore = "furnace #20: default_pull_root prefers ~/.agents/skills"]
fn default_pull_root_prefers_home_agents_over_claude() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::fs::create_dir_all(home.path().join(".agents/skills")).unwrap();
    assert_eq!(
        skills::default_pull_root(home.path()),
        home.path().join(".agents/skills")
    );
}
