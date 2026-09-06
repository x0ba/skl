//! Canonical personal skill library (`{data_dir}/skills`).
//!
//! Roles: the library is canonical; `skl init` / `skl capture` import foreign
//! trees into it; `skl use` / `skl use --all` project from it. `skl sync` is
//! content-addressed against this library only — never agent homes
//! (`~/.agents/skills`, `~/.claude/skills`, …) or project dests.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{Paths, SkillRoot};
use crate::error::Result;
use crate::local::db::LocalDb;
use crate::local::linker;
use crate::local::skills::{self, DiscoveredSkill};

/// Indexed `source` for skills stored in the personal library.
pub const LIBRARY_SOURCE: &str = "agents";

/// Scan `{data_dir}/skills` only.
pub fn discover(paths: &Paths) -> Result<Vec<DiscoveredSkill>> {
    discover_dir(&paths.library_dir())
}

pub fn discover_dir(library_dir: &Path) -> Result<Vec<DiscoveredSkill>> {
    if !library_dir.is_dir() {
        return Ok(Vec::new());
    }
    skills::discover_from_roots(&[SkillRoot {
        source: LIBRARY_SOURCE,
        path: library_dir.to_path_buf(),
    }])
}

/// Sync pull destination: the personal library. Never a harness home.
pub fn default_pull_root(paths: &Paths) -> PathBuf {
    paths.library_dir()
}

pub fn is_under_library(path: &Path, library_dir: &Path) -> bool {
    let lib = fs::canonicalize(library_dir).unwrap_or_else(|_| library_dir.to_path_buf());
    if let Ok(canon) = fs::canonicalize(path) {
        return canon.starts_with(&lib);
    }
    path.starts_with(library_dir) || path.starts_with(&lib)
}

/// True when `link` is a symlink that resolves to `target`.
pub fn resolves_to(link: &Path, target: &Path) -> bool {
    let Ok(current) = fs::read_link(link) else {
        return false;
    };
    if current == target {
        return true;
    }
    let resolved = if current.is_absolute() {
        fs::canonicalize(current).ok()
    } else {
        link.parent()
            .and_then(|parent| fs::canonicalize(parent.join(current)).ok())
    };
    match (resolved, fs::canonicalize(target).ok()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Copy foreign skills that are not already in the library. Never overwrites.
/// Leaves the foreign tree in place (importer, not a projection).
pub fn import_foreign(foreign: &[DiscoveredSkill], library_dir: &Path) -> Result<usize> {
    fs::create_dir_all(library_dir)?;
    let mut seen = BTreeSet::new();
    let mut copied = 0;
    for skill in foreign {
        if !seen.insert(skill.name.clone()) {
            continue;
        }
        if !skill.path.is_dir() {
            continue;
        }
        let dest = library_dir.join(&skill.name);
        if dest.join("SKILL.md").is_file() {
            continue;
        }
        if is_under_library(&skill.path, library_dir) {
            continue;
        }
        if resolves_to(&skill.path, &dest) {
            continue;
        }
        linker::copy_skill_tree(&skill.path, &dest)?;
        copied += 1;
    }
    Ok(copied)
}

/// Heal an old index that still points at harness homes, then re-index
/// the personal library only. Does not scan project dests or agent homes
/// as sync peers.
pub fn reindex_library_only(db: &LocalDb, paths: &Paths) -> Result<()> {
    let library_dir = paths.library_dir();
    fs::create_dir_all(&library_dir)?;
    let indexed = db.list_skills()?;
    let foreign: Vec<_> = indexed
        .into_iter()
        .filter(|skill| !is_under_library(&skill.path, &library_dir))
        .collect();
    if !foreign.is_empty() {
        import_foreign(&foreign, &library_dir)?;
    }
    let discovered = discover(paths)?;
    db.replace_import(&discovered)?;
    Ok(())
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

    #[test]
    fn pull_root_is_library_not_home_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home = tmp.path().join("home");
        fs::create_dir_all(home.join(".agents/skills")).unwrap();
        fs::create_dir_all(home.join(".claude/skills")).unwrap();
        let pull = default_pull_root(&paths);
        assert_eq!(pull, paths.library_dir());
        let rendered = pull.to_string_lossy();
        assert!(
            !rendered.contains(".agents/skills"),
            "sync must not pull into ~/.agents/skills: {rendered}"
        );
        assert!(
            !rendered.contains(".claude/skills"),
            "sync must not pull into ~/.claude/skills: {rendered}"
        );
    }

    #[test]
    fn import_copies_foreign_into_library_and_leaves_source() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let foreign_dir = tmp.path().join("home/.claude/skills/greeter");
        plant(&foreign_dir, "# hello\n");
        let foreign = skills::discover_from_roots(&[SkillRoot {
            source: "claude-code",
            path: foreign_dir.parent().unwrap().to_path_buf(),
        }])
        .unwrap();
        assert_eq!(import_foreign(&foreign, &paths.library_dir()).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# hello\n"
        );
        assert_eq!(
            fs::read_to_string(foreign_dir.join("SKILL.md")).unwrap(),
            "# hello\n"
        );
        assert_eq!(import_foreign(&foreign, &paths.library_dir()).unwrap(), 0);
    }

    #[test]
    fn reindex_migrates_harness_path_then_indexes_library_only() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = isolated_paths(tmp.path());
        let home_skill = tmp.path().join("home/.agents/skills/greeter");
        plant(&home_skill, "# from home\n");
        paths.ensure().unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "agents".into(),
            path: home_skill.clone(),
            tree: skills::hash_skill_dir(&home_skill).unwrap(),
        }])
        .unwrap();

        reindex_library_only(&db, &paths).unwrap();
        let listed = db.list_skills().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, paths.library_skill("greeter"));
        assert!(is_under_library(&listed[0].path, &paths.library_dir()));
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "# from home\n"
        );
        assert_eq!(
            fs::read_to_string(home_skill.join("SKILL.md")).unwrap(),
            "# from home\n"
        );
    }
}
