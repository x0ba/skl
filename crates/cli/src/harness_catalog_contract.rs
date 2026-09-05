//! Hammer coverage stacked on furnace #20 catalog + checklist UX.
//!
//! Consumes `crate::catalog` / `crate::checklist` (vendored JSON only).
//! Soft-prompt is an interactive MultiSelect: locked Universal row +
//! toggleable catalog-custom ids. Never cursor/codex as toggles.
//! Non-TTY / CI / `SKL_NO_PROMPT` / `SKL_YES` skip the UI (no hang).

#![cfg(test)]

use std::path::Path;

use crate::catalog::{self, CLAUDE_CODE_ID, SOFT_PROMPT_CAP, UNIVERSAL_PROJECT_DIR};
use crate::checklist;
use crate::config::{self, Paths};
use crate::local::linker::{self, ManifestTargets};
use crate::local::skills;

fn isolated_paths(tmp: &Path) -> Paths {
    Paths {
        config_dir: tmp.join("cfg"),
        config_file: tmp.join("cfg/config.toml"),
        data_dir: tmp.join("data"),
        db_file: tmp.join("data/state.db"),
    }
}

fn plant_skill(dir: &Path, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), body).unwrap();
}

/// Toggleable checklist rows from furnace candidates (never the locked Universal block).
fn toggle_items(home: &Path, sticky: &[String]) -> Vec<checklist::ChecklistItem> {
    let ids = catalog::soft_prompt_candidates(home, sticky);
    checklist::items_for_ids(&ids)
}

#[test]
fn non_tty_skips_checklist_ui_without_hanging() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = isolated_paths(tmp.path());
    paths.ensure().unwrap();
    std::env::set_var("SKL_NO_PROMPT", "1");
    let cfg = config::maybe_prompt_sticky_extras(&paths).unwrap();
    std::env::remove_var("SKL_NO_PROMPT");
    assert!(
        cfg.targets.extra.is_empty(),
        "non-TTY skip must not invent extras"
    );
    assert!(
        !cfg.targets.prompted,
        "skip must not mark prompted (user never saw the UI)"
    );
}

#[test]
fn yes_mode_skips_checklist_ui_without_hanging() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = isolated_paths(tmp.path());
    paths.ensure().unwrap();
    let prev = std::env::var_os("SKL_YES");
    std::env::set_var("SKL_YES", "1");
    let cfg = config::maybe_prompt_sticky_extras(&paths).unwrap();
    match prev {
        Some(value) => std::env::set_var("SKL_YES", value),
        None => std::env::remove_var("SKL_YES"),
    }
    assert!(cfg.targets.extra.is_empty());
    assert!(!cfg.targets.prompted);
}

#[test]
fn cursor_and_codex_never_appear_as_toggles() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    for rel in [
        ".cursor/skills",
        ".codex/skills",
        ".claude/skills",
        ".continue/skills",
        ".openclaw/skills",
        ".codeium/windsurf/skills",
    ] {
        std::fs::create_dir_all(home.join(rel)).unwrap();
    }

    let candidates = catalog::soft_prompt_candidates(home, &[]);
    assert!(
        !candidates.iter().any(|id| *id == "cursor" || *id == "codex"),
        "soft-prompt candidates leaked cursor/codex: {candidates:?}"
    );

    let items = checklist::items_for_ids(&candidates);
    assert!(
        items
            .iter()
            .all(|item| item.id != "cursor" && item.id != "codex"),
        "checklist toggles leaked cursor/codex: {items:?}"
    );
    for item in &items {
        assert!(
            !item.label.to_ascii_lowercase().contains("cursor")
                && !item.label.to_ascii_lowercase().contains("codex"),
            "toggle label must not name cursor/codex: {}",
            item.label
        );
    }
}

#[test]
fn universal_row_is_locked_only_catalog_customs_toggle() {
    let locked = checklist::locked_universal_lines().join("\n");
    assert!(locked.contains("Universal (.agents/skills)"));
    assert!(locked.contains("always included"));
    assert!(!locked.to_ascii_lowercase().contains("toggle"));

    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
    std::fs::create_dir_all(home.join(".continue/skills")).unwrap();
    let items = toggle_items(home, &[]);

    assert!(
        items.iter().all(|item| item.id != linker::CANONICAL_TARGET_ID),
        "Universal/agents must not be a toggle: {items:?}"
    );
    for item in &items {
        assert!(
            catalog::is_custom_project(&item.id),
            "toggle `{}` is not catalog-custom (project dir {:?})",
            item.id,
            catalog::get(&item.id).map(|e| e.project_skills_dir)
        );
        assert!(!catalog::is_universal(&item.id));
        assert_ne!(
            catalog::get(&item.id).unwrap().project_skills_dir,
            UNIVERSAL_PROJECT_DIR
        );
    }
    assert!(
        items.iter().any(|item| item.id == CLAUDE_CODE_ID),
        "claude-code must be offered when not sticky"
    );
    assert!(items.len() <= SOFT_PROMPT_CAP);
}

