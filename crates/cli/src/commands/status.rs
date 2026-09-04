//! `skl status` — login, api_base, local inventory, last sync.

use crate::auth::{self, TokenPresence};
use crate::config::{self, Paths};
use crate::error::Result;
use crate::local::db::LocalDb;

pub fn run(api_base: String) -> Result<()> {
    let token = auth::token_presence();
    let token_line = match &token {
        TokenPresence::Present { preview } => format!("yes ({preview})"),
        TokenPresence::Absent => "no  (run `skl login` or `skl login --dev-user <id>`)".into(),
        TokenPresence::Error(msg) => format!("error ({msg})"),
    };

    println!("api_base     {api_base}");
    println!("token        {token_line}");

    let paths = match Paths::resolve() {
        Ok(paths) => paths,
        Err(err) => {
            println!("config       {err}");
            return Ok(());
        }
    };
    println!("config       {}", paths.config_file.display());
    println!("state.db     {}", paths.db_file.display());

    if paths.db_file.exists() {
        let db = LocalDb::open(&paths.db_file)?;
        println!("local_skills {}", db.skill_count()?);
        match db.last_sync_summary()? {
            Some((at, summary)) => {
                println!(
                    "last_sync    at={at}  uploaded={}  downloaded={}  pushed={}  conflicts={}  missing_skills={}",
                    summary.uploaded,
                    summary.downloaded,
                    summary.pushed,
                    summary.conflicts,
                    summary.missing_skills
                );
            }
            None => println!("last_sync    (none)"),
        }
    } else {
        println!("local_skills 0  (run `skl init`)");
        println!("last_sync    (none)");
    }

    let _ = config::load(&paths);
    Ok(())
}
