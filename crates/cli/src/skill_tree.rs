use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

const SKIP_DIR_NAMES: &[&str] = &[".git", "node_modules", "target", ".next", "dist"];

#[derive(Debug, Error)]
pub enum TreeError {
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    pub skill_name: String,
    pub relative_path: PathBuf,
    pub abs_path: PathBuf,
    pub hash: String,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillTree {
    pub root: PathBuf,
    pub files: Vec<SkillFile>,
}

impl SkillTree {
    pub fn skill_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.files.iter().map(|f| f.skill_name.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn has_skill(&self, skill: &str) -> bool {
        self.files.iter().any(|f| f.skill_name == skill)
    }

    pub fn files_for<'a>(&'a self, skill: &'a str) -> impl Iterator<Item = &'a SkillFile> + 'a {
        self.files.iter().filter(move |f| f.skill_name == skill)
    }

    /// Canonical tree hash for one skill.
    ///
    /// Matches cipher `contracts.ts`: sort paths as UTF-8, join
    /// `${path}\0${hash}` with `\n` between entries, SHA-256 hex.
    /// Empty tree is SHA-256 of the empty string.
    pub fn tree_hash(&self, skill: &str) -> String {
        hash_tree_entries(self.files_for(skill))
    }

    pub fn latest_mtime(&self, skill: &str) -> Option<SystemTime> {
        self.files_for(skill).filter_map(|f| f.modified).max()
    }
}

/// SHA-256 of the cipher canonical file map. Empty tree → SHA-256("").
pub fn hash_tree_entries<'a>(files: impl Iterator<Item = &'a SkillFile>) -> String {
    let rows: Vec<(String, String)> = files
        .map(|f| (slash_path(&f.relative_path), f.hash.clone()))
        .collect();
    tree_hash_from_pairs(rows)
}

/// Tree hash from a `files: { path: sha256 }` map (POST /v1/sync / PUT tree).
pub fn tree_hash_from_map(files: &std::collections::BTreeMap<String, String>) -> String {
    tree_hash_from_pairs(files.iter().map(|(p, h)| (p.clone(), h.clone())))
}

fn tree_hash_from_pairs(rows: impl IntoIterator<Item = (String, String)>) -> String {
    let mut rows: Vec<(String, String)> = rows.into_iter().collect();
    rows.sort();
    let mut canonical = String::new();
    for (i, (path, hash)) in rows.iter().enumerate() {
        if i > 0 {
            canonical.push('\n');
        }
        canonical.push_str(path);
        canonical.push('\0');
        canonical.push_str(hash);
    }
    hash_bytes(canonical.as_bytes())
}

/// SHA-256 hex of file bytes. Furnace should reuse this for local hashes
/// so they match conflict detection.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

/// Walk `root` for agent skills (directories that contain `SKILL.md`).
///
/// If no `SKILL.md` is present, the root itself is treated as one skill
/// named after the directory so callers can still scrub/compare a tree.
pub fn scan_tree(root: &Path) -> Result<SkillTree, TreeError> {
    let root = root.canonicalize().map_err(|source| TreeError::Io {
        path: root.to_path_buf(),
        source,
    })?;

    let skill_roots = find_skill_roots(&root)?;
    let mut files = Vec::new();

    for skill_root in skill_roots {
        let skill_name = skill_label(&root, &skill_root);
        collect_files(&skill_root, &skill_name, &mut files)?;
    }

    files.sort_by(|a, b| {
        a.skill_name
            .cmp(&b.skill_name)
            .then(slash_path(&a.relative_path).cmp(&slash_path(&b.relative_path)))
    });

    Ok(SkillTree { root, files })
}

fn find_skill_roots(root: &Path) -> Result<Vec<PathBuf>, TreeError> {
    let mut roots = Vec::new();

    if root.join("SKILL.md").is_file() {
        return Ok(vec![root.to_path_buf()]);
    }

    for entry in walk_filtered(root) {
        let entry = entry.map_err(|err| TreeError::Io {
            path: err.path().unwrap_or(root).to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::Other, err.to_string()),
        })?;
        if !entry.file_type().is_dir() {
            continue;
        }
        if entry.path().join("SKILL.md").is_file() {
            roots.push(entry.path().to_path_buf());
        }
    }

    if roots.is_empty() {
        roots.push(root.to_path_buf());
    }

    Ok(roots)
}

