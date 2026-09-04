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

/// SHA-256 hex of file bytes.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Canonical tree hash: SHA-256 of sorted `"{path}\0{hash}\n"` lines.
/// Cipher should treat this as the local algorithm until a shared spec lands.
pub fn tree_hash(files: &BTreeMap<String, String>) -> String {
    let mut canonical = String::new();
    for (path, hash) in files {
        canonical.push_str(path);
        canonical.push('\0');
        canonical.push_str(hash);
        canonical.push('\n');
    }
    hash_bytes(canonical.as_bytes())
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
    fn discovers_claude_and_skips_missing_codex() {
        let home = tempfile::tempdir().unwrap();
        let claude = home.path().join(".claude/skills/foo");
        let cursor = home.path().join(".cursor/skills/bar");
        fs::create_dir_all(&claude).unwrap();
        fs::create_dir_all(&cursor).unwrap();
        fs::write(claude.join("SKILL.md"), "foo").unwrap();
        fs::write(cursor.join("SKILL.md"), "bar").unwrap();

        let found = discover_from_home(home.path()).unwrap();
        let names: Vec<_> = found.iter().map(|s| (s.source.as_str(), s.name.as_str())).collect();
        assert!(names.contains(&("claude", "foo")));
        assert!(names.contains(&("cursor", "bar")));
        assert!(!names.iter().any(|(src, _)| *src == "codex"));
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