#[test]
fn partition_and_sample_customs_from_furnace_catalog() {
    assert!(catalog::is_universal("cursor"));
    assert!(catalog::is_universal("codex"));
    assert!(!catalog::is_custom_project("cursor"));
    assert!(!catalog::is_custom_project("codex"));

    let claude_code = catalog::get("claude-code").expect("vendored catalog");
    assert_eq!(claude_code.project_skills_dir, ".claude/skills");
    assert_eq!(
        claude_code.global_skills_dir,
        Some("{home}/.claude/skills")
    );
    assert!(catalog::is_custom_project("claude-code"));

    let cont = catalog::get("continue").expect("vendored catalog");
    assert_eq!(cont.project_skills_dir, ".continue/skills");
    assert_eq!(cont.global_skills_dir, Some("{home}/.continue/skills"));

    let openclaw = catalog::get("openclaw").expect("vendored catalog");
    assert_eq!(openclaw.project_skills_dir, "skills");
    assert_eq!(
        openclaw.global_skills_dir,
        Some("{home}/.openclaw/skills")
    );
}

#[test]
fn unique_globals_include_fallbacks_and_non_trio() {
    let home = Path::new("/tmp/skl-home");
    let roots = catalog::unique_global_roots(home);
    let sources: Vec<_> = roots.iter().map(|r| r.source).collect();
    assert!(sources.contains(&"agents"));
    assert!(sources.contains(&"xdg-agents"));
    assert!(sources.contains(&"claude-code"));
    assert!(sources.contains(&"continue"));
    assert!(sources.contains(&"openclaw"));
    assert_eq!(
        roots.iter().find(|r| r.source == "openclaw").unwrap().path,
        home.join(".openclaw").join("skills")
    );
}

#[test]
fn sticky_claude_migrates_to_claude_code_dropping_cursor_codex() {
    let (extras, warns) = linker::migrate_extra_ids(&[
        "claude".into(),
        "cursor".into(),
        "codex".into(),
    ]);
    assert_eq!(extras, ["claude-code"]);
    assert!(
        warns.iter().any(|w| w.contains("claude") && w.contains("claude-code")),
        "{warns:?}"
    );
    assert!(warns.iter().any(|w| w.contains("cursor")), "{warns:?}");
    assert!(warns.iter().any(|w| w.contains("codex")), "{warns:?}");
}

#[test]
fn destinations_for_always_agents_extras_only_custom() {
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
                "claude-code".into(),
                "continue".into(),
            ],
        },
    );
    let ids: Vec<_> = dests.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids[0], "agents");
    assert!(ids.contains(&"claude-code"));
    assert!(ids.contains(&"continue"));
    assert!(!ids.contains(&"cursor"));
    assert!(!ids.contains(&"codex"));
}

#[test]
fn init_discovers_skill_under_openclaw_catalog_global() {
    let home = tempfile::tempdir().unwrap();
    let skill = home.path().join(".openclaw/skills/greeter");
    plant_skill(&skill, "from openclaw\n");

    let found = skills::discover_from_home(home.path()).unwrap();
    assert!(
        found
            .iter()
            .any(|s| s.name == "greeter" && s.source == "openclaw"),
        "init must import the openclaw catalog global, got {found:?}"
    );
}

#[test]
fn default_pull_root_prefers_home_agents_over_claude() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::fs::create_dir_all(home.path().join(".agents/skills")).unwrap();
    assert_eq!(
        skills::default_pull_root(home.path()),
        home.path().join(".agents/skills")
    );
}

#[test]
fn use_alone_writes_only_agents_claude_code_adds_claude() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let skill_dir = home.join(".agents/skills/greeter");
    plant_skill(&skill_dir, "# greeter\n");
    let skill = skills::discover_from_home(&home)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "greeter")
        .unwrap();

    let alone = linker::activate(&project, &home, &skill).unwrap();
    assert_eq!(
        alone.links.iter().map(|l| l.agent.as_str()).collect::<Vec<_>>(),
        ["agents"]
    );
    assert!(project.join(".agents/skills/greeter").exists());
    assert!(!project.join(".claude").exists());

    let extra = linker::activate_with_extras(&project, &home, &skill, &["claude-code".into()])
        .unwrap();
    assert_eq!(
        extra
            .links
            .iter()
            .map(|l| l.agent.as_str())
            .collect::<Vec<_>>(),
        ["agents", "claude-code"]
    );
    assert!(project.join(".claude/skills/greeter").exists());
    assert!(!project.join(".cursor").exists());
    assert!(!project.join(".codex").exists());
}
