//! `skl capture` — promote a project-local skill into the personal library.
//!
//! Canonical library: `{Paths.data_dir}/skills/<name>/`
//! (default `~/.local/share/skl/skills/`, override with `SKL_DATA_DIR`).
//!
//! `.agents/skills` is the project link destination (and a home discovery
//! root for `skl init`). It is **not** the personal library.

use std::path::PathBuf;

use crate::config::Paths;
use crate::error::{Result, SklError};

#[derive(Debug, Clone, Default)]
pub struct CaptureOpts {
    pub force: bool,
    pub as_name: Option<String>,
    pub keep_copy: bool,
    pub project: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureAction {
    Captured,
    Forced,
    KeepCopy,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    pub name: String,
    pub source_path: PathBuf,
    pub library_path: PathBuf,
    pub action: CaptureAction,
}

pub async fn run(path: PathBuf, opts: CaptureOpts, api_base: &str) -> Result<()> {
    let paths = Paths::resolve()?;
    let out = capture(&path, &opts, &paths)?;
    eprintln!(
        "captured {}  ({})",
        out.name,
        out.library_path.display()
    );
    // Fail-soft: never fail `skl capture` because auto-sync failed.
    let _ = crate::auto_sync::maybe_run(api_base, &paths, "capture").await;
    Ok(())
}

/// Copy a project skill into `{data_dir}/skills/<name>/`.
///
/// WIP: resolution, clash, symlink replace, and index update land next.
pub fn capture(path: &std::path::Path, opts: &CaptureOpts, paths: &Paths) -> Result<CaptureOutcome> {
    let _ = (path, opts);
    let library = paths.library_dir();
    Err(SklError::LocalState(format!(
        "skl capture is a work in progress (library: {})",
        library.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_paths(tmp: &std::path::Path) -> Paths {
        let data_dir = tmp.join("data");
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            db_file: data_dir.join("state.db"),
            data_dir,
        }
    }

    #[test]
    fn capture_library_is_data_dir_skills_not_home_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        assert_eq!(paths.library_dir(), tmp.path().join("data/skills"));
        assert_eq!(
            paths.library_skill("demo"),
            tmp.path().join("data/skills/demo")
        );
        let rendered = paths.library_dir().to_string_lossy();
        assert!(
            !rendered.contains(".agents/skills"),
            "library must be under data_dir, not ~/.agents/skills: {rendered}"
        );
    }
}
