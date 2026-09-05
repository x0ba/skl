use crate::config::{self, Paths};
use crate::error::Result;
use crate::local::db::LocalDb;
use crate::local::skills;

pub fn run() -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;

    let home = config::home_dir()?;
    let discovered = skills::discover_from_home(&home)?;

    if discovered.is_empty() {
        eprintln!("No skills found under:");
        for root in config::skill_roots(&home) {
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
    config::maybe_prompt_sticky_extras(&paths)?;
    Ok(())
}
