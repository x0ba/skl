use std::io::{self, BufRead, IsTerminal, Write};

use thiserror::Error;

use crate::conflict::{Resolution, ResolvedConflict, SyncAction, SyncConflict};
use crate::skill_tree::format_mtime;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("conflict prompt failed: {0}")]
    Prompt(String),
    #[error("non-interactive sync needs --keep-local or --keep-remote")]
    NotInteractive,
    #[error("conflict resolution aborted")]
    Aborted,
}

/// Furnace: implement or reuse this to decide keep-local / keep-remote.
pub trait ConflictResolver {
    fn resolve(&mut self, conflict: &SyncConflict) -> Result<Resolution, ResolveError>;
}

pub struct PreferLocal;
pub struct PreferRemote;

impl ConflictResolver for PreferLocal {
    fn resolve(&mut self, _conflict: &SyncConflict) -> Result<Resolution, ResolveError> {
        Ok(Resolution::KeepLocal)
    }
}

impl ConflictResolver for PreferRemote {
    fn resolve(&mut self, _conflict: &SyncConflict) -> Result<Resolution, ResolveError> {
        Ok(Resolution::KeepRemote)
    }
}

/// Interactive stdin prompt. No full diff UI and no auto-merge.
/// Shows skill name + short hashes, local mtime if attached, and
/// wire `remote_updated_at`.
pub struct InteractiveResolver<R, W> {
    reader: R,
    writer: W,
}

impl<R: BufRead, W: Write> InteractiveResolver<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl InteractiveResolver<io::BufReader<io::Stdin>, io::Stderr> {
    pub fn stdio() -> Self {
        Self::new(io::BufReader::new(io::stdin()), io::stderr())
    }

    /// Furnace: use this when stdin is a TTY; otherwise require flags.
    pub fn try_stdio() -> Result<Self, ResolveError> {
        if io::stdin().is_terminal() {
            Ok(Self::stdio())
        } else {
            Err(ResolveError::NotInteractive)
        }
    }
}

impl<R: BufRead, W: Write> ConflictResolver for InteractiveResolver<R, W> {
    fn resolve(&mut self, conflict: &SyncConflict) -> Result<Resolution, ResolveError> {
        write_conflict_prompt(&mut self.writer, conflict)
            .map_err(|e| ResolveError::Prompt(e.to_string()))?;
        loop {
            self.writer
                .write_all(b"keep [l]ocal or [r]emote? ")
                .and_then(|_| self.writer.flush())
                .map_err(|e| ResolveError::Prompt(e.to_string()))?;

            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .map_err(|e| ResolveError::Prompt(e.to_string()))?;
            if n == 0 {
                return Err(ResolveError::Aborted);
            }
            match parse_resolution(line.trim()) {
                Some(res) => return Ok(res),
                None => {
                    writeln!(self.writer, "enter l / local or r / remote")
                        .map_err(|e| ResolveError::Prompt(e.to_string()))?;
                }
            }
        }
    }
}

