//! `skl unuse` — remove project skill symlinks and drop `skills.toml` entries.

use std::path::PathBuf;

use crate::config::{self, Paths};
use crate::error::{Result, SklError};
use crate::local::linker::{self, LinkAction};

use super::use_cmd::{resolve_activation_extras, resolve_project};

pub async fn run(names: &[String], project: Option<PathBuf>, api_base: &str) -> Result<()> {
    if names.is_empty() {
        return Err(SklError::LocalState(
            "specify at least one skill: `skl unuse <skill>`".into(),
        ));
    }

    let project = resolve_project(project)?;
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let extras = resolve_activation_extras(paths.as_ref(), &[])?;

    for name in names {
        let out = linker::deactivate_with_extras(&project, &home, name, &extras)?;
        eprintln!("unused {}", out.skill);
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
    // Fail-soft: never fail `skl unuse` because auto-sync failed.
    if let Some(paths) = paths.as_ref() {
        let _ = crate::auto_sync::maybe_run(api_base, paths, "unuse").await;
    }
    Ok(())
}

fn action_label(action: LinkAction) -> &'static str {
    match action {
        LinkAction::Removed => "removed",
        LinkAction::Absent => "absent",
        LinkAction::Created => "symlink",
        LinkAction::Copied => "copy",
        LinkAction::Replaced => "replace",
        LinkAction::CopyReplaced => "copy*",
        LinkAction::Unchanged => "ok",
    }
}
