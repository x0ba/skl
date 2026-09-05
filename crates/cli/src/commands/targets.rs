//! `skl targets` — list / add / remove sticky extra dests.

use crate::config::{self, Paths};
use crate::error::Result;
use crate::local::linker::{CANONICAL_TARGET_ID, EXTRA_TARGET_IDS};

#[derive(Debug, Clone)]
pub enum TargetsAction {
    List,
    Add(Vec<String>),
    Remove(Vec<String>),
}

pub fn run(action: TargetsAction) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;

    match action {
        TargetsAction::List => {
            let cfg = config::load(&paths).unwrap_or_default();
            print_targets(&paths, &cfg);
        }
        TargetsAction::Add(ids) => {
            let cfg = config::add_sticky_extras(&paths, &ids)?;
            eprintln!("updated extras in {}", paths.config_file.display());
            print_targets(&paths, &cfg);
        }
        TargetsAction::Remove(ids) => {
            let cfg = config::remove_sticky_extras(&paths, &ids)?;
            eprintln!("updated extras in {}", paths.config_file.display());
            print_targets(&paths, &cfg);
        }
    }
    Ok(())
}

fn print_targets(paths: &Paths, cfg: &config::Config) {
    println!("canonical  {CANONICAL_TARGET_ID}  (.agents/skills)");
    let extras = cfg.sticky_extras();
    if extras.is_empty() {
        println!("extra      (none — `skl use` writes only .agents/skills)");
    } else {
        println!("extra      {}", extras.join(", "));
    }
    println!("config     {}", paths.config_file.display());
    println!(
        "ids        {CANONICAL_TARGET_ID} (always)  extras: {}",
        EXTRA_TARGET_IDS.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_shows_empty_extras_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        paths.ensure().unwrap();
        let cfg = config::load(&paths).unwrap_or_default();
        assert!(cfg.sticky_extras().is_empty());
    }
}
