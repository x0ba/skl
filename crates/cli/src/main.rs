mod api;
mod auth;
mod auto_sync;
mod catalog;
mod checklist;
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
mod tui;

#[cfg(test)]
mod harness_catalog_contract;

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

use crate::config::{resolve_api_base, Paths};
use crate::error::SklError;
use crate::hooks::conflict::ConflictMode;
use crate::tui::{decide_launch, LaunchDecision, LaunchInput};

#[derive(Debug, Parser)]
#[command(
    name = "skl",
    about = "Personal agent skill sync",
    version
)]
struct Cli {
    /// API origin (no trailing slash). Overrides config. Env: API_BASE.
    #[arg(long, env = "API_BASE", global = true)]
    api_base: Option<String>,

    /// Print help instead of opening the TUI (also `SKL_NO_TUI=1`).
    #[arg(long, global = true)]
    no_tui: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Device authorization: poll for approval, store token in OS keyring.
    Login {
        /// Local-only: store `Authorization: Bearer dev:<USER_ID>` (no Clerk / no device poll).
        #[arg(long, value_name = "USER_ID")]
        dev_user: Option<String>,
    },
    /// First-run after install: login [Y/n], init [Y/n], harness checklist.
    Setup {
        /// Binary-only: skip login / init / checklist prompts.
        #[arg(long)]
        non_interactive: bool,
    },
    /// Import skills from unique catalog global roots plus ~/.agents/skills.
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
    /// Activate a skill, list activated skills, or restore all (`--all`).
    Use {
        /// Skill names. With none, list skills already activated in the project.
        #[arg(value_name = "SKILL")]
        skills: Vec<String>,
        /// Rematerialize every skill listed in skills.toml from this machine's library.
        #[arg(long)]
        all: bool,
        /// Project directory (default: cwd).
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Extra dest for this activation (custom catalog id, e.g. `claude-code`). Repeatable.
        #[arg(short = 'a', long = "agent", value_name = "ID")]
        agents: Vec<String>,
    },
    /// Promote a project skill into the personal library (`~/.local/share/skl/skills`).
    Capture {
        /// Project skill path or name (resolved under `.agents/skills` + sticky extras).
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Overwrite an existing library skill of the same name.
        #[arg(long)]
        force: bool,
        /// Capture under a different skill name.
        #[arg(long = "as", value_name = "NAME")]
        as_name: Option<String>,
        /// Copy into the library; leave the project dir as a real copy (no symlink).
        #[arg(long)]
        keep_copy: bool,
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
    /// Explicit layout rewrites (never run from `skl use`).
    Migrate {
        #[command(subcommand)]
        action: MigrateAction,
    },
    /// Interactive two-pane skill browser (also opened by bare `skl` on a TTY).
    Tui,
    /// Alias for `tui`.
    Ui,
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
    /// Add extra dests (custom catalog ids, e.g. `claude-code`). Rejects universal ids.
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

    let wants_tui = matches!(cli.command, None | Some(Command::Tui) | Some(Command::Ui));
    if wants_tui {
        return dispatch_tui(cli.no_tui, api_base).await;
    }

    match cli.command.expect("subcommand required when not launching TUI") {
        Command::Login { dev_user } => commands::login::run(api_base, dev_user).await,
        Command::Setup { non_interactive } => commands::setup::run(api_base, non_interactive).await,
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
            all,
            project,
            agents,
        } => commands::use_cmd::run(&skills, project, &agents, all, &api_base).await,
        Command::Capture {
            path,
            force,
            as_name,
            keep_copy,
            project,
        } => {
            commands::capture::run(
                path,
                commands::capture::CaptureOpts {
                    force,
                    as_name,
                    keep_copy,
                    project,
                },
                &api_base,
            )
            .await
        }
        Command::Unuse { skills, project } => {
            commands::unuse::run(&skills, project, &api_base).await
        }
        Command::Migrate {
            action: MigrateAction::Targets { project, prune_old },
        } => commands::migrate::run(project, prune_old),
        Command::Tui | Command::Ui => unreachable!("TUI dispatched before match"),
    }
}

async fn dispatch_tui(no_tui_flag: bool, api_base: String) -> Result<(), SklError> {
    let decision = decide_launch(&LaunchInput::from_process(true, no_tui_flag));
    match decision {
        LaunchDecision::Enter => tui::run(api_base).await,
        LaunchDecision::Help => {
            let mut cmd = Cli::command();
            tui::print_help(&mut cmd)
        }
        LaunchDecision::Unsupported { reason } => {
            eprintln!("{}", tui::unsupported_terminal_message(reason));
            Ok(())
        }
    }
}

#[cfg(test)]
mod cli_parse_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn bare_skl_is_tui_request() {
        let cli = Cli::try_parse_from(["skl"]).unwrap();
        assert!(cli.command.is_none());
        assert!(!cli.no_tui);
    }

    #[test]
    fn tui_and_ui_subcommands() {
        let tui = Cli::try_parse_from(["skl", "tui"]).unwrap();
        assert!(matches!(tui.command, Some(Command::Tui)));
        let ui = Cli::try_parse_from(["skl", "ui"]).unwrap();
        assert!(matches!(ui.command, Some(Command::Ui)));
    }

    #[test]
    fn no_tui_flag_parses() {
        let cli = Cli::try_parse_from(["skl", "--no-tui"]).unwrap();
        assert!(cli.no_tui);
        assert!(cli.command.is_none());
        let with_cmd = Cli::try_parse_from(["skl", "--no-tui", "status"]).unwrap();
        assert!(with_cmd.no_tui);
        assert!(matches!(with_cmd.command, Some(Command::Status)));
    }

    #[test]
    fn other_subcommands_still_parse() {
        assert!(matches!(
            Cli::try_parse_from(["skl", "list"]).unwrap().command,
            Some(Command::List)
        ));
        assert!(matches!(
            Cli::try_parse_from(["skl", "sync"]).unwrap().command,
            Some(Command::Sync { .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["skl", "use", "greeter"]).unwrap().command,
            Some(Command::Use { .. })
        ));
    }
}
