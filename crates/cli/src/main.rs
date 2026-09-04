mod api;
mod auth;
mod commands;
mod config;
mod error;
mod local;

use clap::{Parser, Subcommand};

use crate::config::{resolve_api_base, Paths};
use crate::error::SklError;

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
    Login,
    /// Import skills from ~/.claude/skills, ~/.cursor/skills, and ~/.codex/skills if present.
    Init,
    /// POST /v1/sync with the local inventory (blob/conflict steps are stubs).
    Sync,
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
        Command::Login => commands::login::run(api_base).await,
        Command::Init => commands::init::run(),
        Command::Sync => commands::sync::run(api_base).await,
    }
}
