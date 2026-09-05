use std::path::Path;

use crate::config::{self, Paths};
use crate::error::Result;
use crate::local::db::LocalDb;
use crate::local::skills;

pub async fn run(api_base: String) -> Result<()> {
    let paths = Paths::resolve()?;
    let home = config::home_dir()?;
    import_from_home(&home, &paths)?;
    config::maybe_prompt_sticky_extras(&paths)?;
    let _ = crate::auto_sync::maybe_run(&api_base, &paths, "init").await;
    Ok(())
}

/// Discover `skill_roots` under `home` and replace the local import index.
fn import_from_home(home: &Path, paths: &Paths) -> Result<usize> {
    paths.ensure()?;

    let discovered = skills::discover_from_home(home)?;

    if discovered.is_empty() {
        eprintln!("No skills found under:");
        for root in config::skill_roots(home) {
            let mark = if root.path.is_dir() {
                "empty"
            } else {
                "missing"
            };
            eprintln!("  {:<8} {} ({mark})", root.source, root.path.display());
        }
    } else {
        eprintln!("Discovered {} skill(s):", discovered.len());
        for skill in &discovered {
            eprintln!(
                "  {:<8} {:<24} {}  files={}  tree={}",
                skill.source,
                skill.name,
                skill.path.display(),
                skill.tree.files.len(),
                &skill.tree.tree_hash[..skill.tree.tree_hash.len().min(12)]
            );
        }
    }

    let db = LocalDb::open(&paths.db_file)?;
    db.replace_import(&discovered)?;

    eprintln!();
    eprintln!(
        "Imported {} skill(s) into {}",
        discovered.len(),
        paths.db_file.display()
    );
    eprintln!("Local state is ready for `skl sync` (POST /v1/sync).");
    Ok(discovered.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn isolated_paths(tmp: &Path) -> Paths {
        let config_dir = tmp.join("config");
        let data_dir = tmp.join("data");
        Paths {
            config_file: config_dir.join("config.toml"),
            db_file: data_dir.join("state.db"),
            config_dir,
            data_dir,
        }
    }

    fn plant_skill(dir: &Path, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn init_imports_skill_planted_under_home_agents_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".agents/skills/greeter");
        plant_skill(&skill_dir, "hi from agents");

        let paths = isolated_paths(tmp.path());
        let imported = import_from_home(&home, &paths).unwrap();
        assert_eq!(imported, 1);

        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source, "agents");
        assert_eq!(listed[0].name, "greeter");
        assert_eq!(listed[0].path, skill_dir);
        assert!(listed[0].tree.files.contains_key("SKILL.md"));
    }

    #[test]
    fn init_imports_skill_planted_under_xdg_agents_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skill_dir = home.join(".config/agents/skills/notes");
        plant_skill(&skill_dir, "notes");

        let paths = isolated_paths(tmp.path());
        let imported = import_from_home(&home, &paths).unwrap();
        assert_eq!(imported, 1);

        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source, "xdg-agents");
        assert_eq!(listed[0].name, "notes");
        assert_eq!(listed[0].path, skill_dir);
    }

    #[test]
    fn init_imports_both_universal_home_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let agents = home.join(".agents/skills/greeter");
        let xdg = home.join(".config/agents/skills/notes");
        plant_skill(&agents, "hi");
        plant_skill(&xdg, "notes");

        let paths = isolated_paths(tmp.path());
        let imported = import_from_home(&home, &paths).unwrap();
        assert_eq!(imported, 2);

        let listed = LocalDb::open(&paths.db_file)
            .unwrap()
            .list_skills()
            .unwrap();
        let pairs: Vec<_> = listed
            .iter()
            .map(|skill| (skill.source.as_str(), skill.name.as_str()))
            .collect();
        assert!(pairs.contains(&("agents", "greeter")));
        assert!(pairs.contains(&("xdg-agents", "notes")));
        assert!(!pairs.iter().any(|(src, _)| *src == "claude"));
    }
}
