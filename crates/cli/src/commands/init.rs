use std::path::Path;

use crate::config::{self, Paths};
use crate::error::Result;
use crate::local::db::LocalDb;
use crate::local::library;
use crate::local::skills;

pub async fn run(api_base: String) -> Result<()> {
    let paths = Paths::resolve()?;
    let home = config::home_dir()?;
    import_from_home(&home, &paths)?;
    config::maybe_prompt_sticky_extras(&paths)?;
    let _ = crate::auto_sync::maybe_run(&api_base, &paths, "init").await;
    Ok(())
}

/// Discover foreign home roots, copy missing skills into the personal library,
/// and index the library only. Foreign trees stay in place (importer).
fn import_from_home(home: &Path, paths: &Paths) -> Result<usize> {
    paths.ensure()?;
    std::fs::create_dir_all(paths.library_dir())?;

    let foreign = skills::discover_from_home(home)?;

    if foreign.is_empty() {
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
        eprintln!("Discovered {} skill(s):", foreign.len());
        for skill in &foreign {
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

    let copied = library::import_foreign(&foreign, &paths.library_dir())?;
    if copied > 0 {
        eprintln!(
            "Copied {copied} skill(s) into the personal library ({})",
            paths.library_dir().display()
        );
    }

    let discovered = library::discover(paths)?;
    let db = LocalDb::open(&paths.db_file)?;
    db.replace_import(&discovered)?;

    eprintln!();
    eprintln!(
        "Imported {} skill(s) into {} and {}",
        discovered.len(),
        paths.library_dir().display(),
        paths.db_file.display()
    );
    eprintln!(
        "Local state is ready for `skl sync` (personal library only — not agent or project dirs)."
    );
    Ok(discovered.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::library;
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
        assert_eq!(listed[0].source, library::LIBRARY_SOURCE);
        assert_eq!(listed[0].name, "greeter");
        assert_eq!(listed[0].path, paths.library_skill("greeter"));
        assert!(listed[0].tree.files.contains_key("SKILL.md"));
        assert_eq!(
            fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
            "hi from agents"
        );
        assert_eq!(
            fs::read_to_string(paths.library_skill("greeter").join("SKILL.md")).unwrap(),
            "hi from agents"
        );
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
        assert_eq!(listed[0].source, library::LIBRARY_SOURCE);
        assert_eq!(listed[0].name, "notes");
        assert_eq!(listed[0].path, paths.library_skill("notes"));
        assert!(skill_dir.join("SKILL.md").is_file());
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
        let names: Vec<_> = listed.iter().map(|skill| skill.name.as_str()).collect();
        assert!(names.contains(&"greeter"));
        assert!(names.contains(&"notes"));
        for skill in &listed {
            assert_eq!(skill.source, library::LIBRARY_SOURCE);
            assert_eq!(skill.path, paths.library_skill(&skill.name));
        }
        assert!(agents.join("SKILL.md").is_file());
        assert!(xdg.join("SKILL.md").is_file());
    }
}
