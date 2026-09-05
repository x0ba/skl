//! `skl status` — login, api_base, local inventory, last sync.
//!
//! Best-effort piggyback: may run auto-sync when due, then still prints status.

use crate::auth::{self, TokenPresence};
use crate::auto_sync::{maybe_run, AutoSyncResult};
use crate::config::{self, Paths};
use crate::error::Result;
use crate::local::db::LocalDb;

pub async fn run(api_base: String) -> Result<()> {
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

    let cfg = config::load(&paths).unwrap_or_default();
    println!(
        "auto_sync    {}",
        if cfg.sync.auto { "on" } else { "off" }
    );
    println!("sync_frequency {}s", cfg.sync.frequency_secs);

    let auto = maybe_run(&api_base, &paths, "status").await;

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
        match &auto {
            AutoSyncResult::FailedSoft { err } => println!("sync_issue   {err}"),
            _ => {
                if let Some(issue) = db.last_sync_error()? {
                    println!("sync_issue   {issue}");
                }
            }
        }
    } else {
        println!("local_skills 0  (run `skl init`)");
        println!("last_sync    (none)");
        if let AutoSyncResult::FailedSoft { err } = &auto {
            println!("sync_issue   {err}");
        }
    }

    if let AutoSyncResult::Ran(outcome) = auto {
        println!(
            "auto_sync    ran  uploaded={}  downloaded={}  pushed={}  conflicts={}",
            outcome.uploaded.len(),
            outcome.downloaded.len(),
            outcome.pushed.len(),
            outcome.conflicts
        );
    }

    Ok(())
}
