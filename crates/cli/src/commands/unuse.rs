//! `skl unuse` — remove project skill symlinks and drop `skills.toml` entries.

use std::path::PathBuf;

use crate::config;
use crate::error::{Result, SklError};
use crate::local::linker::{self, LinkAction};

use super::use_cmd::resolve_project;

pub fn run(names: &[String], project: Option<PathBuf>) -> Result<()> {
    if names.is_empty() {
        return Err(SklError::LocalState(
            "specify at least one skill: `skl unuse <skill>`".into(),
        ));
    }

    let project = resolve_project(project)?;
    let home = config::home_dir()?;

    for name in names {
        let out = linker::deactivate(&project, &home, name)?;
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
    Ok(())
}

fn action_label(action: LinkAction) -> &'static str {
    match action {
        LinkAction::Removed => "removed",
        LinkAction::Absent => "absent",
        LinkAction::Created => "symlink",
        LinkAction::Replaced => "replace",
        LinkAction::Unchanged => "ok",
    }
}
