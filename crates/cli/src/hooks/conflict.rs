//! Tree-hash conflict hook — keep-local / keep-remote only. No auto-merge.
//!
//! Wired from `crate::sync` after POST /v1/sync. The sync engine applies the
//! choice (PUT local tree, or GET remote + write files) then re-POSTs.

use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::api::SyncConflict;
use crate::conflict::SyncConflict as PromptConflict;
use crate::error::{Result, SklError};
use crate::prompt::{
    resolve_conflicts as prompt_resolve, write_conflict_prompt, InteractiveResolver, PreferLocal,
    PreferRemote, ResolveError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictMode {
    /// Interactive prompt; non-TTY requires `--keep-local` / `--keep-remote`.
    Prompt,
    KeepLocal,
    KeepRemote,
    /// Print placeholders and leave unresolved (legacy tests / dry run).
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    KeepLocal,
    KeepRemote,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictResolution {
    pub skill: String,
    pub choice: ConflictChoice,
}

/// Prompt or apply keep-local / keep-remote per skill.
pub fn resolve_conflicts(
    conflicts: &[SyncConflict],
    mode: ConflictMode,
    local_mtimes: &BTreeMap<String, Option<SystemTime>>,
) -> Result<Vec<ConflictResolution>> {
    if conflicts.is_empty() {
        return Ok(Vec::new());
    }

    let rows: Vec<PromptConflict> = conflicts
        .iter()
        .map(|c| {
            PromptConflict::from_wire(
                c.skill.clone(),
                c.local_tree_hash.clone(),
                c.remote_tree_hash.clone(),
                c.remote_updated_at.clone(),
            )
            .with_local_mtime(local_mtimes.get(&c.skill).copied().flatten())
        })
        .collect();

    match mode {
        ConflictMode::Defer => Ok(print_placeholders(conflicts)),
        ConflictMode::KeepLocal => map_prompted(&rows, &mut PreferLocal),
        ConflictMode::KeepRemote => map_prompted(&rows, &mut PreferRemote),
        ConflictMode::Prompt => match InteractiveResolver::try_stdio() {
            Ok(mut prompt) => map_prompted(&rows, &mut prompt),
            Err(ResolveError::NotInteractive) => {
                let mut stderr = std::io::stderr().lock();
                for row in &rows {
                    let _ = write_conflict_prompt(&mut stderr, row);
                }
                Err(SklError::Conflict(
                    "non-interactive sync needs --keep-local or --keep-remote".into(),
                ))
            }
            Err(err) => Err(err.into()),
        },
    }
}

fn map_prompted<R: crate::prompt::ConflictResolver>(
    rows: &[PromptConflict],
    resolver: &mut R,
) -> Result<Vec<ConflictResolution>> {
    let resolved = prompt_resolve(rows, resolver)?;
    Ok(resolved
        .into_iter()
        .map(|item| ConflictResolution {
            skill: item.conflict.skill,
            choice: match item.resolution {
                crate::conflict::Resolution::KeepLocal => ConflictChoice::KeepLocal,
                crate::conflict::Resolution::KeepRemote => ConflictChoice::KeepRemote,
            },
        })
        .collect())
}

fn print_placeholders(conflicts: &[SyncConflict]) -> Vec<ConflictResolution> {
    eprintln!();
    eprintln!("Conflicts (tree hashes differ). No auto-merge.");
    eprintln!("Pass --keep-local or --keep-remote, or run interactively.");
    eprintln!();
    for conflict in conflicts {
        eprintln!(
            "  skill {}  local={}  remote={}  remote_updated_at={}",
            conflict.skill,
            conflict.local_tree_hash,
            conflict.remote_tree_hash,
            conflict.remote_updated_at
        );
    }
    conflicts
        .iter()
        .map(|conflict| ConflictResolution {
            skill: conflict.skill.clone(),
            choice: ConflictChoice::Unresolved,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SyncConflict {
        SyncConflict {
            skill: "greeter".into(),
            local_tree_hash: "aaa".into(),
            remote_tree_hash: "bbb".into(),
            remote_updated_at: "2026-09-04T08:00:00.000Z".into(),
        }
    }

    #[test]
    fn defer_leaves_unresolved() {
        let resolved =
            resolve_conflicts(&[sample()], ConflictMode::Defer, &BTreeMap::new()).unwrap();
        assert_eq!(resolved[0].choice, ConflictChoice::Unresolved);
    }

    #[test]
    fn keep_local_mode_applies() {
        let resolved =
            resolve_conflicts(&[sample()], ConflictMode::KeepLocal, &BTreeMap::new()).unwrap();
        assert_eq!(resolved[0].choice, ConflictChoice::KeepLocal);
        assert_eq!(resolved[0].skill, "greeter");
    }

    #[test]
    fn keep_remote_mode_applies() {
        let resolved =
            resolve_conflicts(&[sample()], ConflictMode::KeepRemote, &BTreeMap::new()).unwrap();
        assert_eq!(resolved[0].choice, ConflictChoice::KeepRemote);
    }
}
