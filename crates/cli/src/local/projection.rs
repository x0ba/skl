//! Warn-only projection checks (library → project).
//!
//! Doctor never auto-merges, auto-captures, or treats project dests as
//! sync peers.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Paths;
use crate::local::library;
use crate::local::linker;
use crate::local::skills;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionKind {
    DivergentCopy,
    DanglingLink,
    OrphanActivation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionWarning {
    pub kind: ProjectionKind,
    pub skill: String,
    pub dest: Option<PathBuf>,
    pub message: String,
}

/// Inspect a project’s projected skills against this machine’s library.
///
/// Only runs when `project` looks like a project (has `skills.toml` and/or
/// projected dests). Warn-only — never mutates.
pub fn inspect(project: &Path, home: &Path, paths: Option<&Paths>) -> Vec<ProjectionWarning> {
    let Ok(manifest) = linker::load_manifest(project) else {
        return Vec::new();
    };
    if manifest.skills.is_empty() && !linker::manifest_path(project).exists() {
        return Vec::new();
    }

    let dests = linker::destinations_for(project, home, &manifest.targets);
    let mut out = Vec::new();
    for entry in &manifest.skills {
        let library_skill = paths.map(|p| p.library_skill(&entry.name));
        let in_library = library_skill
            .as_ref()
            .map(|p| p.is_dir() && p.join("SKILL.md").is_file())
            .unwrap_or(false);

        if !in_library {
            out.push(ProjectionWarning {
                kind: ProjectionKind::OrphanActivation,
                skill: entry.name.clone(),
                dest: None,
                message: format!(
                    "{} is listed in skills.toml but missing from the personal library; run `skl sync` or remove it from the manifest",
                    entry.name
                ),
            });
        }

        for dest in &dests {
            let candidate = dest.path.join(&entry.name);
            let meta = match fs::symlink_metadata(&candidate) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                let ok = match library_skill.as_ref() {
                    Some(lib) if in_library => library::resolves_to(&candidate, lib),
                    _ => false,
                };
                if !ok {
                    out.push(ProjectionWarning {
                        kind: ProjectionKind::DanglingLink,
                        skill: entry.name.clone(),
                        dest: Some(candidate.clone()),
                        message: format!(
                            "{} symlink does not resolve to this machine's library skill; run `skl use --all`",
                            entry.name
                        ),
                    });
                }
                continue;
            }
            if meta.file_type().is_dir() && in_library {
                if let Some(lib) = library_skill.as_ref() {
                    if !same_tree(&candidate, lib) {
                        out.push(ProjectionWarning {
                            kind: ProjectionKind::DivergentCopy,
                            skill: entry.name.clone(),
                            dest: Some(candidate.clone()),
                            message: format!(
                                "{}: project owns a copy; not a sync peer. `skl capture` to import, or replace with `skl use`.",
                                entry.name
                            ),
                        });
                    }
                }
            }
        }
    }
    out
}

fn same_tree(a: &Path, b: &Path) -> bool {
    match (skills::hash_skill_dir(a), skills::hash_skill_dir(b)) {
        (Ok(left), Ok(right)) => left.tree_hash == right.tree_hash,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;

    fn isolated_paths(tmp: &Path) -> Paths {
        let data_dir = tmp.join("data");
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            db_file: data_dir.join("state.db"),
            data_dir,
        }
    }

    fn plant(dir: &Path, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    fn write_manifest(project: &Path, name: &str) {
        fs::create_dir_all(project).unwrap();
        fs::write(
            linker::manifest_path(project),
            format!(
                r#"
[[skills]]
name = "{name}"
mode = "symlink"
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn divergent_real_copy_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        plant(&paths.library_skill("greeter"), "# library\n");
        plant(
            &project.join(".agents/skills/greeter"),
            "# project copy\n",
        );
        write_manifest(&project, "greeter");

        let warns = inspect(&project, &home, Some(&paths));
        assert!(
            warns
                .iter()
                .any(|w| w.kind == ProjectionKind::DivergentCopy && w.skill == "greeter"),
            "{warns:?}"
        );
        assert!(
            warns
                .iter()
                .any(|w| w.message.contains("not a sync peer") && w.message.contains("skl capture")),
            "{warns:?}"
        );
    }

    #[test]
    fn matching_real_copy_is_not_divergent() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        plant(&paths.library_skill("greeter"), "# same\n");
        plant(&project.join(".agents/skills/greeter"), "# same\n");
        write_manifest(&project, "greeter");

        let warns = inspect(&project, &home, Some(&paths));
        assert!(
            !warns.iter().any(|w| w.kind == ProjectionKind::DivergentCopy),
            "{warns:?}"
        );
    }

    #[test]
    fn dangling_symlink_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        plant(&paths.library_skill("greeter"), "# library\n");
        let dest = project.join(".agents/skills/greeter");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let elsewhere = tmp.path().join("elsewhere/greeter");
        plant(&elsewhere, "# other\n");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, &dest).unwrap();
        write_manifest(&project, "greeter");

        let warns = inspect(&project, &home, Some(&paths));
        assert!(
            warns
                .iter()
                .any(|w| w.kind == ProjectionKind::DanglingLink
                    && w.message.contains("skl use --all")),
            "{warns:?}"
        );
    }

    #[test]
    fn library_link_is_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        let lib = paths.library_skill("greeter");
        plant(&lib, "# library\n");
        let dest = project.join(".agents/skills/greeter");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&lib, &dest).unwrap();
        write_manifest(&project, "greeter");

        let warns = inspect(&project, &home, Some(&paths));
        assert!(warns.is_empty(), "{warns:?}");
    }

    #[test]
    fn orphan_manifest_entry_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        write_manifest(&project, "ghost");

        let warns = inspect(&project, &home, Some(&paths));
        assert!(
            warns.iter().any(|w| w.kind == ProjectionKind::OrphanActivation
                && w.skill == "ghost"
                && w.message.contains("skl sync")
                && w.message.contains("manifest")),
            "{warns:?}"
        );
    }
}
