mod api;
mod auth;
mod commands;
mod config;
mod conflict;
mod error;
mod hooks;
mod local;
mod paths;
mod prepare;
mod prompt;
mod scrub;
mod skill_tree;
mod sync;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::{resolve_api_base, Paths};
use crate::error::SklError;
use crate::hooks::conflict::ConflictMode;

#[derive(Debug, Parser)]
#[command(
    name = "skl",
    about = "Personal agent skill sync",
    version,
    arg_required_else_help = true
)]
struct Cli {
    /// API origin (no trailing slash). Overrides config. Env: API_BASE.
    #[arg(long, env = "API_BASE", global = true)]
    api_base: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Device authorization: poll for approval, store token in OS keyring.
    Login {
        /// Local-only: store `Authorization: Bearer dev:<USER_ID>` (no Clerk / no device poll).
        #[arg(long, value_name = "USER_ID")]
        dev_user: Option<String>,
    },
    /// Import skills from ~/.claude/skills, ~/.cursor/skills, and ~/.codex/skills if present.
    Init,
    /// Hash sync: POST /v1/sync, PUT blobs, PUT trees, GET downloads.
    Sync {
        /// Resolve every tree-hash conflict by keeping local (overwrite remote).
        #[arg(long, group = "resolution")]
        keep_local: bool,
        /// Resolve every tree-hash conflict by keeping remote (overwrite local).
        #[arg(long, group = "resolution")]
        keep_remote: bool,
        /// Allow warning-level secret findings; blocks still refuse upload.
        #[arg(long)]
        allow_warnings: bool,
    },
    /// Show login, api_base, local skill count, last sync.
    Status,
    /// List local skills from state.db (and remote presence when logged in).
    List,
    /// Diagnose agent skill paths, keyring, state.db, and GET /v1/health.
    Doctor,
    /// Symlink a skill into `.agents/skills` (plus legacy agent dirs) and skills.toml.
    Use {
        /// Skill names. With none, list skills already activated in the project.
        #[arg(value_name = "SKILL")]
        skills: Vec<String>,
        /// Project directory (default: cwd).
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Remove project skill symlinks and drop them from skills.toml.
    Unuse {
        #[arg(value_name = "SKILL", required = true)]
        skills: Vec<String>,
        /// Project directory (default: cwd).
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), SklError> {
    let cli = Cli::parse();
    let paths = Paths::resolve().ok();
    let stored = paths
        .as_ref()
        .and_then(|p| crate::config::load(p).ok())
        .unwrap_or_default();
    let api_base = resolve_api_base(cli.api_base.as_deref(), &stored);

    match cli.command {
        Command::Login { dev_user } => commands::login::run(api_base, dev_user).await,
        Command::Init => commands::init::run(),
        Command::Sync {
            keep_local,
            keep_remote,
            allow_warnings,
        } => {
            let conflict = if keep_local {
                ConflictMode::KeepLocal
            } else if keep_remote {
                ConflictMode::KeepRemote
            } else {
                ConflictMode::Prompt
            };
            commands::sync::run(
                api_base,
                sync::SyncOptions {
                    conflict,
                    allow_warnings,
                },
            )
            .await
        }
        Command::Status => commands::status::run(api_base),
        Command::List => commands::list::run(api_base).await,
        Command::Doctor => commands::doctor::run(api_base).await,
        Command::Use { skills, project } => commands::use_cmd::run(&skills, project),
        Command::Unuse { skills, project } => commands::unuse::run(&skills, project),
    }
}
