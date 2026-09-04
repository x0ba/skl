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
    }
}
