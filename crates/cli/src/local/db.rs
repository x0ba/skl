use std::path::Path;

use rusqlite::{params, Connection};

use crate::api::types::{SkillTree, SyncRequest};
use crate::error::{Result, SklError};
use crate::local::skills::DiscoveredSkill;

pub struct LocalDb {
    conn: Connection,
}

impl LocalDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS skills (
                name TEXT NOT NULL,
                source TEXT NOT NULL,
                path TEXT NOT NULL,
                tree_hash TEXT NOT NULL,
                imported_at INTEGER NOT NULL,
                PRIMARY KEY (name, source)
            );
            CREATE TABLE IF NOT EXISTS skill_files (
                skill_name TEXT NOT NULL,
                source TEXT NOT NULL,
                path TEXT NOT NULL,
                hash TEXT NOT NULL,
                PRIMARY KEY (skill_name, source, path)
            );
            ",
        )?;
        Ok(())
    }

    pub fn replace_import(&self, skills: &[DiscoveredSkill]) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM skill_files", [])?;
        tx.execute("DELETE FROM skills", [])?;
        for skill in skills {
            tx.execute(
                "INSERT INTO skills (name, source, path, tree_hash, imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    skill.name,
                    skill.source,
                    skill.path.to_string_lossy(),
                    skill.tree.tree_hash,
                    now
                ],
            )?;
            for (path, hash) in &skill.tree.files {
                tx.execute(
                    "INSERT INTO skill_files (skill_name, source, path, hash)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![skill.name, skill.source, path, hash],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('last_init_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![now.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_skills(&self) -> Result<Vec<DiscoveredSkill>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, source, path, tree_hash FROM skills ORDER BY source, name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut skills = Vec::new();
        for row in rows {
            let (name, source, path, tree_hash) = row?;
            let mut files = std::collections::BTreeMap::new();
            let mut file_stmt = self.conn.prepare(
                "SELECT path, hash FROM skill_files WHERE skill_name = ?1 AND source = ?2",
            )?;
            let file_rows = file_stmt.query_map(params![name, source], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for file in file_rows {
                let (p, h) = file?;
                files.insert(p, h);
            }
            skills.push(DiscoveredSkill {
                name,
                source,
                path: path.into(),
                tree: SkillTree { tree_hash, files },
            });
        }
        Ok(skills)
    }

    pub fn sync_request(&self) -> Result<SyncRequest> {
        let skills = self.list_skills()?;
        let mut map = std::collections::BTreeMap::new();
        for skill in skills {
            if map.contains_key(&skill.name) {
                return Err(SklError::LocalState(format!(
                    "duplicate skill name `{}` from multiple sources; hammer/conflict pass needed",
                    skill.name
                )));
            }
            map.insert(skill.name, skill.tree);
        }
        Ok(SyncRequest { skills: map })
    }

    pub fn skill_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn find_skill(&self, name: &str) -> Result<Option<DiscoveredSkill>> {
        Ok(self
            .list_skills()?
            .into_iter()
            .find(|skill| skill.name == name))
    }

    /// Locate a local file whose content hash matches.
    pub fn find_file_by_hash(&self, hash: &str) -> Result<Option<(std::path::PathBuf, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.path, f.path
             FROM skill_files f
             JOIN skills s ON s.name = f.skill_name AND s.source = f.source
             WHERE f.hash = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![hash])?;
        if let Some(row) = rows.next()? {
            let skill_dir: String = row.get(0)?;
            let rel: String = row.get(1)?;
            return Ok(Some((skill_dir.into(), rel)));
        }
        Ok(None)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(row.get(0)?));
        }
        Ok(None)
    }

    pub fn record_sync_summary(&self, summary: &SyncSummary) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.set_meta("last_sync_at", &now.to_string())?;
        self.set_meta(
            "last_sync_json",
            &serde_json::to_string(summary).map_err(SklError::from)?,
        )?;
        self.clear_sync_error()?;
        Ok(())
    }

    pub fn record_sync_error(&self, message: &str) -> Result<()> {
        self.set_meta("last_sync_error", message)
    }

    pub fn clear_sync_error(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM meta WHERE key = 'last_sync_error'", [])?;
        Ok(())
    }

    pub fn last_sync_error(&self) -> Result<Option<String>> {
        Ok(self.get_meta("last_sync_error")?.filter(|s| !s.is_empty()))
    }

    pub fn last_sync_summary(&self) -> Result<Option<(i64, SyncSummary)>> {
        let Some(at) = self.get_meta("last_sync_at")? else {
            return Ok(None);
        };
        let Some(json) = self.get_meta("last_sync_json")? else {
            return Ok(None);
        };
        let at: i64 = at.parse().unwrap_or(0);
        let summary: SyncSummary = serde_json::from_str(&json)?;
        Ok(Some((at, summary)))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct SyncSummary {
    pub uploaded: usize,
    pub downloaded: usize,
    pub pushed: usize,
    pub conflicts: usize,
    pub missing_skills: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::skills::hash_skill_dir;

    #[test]
    fn import_and_sync_payload() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("foo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();
        let tree = hash_skill_dir(&skill_dir).unwrap();
        let db = LocalDb::open(&tmp.path().join("state.db")).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "foo".into(),
            source: "claude".into(),
            path: skill_dir,
            tree: tree.clone(),
        }])
        .unwrap();
        let req = db.sync_request().unwrap();
        assert_eq!(req.skills["foo"].tree_hash, tree.tree_hash);
        let found = db.find_file_by_hash(&tree.files["SKILL.md"]).unwrap();
        assert!(found.is_some());
        db.record_sync_summary(&SyncSummary {
            uploaded: 1,
            downloaded: 0,
            pushed: 1,
            conflicts: 0,
            missing_skills: 0,
        })
        .unwrap();
        let (at, summary) = db.last_sync_summary().unwrap().unwrap();
        assert!(at > 0);
        assert_eq!(summary.uploaded, 1);
        assert_eq!(db.skill_count().unwrap(), 1);
    }
}