fn collect_files(
    skill_root: &Path,
    skill_name: &str,
    out: &mut Vec<SkillFile>,
) -> Result<(), TreeError> {
    if skill_root.is_file() {
        return Ok(());
    }

    for entry in walk_filtered(skill_root) {
        let entry = entry.map_err(|err| TreeError::Io {
            path: err.path().unwrap_or(skill_root).to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::Other, err.to_string()),
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let abs_path = entry.path().to_path_buf();
        let relative_path = abs_path
            .strip_prefix(skill_root)
            .unwrap_or(&abs_path)
            .to_path_buf();
        let bytes = fs::read(&abs_path).map_err(|source| TreeError::Io {
            path: abs_path.clone(),
            source,
        })?;
        let modified = fs::metadata(&abs_path).ok().and_then(|m| m.modified().ok());
        out.push(SkillFile {
            skill_name: skill_name.to_string(),
            relative_path,
            abs_path,
            hash: hash_bytes(&bytes),
            modified,
        });
    }

    Ok(())
}

fn walk_filtered(root: &Path) -> impl Iterator<Item = Result<walkdir::DirEntry, walkdir::Error>> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                e.file_name()
                    .to_str()
                    .map(|name| !SKIP_DIR_NAMES.contains(&name))
                    .unwrap_or(true)
            } else {
                true
            }
        })
}

fn skill_label(tree_root: &Path, skill_root: &Path) -> String {
    if skill_root == tree_root {
        return skill_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "skill".to_string());
    }
    skill_root
        .strip_prefix(tree_root)
        .map(slash_path)
        .unwrap_or_else(|_| {
            skill_root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "skill".to_string())
        })
}

pub fn slash_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn format_mtime(ts: Option<SystemTime>) -> String {
    match ts {
        Some(t) => humantime::format_rfc3339_seconds(t).to_string(),
        None => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn finds_nested_skills_and_hashes() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(&tmp.path().join("alpha/SKILL.md"), "# alpha\n");
        write_file(&tmp.path().join("alpha/notes.txt"), "hello\n");
        write_file(&tmp.path().join("beta/SKILL.md"), "# beta\n");
        write_file(&tmp.path().join("ignored/target/secret.txt"), "nope\n");

        let tree = scan_tree(tmp.path()).unwrap();
        let names: Vec<_> = tree.files.iter().map(|f| f.skill_name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(!tree
            .files
            .iter()
            .any(|f| slash_path(&f.relative_path).contains("target")));

        let notes = tree
            .files
            .iter()
            .find(|f| f.skill_name == "alpha" && slash_path(&f.relative_path) == "notes.txt")
            .unwrap();
        assert_eq!(notes.hash, hash_bytes(b"hello\n"));
    }

    #[test]
    fn root_without_skill_md_is_one_tree() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(&tmp.path().join("readme.md"), "hi\n");
        let tree = scan_tree(tmp.path()).unwrap();
        assert_eq!(tree.files.len(), 1);
        assert_eq!(slash_path(&tree.files[0].relative_path), "readme.md");
    }

    #[test]
    fn tree_hash_is_stable_for_same_bytes() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write_file(&a.path().join("alpha/SKILL.md"), "# same\n");
        write_file(&b.path().join("alpha/SKILL.md"), "# same\n");
        let ha = scan_tree(a.path()).unwrap().tree_hash("alpha");
        let hb = scan_tree(b.path()).unwrap().tree_hash("alpha");
        assert_eq!(ha, hb);
        assert_eq!(ha.len(), 64);
    }

    #[test]
    fn empty_tree_hash_is_sha256_of_empty_string() {
        assert_eq!(hash_tree_entries(std::iter::empty()), hash_bytes(b""));
        assert_eq!(
            hash_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn tree_hash_joins_with_newline_between_entries() {
        // Two files: cipher canonical is `a\0ha\nb\0hb` (no trailing newline).
        let a = SkillFile {
            skill_name: "s".into(),
            relative_path: PathBuf::from("a"),
            abs_path: PathBuf::from("/tmp/a"),
            hash: "ha".into(),
            modified: None,
        };
        let b = SkillFile {
            skill_name: "s".into(),
            relative_path: PathBuf::from("b"),
            abs_path: PathBuf::from("/tmp/b"),
            hash: "hb".into(),
            modified: None,
        };
        let got = hash_tree_entries([&a, &b].into_iter());
        let expected = hash_bytes(b"a\0ha\nb\0hb");
        assert_eq!(got, expected);
    }
}
