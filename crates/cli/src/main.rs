mod api;
mod auth;
mod auto_sync;
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
    /// Import skills from ~/.claude/skills, ~/.cursor/skills, ~/.codex/skills, ~/.agents/skills, and ~/.config/agents/skills if present.
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
    /// Show or edit sticky extra dests (`~/.config/skl/config.toml`).
    Targets {
        #[command(subcommand)]
        action: Option<TargetsCommand>,
    },
    /// Symlink a skill into this project's agent dirs and skills.toml.
    Use {
        /// Skill names. With none, list skills already activated in the project.
        #[arg(value_name = "SKILL")]
        skills: Vec<String>,
        /// Project directory (default: cwd).
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Extra dest for this activation (`claude`, `cursor`, or `codex`). Repeatable.
        #[arg(short = 'a', long = "agent", value_name = "ID")]
        agents: Vec<String>,
    },
    /// Remove project skill symlinks and drop them from skills.toml.
    Unuse {
        #[arg(value_name = "SKILL", required = true)]
        skills: Vec<String>,
        /// Project directory (default: cwd).
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Explicit layout rewrites (never run from `skl use`).
    Migrate {
        #[command(subcommand)]
        action: MigrateAction,
    },
}

#[derive(Debug, Subcommand)]
enum MigrateAction {
    /// Move M0 `.claude`/`.cursor` project links onto canonical `.agents/skills`.
    Targets {
        /// Project directory (default: cwd).
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Remove old .claude/.cursor (and .codex) links after the canonical dest exists.
        #[arg(long)]
        prune_old: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TargetsCommand {
    /// Add extra dests (`claude`, `cursor`, or `codex`).
    Add {
        #[arg(value_name = "ID", required = true)]
        ids: Vec<String>,
    },
    /// Remove extra dests.
    Remove {
        #[arg(value_name = "ID", required = true)]
        ids: Vec<String>,
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
        Command::Init => commands::init::run(api_base).await,
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
        Command::Status => commands::status::run(api_base).await,
        Command::List => commands::list::run(api_base).await,
        Command::Doctor => commands::doctor::run(api_base).await,
        Command::Targets { action } => {
            let action = match action {
                None => commands::targets::TargetsAction::List,
                Some(TargetsCommand::Add { ids }) => commands::targets::TargetsAction::Add(ids),
                Some(TargetsCommand::Remove { ids }) => {
                    commands::targets::TargetsAction::Remove(ids)
                }
            };
            commands::targets::run(action)
        }
        Command::Use {
            skills,
            project,
            agents,
        } => commands::use_cmd::run(&skills, project, &agents, &api_base).await,
        Command::Unuse { skills, project } => {
            commands::unuse::run(&skills, project, &api_base).await
        }
        Command::Migrate {
            action: MigrateAction::Targets { project, prune_old },
        } => commands::migrate::run(project, prune_old),
    }
}
