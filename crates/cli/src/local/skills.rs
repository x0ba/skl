use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::api::types::SkillTree;
use crate::config::{skill_roots, SkillRoot};
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub name: String,
    pub source: String,
    pub path: PathBuf,
    pub tree: SkillTree,
}

/// SHA-256 hex of raw bytes (lowercase), matching `apps/api/src/lib/hash.ts`.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize()).to_ascii_lowercase()
}

pub fn normalize_hash(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// Canonical tree hash from `apps/api/src/lib/tree.ts` / contracts.ts:
/// sort paths lex; join `${path}\0${hash}` with `\n` (NO trailing newline);
/// sha256 hex lowercase. Empty tree => sha256("").
pub fn tree_hash(files: &BTreeMap<String, String>) -> String {
    if files.is_empty() {
        return hash_bytes(b"");
    }
    let mut paths: Vec<&String> = files.keys().collect();
    paths.sort();
    let canonical = paths
        .into_iter()
        .map(|path| {
            let hash = files
                .get(path)
                .map(|h| normalize_hash(h))
                .unwrap_or_default();
            format!("{path}\0{hash}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    hash_bytes(canonical.as_bytes())
}

/// Same path rules as `apps/api/src/lib/tree.ts` `isSafeFilePath`.
pub fn is_safe_file_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 512 {
        return false;
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    path.split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn write_blob_file(skill_dir: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
    if !is_safe_file_path(rel) {
        return Err(crate::error::SklError::LocalState(format!(
            "refusing unsafe skill path `{rel}`"
        )));
    }
    let dest = skill_dir.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dest, bytes)?;
    Ok(())
}

pub fn default_pull_root(home: &Path) -> PathBuf {
    crate::catalog::agents_home_path(home)
}

pub fn hash_skill_dir(dir: &Path) -> Result<SkillTree> {
    let mut files = BTreeMap::new();
    if dir.is_dir() {
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                name != ".git" && name != ".DS_Store" && name != "node_modules"
            })
        {
            let entry = entry.map_err(|err| {
                crate::error::SklError::LocalState(format!("walk {}: {err}", dir.display()))
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(dir)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if rel.is_empty() || rel.ends_with(".DS_Store") {
                continue;
            }
            let bytes = fs::read(entry.path())?;
            files.insert(rel, hash_bytes(&bytes));
        }
    }
    let tree_hash = tree_hash(&files);
    Ok(SkillTree { tree_hash, files })
}

pub fn discover_from_home(home: &Path) -> Result<Vec<DiscoveredSkill>> {
    discover_from_roots(&skill_roots(home))
}

pub fn discover_from_roots(roots: &[SkillRoot]) -> Result<Vec<DiscoveredSkill>> {
    let mut out = Vec::new();
    for root in roots {
        if !root.path.is_dir() {
            continue;
        }
        let mut children: Vec<PathBuf> = fs::read_dir(&root.path)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        children.sort();
        for dir in children {
            let name = match dir.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            if name.starts_with('.') {
                continue;
            }
            let tree = hash_skill_dir(&dir)?;
            out.push(DiscoveredSkill {
                name,
                source: root.source.to_string(),
                path: dir,
                tree,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SkillRoot;

    #[test]
    fn hashes_skill_markdown() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path().join("demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\n# hi\n").unwrap();

        let tree = hash_skill_dir(&skill).unwrap();
        assert!(tree.files.contains_key("SKILL.md"));
        assert_eq!(tree.tree_hash, tree_hash(&tree.files));
        assert_eq!(
            tree.files["SKILL.md"],
            hash_bytes(b"---\nname: demo\n---\n# hi\n")
        );
    }

    #[test]
    fn tree_hash_matches_api_contract() {
        // apps/api/src/tree.test.ts
        assert_eq!(
            tree_hash(&BTreeMap::new()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut files = BTreeMap::new();
        files.insert("b.md".into(), "bb".into());
        files.insert("a.md".into(), "aa".into());
        assert_eq!(tree_hash(&files), hash_bytes(b"a.md\0aa\nb.md\0bb"));
        assert_ne!(tree_hash(&files), hash_bytes(b"a.md\0aa\nb.md\0bb\n"));
        assert!(hash_bytes(b"x").chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(is_safe_file_path("SKILL.md"));
        assert!(is_safe_file_path("scripts/run.sh"));
        assert!(!is_safe_file_path("../secret"));
        assert!(!is_safe_file_path("/etc/passwd"));
    }

    #[test]
    fn discovers_claude_and_skips_missing_codex() {
        let home = tempfile::tempdir().unwrap();
        let claude = home.path().join(".claude/skills/foo");
        let cursor = home.path().join(".cursor/skills/bar");
        fs::create_dir_all(&claude).unwrap();
        fs::create_dir_all(&cursor).unwrap();
        fs::write(claude.join("SKILL.md"), "foo").unwrap();
        fs::write(cursor.join("SKILL.md"), "bar").unwrap();

        let found = discover_from_home(home.path()).unwrap();
        let names: Vec<_> = found
            .iter()
            .map(|s| (s.source.as_str(), s.name.as_str()))
            .collect();
        assert!(names.contains(&("claude-code", "foo")));
        assert!(names.contains(&("cursor", "bar")));
        assert!(!names.iter().any(|(src, _)| *src == "codex"));
        assert!(!names.iter().any(|(src, _)| *src == "agents"));
        assert!(!names.iter().any(|(src, _)| *src == "xdg-agents"));
    }

    #[test]
    fn discovers_universal_home_agents_roots() {
        let home = tempfile::tempdir().unwrap();
        let agents = home.path().join(".agents/skills/greeter");
        let xdg = home.path().join(".config/agents/skills/notes");
        fs::create_dir_all(&agents).unwrap();
        fs::create_dir_all(&xdg).unwrap();
        fs::write(agents.join("SKILL.md"), "hi").unwrap();
        fs::write(xdg.join("SKILL.md"), "notes").unwrap();

        let found = discover_from_home(home.path()).unwrap();
        let names: Vec<_> = found
            .iter()
            .map(|s| (s.source.as_str(), s.name.as_str()))
            .collect();
        assert!(names.contains(&("agents", "greeter")));
        assert!(names.contains(&("xdg-agents", "notes")));
        assert!(!names.iter().any(|(src, _)| *src == "claude"));
    }

    #[test]
    fn imports_discovered_agents_skills() {
        let home = tempfile::tempdir().unwrap();
        let agents = home.path().join(".agents/skills/greeter");
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("SKILL.md"), "hi").unwrap();

        let found = discover_from_home(home.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, "agents");
        assert_eq!(found[0].name, "greeter");
        assert_eq!(found[0].path, agents);

        let db = crate::local::db::LocalDb::open(&home.path().join("state.db")).unwrap();
        db.replace_import(&found).unwrap();
        let listed = db.list_skills().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source, "agents");
        assert_eq!(listed[0].name, "greeter");
        assert_eq!(listed[0].path, agents);
    }

    #[test]
    fn default_pull_root_prefers_home_agents_skills() {
        let home = Path::new("/tmp/skl-home");
        assert_eq!(default_pull_root(home), home.join(".agents").join("skills"));
    }

    #[test]
    fn optional_codex_root() {
        let home = tempfile::tempdir().unwrap();
        let codex = home.path().join(".codex/skills/baz");
        fs::create_dir_all(&codex).unwrap();
        fs::write(codex.join("SKILL.md"), "baz").unwrap();
        let roots = [SkillRoot {
            source: "codex",
            path: home.path().join(".codex/skills"),
        }];
        let found = discover_from_roots(&roots).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "baz");
        assert_eq!(found[0].source, "codex");
    }
}