/// Strip C0/C1 controls so a remote skill name cannot drive the terminal.
pub fn display_label(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

pub fn write_conflict_prompt(out: &mut impl Write, conflict: &SyncConflict) -> io::Result<()> {
    writeln!(out, "hash mismatch: {}", display_label(&conflict.skill))?;
    write!(out, "  local   {}", short_hash(&conflict.local_tree_hash))?;
    if conflict.local_mtime.is_some() {
        write!(out, "  {}", format_mtime(conflict.local_mtime))?;
    }
    writeln!(out)?;
    write!(out, "  remote  {}", short_hash(&conflict.remote_tree_hash))?;
    if !conflict.remote_updated_at.is_empty() {
        write!(out, "  {}", conflict.remote_updated_at)?;
    }
    writeln!(out)?;
    Ok(())
}

pub fn parse_resolution(input: &str) -> Option<Resolution> {
    match input.trim().to_ascii_lowercase().as_str() {
        "l" | "local" | "keep-local" | "1" => Some(Resolution::KeepLocal),
        "r" | "remote" | "keep-remote" | "2" => Some(Resolution::KeepRemote),
        _ => None,
    }
}

pub fn short_hash(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

pub fn resolve_conflicts<R: ConflictResolver>(
    conflicts: &[SyncConflict],
    resolver: &mut R,
) -> Result<Vec<ResolvedConflict>, ResolveError> {
    let mut out = Vec::with_capacity(conflicts.len());
    for conflict in conflicts {
        let resolution = resolver.resolve(conflict)?;
        out.push(ResolvedConflict {
            conflict: conflict.clone(),
            resolution,
            action: SyncAction::from(resolution),
        });
    }
    Ok(out)
}

pub fn write_resolution(out: &mut impl Write, resolved: &ResolvedConflict) -> io::Result<()> {
    let side = match resolved.resolution {
        Resolution::KeepLocal => "keep-local (overwrite remote, then re-PUT tree)",
        Resolution::KeepRemote => {
            "keep-remote (take remote; rename or skip local, then re-POST /v1/sync)"
        }
    };
    writeln!(
        out,
        "resolved: {} → {side}",
        display_label(&resolved.conflict.skill)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{Duration, UNIX_EPOCH};

    fn conflict() -> SyncConflict {
        let ts = UNIX_EPOCH + Duration::from_secs(1_704_067_200);
        SyncConflict::from_wire(
            "research",
            "abcdef0123456789",
            "ffffffffffffffff",
            "2024-06-15T12:00:00Z",
        )
        .with_local_mtime(Some(ts))
    }

    #[test]
    fn prompt_shows_skill_and_hashes_plus_local_mtime() {
        let mut buf = Vec::new();
        write_conflict_prompt(&mut buf, &conflict()).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("hash mismatch: research"));
        assert!(text.contains("abcdef01"));
        assert!(text.contains("ffffffff"));
        assert!(!text.contains("abcdef0123456789"));
        assert!(text.contains("local"));
        assert!(text.contains("2024-01-01") || text.contains("T"));
        assert!(text.contains("2024-06-15T12:00:00Z"));
        assert!(!text.contains("<<<<<<<"));
        assert!(!text.contains("SKILL.md"));
    }

    #[test]
    fn prompt_omits_mtime_when_absent() {
        let c = SyncConflict::from_wire("demo", "aa", "bb", "");
        let mut buf = Vec::new();
        write_conflict_prompt(&mut buf, &c).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("demo"));
        assert!(!text.contains("local mtime"));
    }

    #[test]
    fn interactive_accepts_local() {
        let input = Cursor::new("local\n");
        let mut out = Vec::new();
        let mut prompt = InteractiveResolver::new(input, &mut out);
        assert_eq!(prompt.resolve(&conflict()).unwrap(), Resolution::KeepLocal);
    }

    #[test]
    fn interactive_reprompts_then_remote() {
        let input = Cursor::new("nope\nr\n");
        let mut out = Vec::new();
        let mut prompt = InteractiveResolver::new(input, &mut out);
        assert_eq!(prompt.resolve(&conflict()).unwrap(), Resolution::KeepRemote);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("enter l / local or r / remote"));
    }

    #[test]
    fn resolve_all_with_prefer_local() {
        let items = vec![conflict()];
        let resolved = resolve_conflicts(&items, &mut PreferLocal).unwrap();
        assert_eq!(resolved[0].resolution, Resolution::KeepLocal);
        assert_eq!(resolved[0].action, SyncAction::OverwriteRemote);
    }

    #[test]
    fn prompt_strips_terminal_controls_from_skill_name() {
        let c = SyncConflict::from_wire("bad\u{1b}]0;x\u{07}name\r\n", "aa", "bb", "");
        let mut buf = Vec::new();
        write_conflict_prompt(&mut buf, &c).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{07}'));
        assert!(!text.contains('\r'));
        assert!(text.contains("hash mismatch:"));
    }
}
