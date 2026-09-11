//! Launch `$VISUAL` or `$EDITOR` (fallback `vi`) on a path.

use std::path::Path;
use std::process::Command;

use crate::error::{Result, SklError};

/// Resolve the user's editor: `$VISUAL`, then `$EDITOR`, then `vi`.
///
/// TUI / `skl create` use this fallback so a missing env does not block
/// creating a skill. [`required_editor`] is stricter (`skl edit`).
pub fn editor_command() -> String {
    required_editor().unwrap_or_else(|_| "vi".into())
}

/// `$VISUAL` or `$EDITOR`. Errors when both are unset (no `vi` fallback).
pub fn required_editor() -> Result<String> {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .map_err(|_| {
            SklError::LocalState(
                "$VISUAL and $EDITOR are unset; set one to open the library skill".into(),
            )
        })
}

/// Block until the editor exits. `editor` may include arguments (`code -w`).
pub fn open(path: &Path) -> Result<()> {
    open_with(&editor_command(), path)
}

/// Same as [`open`], but fails clearly when `$VISUAL` / `$EDITOR` are unset.
pub fn open_required(path: &Path) -> Result<()> {
    open_with(&required_editor()?, path)
}

fn open_with(editor: &str, path: &Path) -> Result<()> {
    let mut parts = editor.split_whitespace();
    let bin = parts.next().unwrap_or("vi");
    let mut cmd = Command::new(bin);
    for arg in parts {
        cmd.arg(arg);
    }
    cmd.arg(path);
    let status = cmd
        .status()
        .map_err(|err| SklError::LocalState(format!("spawn {bin}: {err}")))?;
    if !status.success() {
        return Err(SklError::LocalState(format!(
            "{bin} exited {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) struct IsolatedEditorEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    prev_visual: Option<std::ffi::OsString>,
    prev_editor: Option<std::ffi::OsString>,
}

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
impl IsolatedEditorEnv {
    pub(crate) fn enter() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let this = Self {
            _guard: guard,
            prev_visual: std::env::var_os("VISUAL"),
            prev_editor: std::env::var_os("EDITOR"),
        };
        std::env::remove_var("VISUAL");
        std::env::remove_var("EDITOR");
        this
    }
}

#[cfg(test)]
impl Drop for IsolatedEditorEnv {
    fn drop(&mut self) {
        match &self.prev_visual {
            Some(v) => std::env::set_var("VISUAL", v),
            None => std::env::remove_var("VISUAL"),
        }
        match &self.prev_editor {
            Some(v) => std::env::set_var("EDITOR", v),
            None => std::env::remove_var("EDITOR"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_visual_then_editor_then_vi() {
        let _iso = IsolatedEditorEnv::enter();
        assert_eq!(editor_command(), "vi");
        std::env::set_var("EDITOR", "nano");
        assert_eq!(editor_command(), "nano");
        std::env::set_var("VISUAL", "code -w");
        assert_eq!(editor_command(), "code -w");
    }

    #[test]
    fn required_editor_fails_when_unset() {
        let _iso = IsolatedEditorEnv::enter();
        let err = required_editor().unwrap_err().to_string();
        assert!(err.contains("$VISUAL"), "{err}");
        assert!(err.contains("$EDITOR"), "{err}");
        std::env::set_var("EDITOR", "nano");
        assert_eq!(required_editor().unwrap(), "nano");
        std::env::set_var("VISUAL", "code -w");
        assert_eq!(required_editor().unwrap(), "code -w");
    }
}
