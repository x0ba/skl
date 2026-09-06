//! Launch `$VISUAL` or `$EDITOR` (fallback `vi`) on a path.

use std::path::Path;
use std::process::Command;

use crate::error::{Result, SklError};

/// Resolve the user's editor: `$VISUAL`, then `$EDITOR`, then `vi`.
pub fn editor_command() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into())
}

/// Block until the editor exits. `editor` may include arguments (`code -w`).
pub fn open(path: &Path) -> Result<()> {
    let editor = editor_command();
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
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct IsolatedEditorEnv {
        _guard: std::sync::MutexGuard<'static, ()>,
        prev_visual: Option<std::ffi::OsString>,
        prev_editor: Option<std::ffi::OsString>,
    }

    impl IsolatedEditorEnv {
        fn enter() -> Self {
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

    #[test]
    fn prefers_visual_then_editor_then_vi() {
        let _iso = IsolatedEditorEnv::enter();
        assert_eq!(editor_command(), "vi");
        std::env::set_var("EDITOR", "nano");
        assert_eq!(editor_command(), "nano");
        std::env::set_var("VISUAL", "code -w");
        assert_eq!(editor_command(), "code -w");
    }
}
