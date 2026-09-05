//! Interactive extras checklist modeled on skills.sh `search-multiselect`.
//!
//! Locked section: Universal (`.agents/skills`) — always selected, not toggleable.
//! Toggleable: detected custom-project agents + always-offered `claude-code`.
//! Never cursor/codex. ↑↓ move, space toggle, enter confirm.

use std::io::{self, Write};

use dialoguer::{theme::ColorfulTheme, MultiSelect};

use crate::catalog::{self, AgentEntry};
use crate::error::{Result, SklError};

/// Well-known universal readers shown in the locked preview (not the full ~20).
const LOCKED_PREVIEW_IDS: &[&str] = &["cursor", "codex", "amp", "gemini-cli", "github-copilot"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub id: String,
    pub label: String,
}

/// Rendered locked Universal block (always included, not interactive).
pub fn locked_universal_lines() -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("Universal (.agents/skills)  ── always included".into());
    let mut shown = 0;
    for id in LOCKED_PREVIEW_IDS {
        if let Some(entry) = catalog::get(id) {
            if entry.project_skills_dir == catalog::UNIVERSAL_PROJECT_DIR {
                lines.push(format!("  ✓  {}", display_name(entry)));
                shown += 1;
            }
        }
    }
    let total_universal = catalog::agents()
        .iter()
        .filter(|e| e.project_skills_dir == catalog::UNIVERSAL_PROJECT_DIR)
        .count();
    if total_universal > shown {
        lines.push(format!("  …and {} more", total_universal - shown));
    }
    lines
}

pub fn on_screen_hint() -> &'static str {
    "↑↓ move, space toggle, enter confirm"
}

pub fn checklist_item_label(id: &str) -> String {
    match catalog::get(id) {
        Some(entry) => format!("{}  → {}", display_name(entry), entry.project_skills_dir),
        None => id.to_string(),
    }
}

fn display_name(entry: &AgentEntry) -> String {
    entry.name.unwrap_or(entry.id).to_string()
}

pub fn items_for_ids(ids: &[&str]) -> Vec<ChecklistItem> {
    ids.iter()
        .map(|id| ChecklistItem {
            id: (*id).to_string(),
            label: checklist_item_label(id),
        })
        .collect()
}

/// Print locked Universal section + MultiSelect for custom extras.
/// Returns selected custom ids (never includes cursor/codex/universal).
pub fn run_extras_checklist(candidates: &[&str]) -> Result<Vec<String>> {
    let mut stderr = io::stderr();
    writeln!(stderr)?;
    writeln!(stderr, "Which extra project dests should `skl use` link?")?;
    for line in locked_universal_lines() {
        writeln!(stderr, "  {line}")?;
    }
    writeln!(stderr)?;
    writeln!(stderr, "  Additional agents")?;
    writeln!(stderr, "  {}", on_screen_hint())?;
    stderr.flush()?;

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let items = items_for_ids(candidates);
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    let theme = ColorfulTheme::default();
    let selected = MultiSelect::with_theme(&theme)
        .with_prompt("Toggle extra dests")
        .items(&labels)
        .interact()
        .map_err(|err| SklError::Config(format!("checklist: {err}")))?;

    Ok(selected
        .into_iter()
        .filter_map(|idx| items.get(idx).map(|item| item.id.clone()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_section_is_universal_and_not_cursor_codex_toggleable() {
        let text = locked_universal_lines().join("\n");
        assert!(text.contains("Universal (.agents/skills)"));
        assert!(text.contains("always included"));
        assert!(text.contains("Cursor") || text.contains("cursor"));
        assert!(text.contains("…and"), "{text}");
        assert!(!text.contains("toggle"));
    }

    #[test]
    fn hint_documents_keys() {
        let hint = on_screen_hint();
        assert!(hint.contains("↑↓"));
        assert!(hint.contains("space"));
        assert!(hint.contains("enter"));
    }

    #[test]
    fn checklist_items_never_include_cursor_or_codex() {
        let ids = catalog::soft_prompt_candidates(std::path::Path::new("/tmp/missing-home"), &[]);
        assert!(ids.contains(&catalog::CLAUDE_CODE_ID));
        assert!(!ids.iter().any(|id| *id == "cursor" || *id == "codex"));
        let items = items_for_ids(&ids);
        assert!(items
            .iter()
            .all(|item| item.id != "cursor" && item.id != "codex"));
        assert!(items.iter().any(
            |item| item.id == catalog::CLAUDE_CODE_ID && item.label.contains(".claude/skills")
        ));
    }
}
