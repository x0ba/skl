//! Local two-pane skill browser. Wired to existing CLI use/unuse/sync.
//!
//! Entry: bare `skl` on a TTY, or `skl tui` / `skl ui`.
//! Never enters raw mode unless [`LaunchDecision::Enter`] is chosen.

use std::io::Write;

mod app;
mod launch;
mod render;
mod terminal;

pub use launch::{decide_launch, LaunchDecision, LaunchInput};

use crate::error::Result;

/// Run the TUI. Caller must have already chosen [`LaunchDecision::Enter`].
pub async fn run(api_base: String) -> Result<()> {
    app::run(api_base).await
}

/// Print clap help (non-TTY / `--no-tui` / `SKL_NO_TUI`). Never hangs.
pub fn print_help(cmd: &mut clap::Command) -> Result<()> {
    if let Err(err) = cmd.print_help() {
        if err.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(err.into());
        }
        return Ok(());
    }
    if let Err(err) = writeln!(std::io::stdout()) {
        if err.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(err.into());
        }
    }
    Ok(())
}

/// Message when the terminal cannot host a TUI (dumb / unsupported).
pub fn unsupported_terminal_message(reason: &str) -> String {
    format!(
        "skl TUI requires a capable terminal ({reason}).\n\
         Use a subcommand instead (`skl list`, `skl use`, `skl --help`)."
    )
}
