//! Tree-hash conflict hook — hammer owns keep-local / keep-remote.
//!
//! This crate prints placeholders only. Do **not** auto-merge.

use crate::api::SyncConflict;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    /// TODO(hammer/conflict): apply local tree.
    #[allow(dead_code)]
    KeepLocal,
    /// TODO(hammer/conflict): apply remote tree.
    #[allow(dead_code)]
    KeepRemote,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictResolution {
    pub skill: String,
    pub choice: ConflictChoice,
}

/// Print conflict UX and return unresolved choices.
///
/// TODO(hammer/conflict): prompt or apply `keep-local` / `keep-remote` per
/// skill. Keep-local => PUT /v1/skills/:name/tree with the local tree.
/// Keep-remote => GET blobs + write remote files, refresh state.db.
/// Do not invent three-way or auto-merge.
pub fn resolve_conflicts(conflicts: &[SyncConflict]) -> Vec<ConflictResolution> {
    if conflicts.is_empty() {
        return Vec::new();
    }

    eprintln!();
    eprintln!("Conflicts (tree hashes differ). No auto-merge.");
    eprintln!("Placeholder actions (not applied this run):");
    eprintln!("  keep-local   push the local tree after resolving");
    eprintln!("  keep-remote  pull the remote tree and overwrite local files");
    eprintln!();

    for conflict in conflicts {
        eprintln!(
            "  skill {}  local={}  remote={}  remote_updated_at={}",
            conflict.skill,
            conflict.local_tree_hash,
            conflict.remote_tree_hash,
            conflict.remote_updated_at
        );
        eprintln!("    [ ] keep-local");
        eprintln!("    [ ] keep-remote");
        // TODO(hammer/conflict): resolve `{conflict.skill}` — call site only.
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

    #[test]
    fn leaves_conflicts_unresolved() {
        let conflicts = [SyncConflict {
            skill: "greeter".into(),
            local_tree_hash: "aaa".into(),
            remote_tree_hash: "bbb".into(),
            remote_updated_at: "2026-09-04T08:00:00.000Z".into(),
        }];
        let resolved = resolve_conflicts(&conflicts);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].choice, ConflictChoice::Unresolved);
        assert_eq!(resolved[0].skill, "greeter");
    }
}
