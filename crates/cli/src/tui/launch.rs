//! Pure launch policy. No raw mode, no crossterm — safe in CI.

use std::io::IsTerminal;

/// What the binary should do instead of (or before) entering the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchDecision {
    /// Safe to enter alternate screen + raw mode.
    Enter,
    /// Print clap help and exit. Never hang.
    Help,
    /// TTY is present but cannot host a TUI. Print a degrade message.
    Unsupported { reason: &'static str },
}

#[derive(Debug, Clone)]
pub struct LaunchInput {
    /// User asked for the TUI (`skl`, `skl tui`, `skl ui`).
    pub wants_tui: bool,
    /// `--no-tui` on the CLI.
    pub no_tui_flag: bool,
    /// `SKL_NO_TUI` is a blocking value (`1`, `true`, `yes`).
    pub no_tui_env: bool,
    pub stdin_is_tty: bool,
    pub stdout_is_tty: bool,
    /// `TERM` value, if set.
    pub term: Option<String>,
    /// `cfg!(windows)` for the current compile, overridable in tests.
    pub windows: bool,
}

impl LaunchInput {
    pub fn from_process(wants_tui: bool, no_tui_flag: bool) -> Self {
        Self {
            wants_tui,
            no_tui_flag,
            no_tui_env: env_no_tui(),
            stdin_is_tty: std::io::stdin().is_terminal(),
            stdout_is_tty: std::io::stdout().is_terminal(),
            term: std::env::var("TERM").ok(),
            windows: cfg!(windows),
        }
    }
}

/// `SKL_NO_TUI=1` (or true/yes) blocks the TUI. Unset / `0` / `false` do not.
pub fn env_no_tui() -> bool {
    match std::env::var("SKL_NO_TUI") {
        Ok(raw) => is_truthy(&raw),
        Err(_) => false,
    }
}

pub fn is_truthy(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub fn decide_launch(input: &LaunchInput) -> LaunchDecision {
    if !input.wants_tui {
        return LaunchDecision::Help;
    }
    if input.no_tui_flag || input.no_tui_env {
        return LaunchDecision::Help;
    }
    if !input.stdin_is_tty || !input.stdout_is_tty {
        return LaunchDecision::Help;
    }
    if is_dumb_term(input.term.as_deref()) {
        return LaunchDecision::Unsupported {
            reason: "TERM=dumb",
        };
    }
    if input.windows && !windows_console_ok(input.term.as_deref()) {
        return LaunchDecision::Unsupported {
            reason: "Windows console is not VT-capable; use Windows Terminal",
        };
    }
    LaunchDecision::Enter
}

fn is_dumb_term(term: Option<&str>) -> bool {
    match term {
        Some(t) => t.eq_ignore_ascii_case("dumb"),
        None => false,
    }
}

/// Modern Windows Terminal / ConPTY set `WT_SESSION` or a non-dumb `TERM`.
/// A bare `cmd.exe` without VT is treated as unsupported so we never half-init.
fn windows_console_ok(term: Option<&str>) -> bool {
    if std::env::var_os("WT_SESSION").is_some() {
        return true;
    }
    match term {
        Some(t) if !t.is_empty() && !t.eq_ignore_ascii_case("dumb") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> LaunchInput {
        LaunchInput {
            wants_tui: true,
            no_tui_flag: false,
            no_tui_env: false,
            stdin_is_tty: true,
            stdout_is_tty: true,
            term: Some("xterm-256color".into()),
            windows: false,
        }
    }

    #[test]
    fn tty_opens_tui() {
        assert_eq!(decide_launch(&base()), LaunchDecision::Enter);
    }

    #[test]
    fn non_tty_is_help_never_enter() {
        let mut input = base();
        input.stdin_is_tty = false;
        assert_eq!(decide_launch(&input), LaunchDecision::Help);
        input.stdin_is_tty = true;
        input.stdout_is_tty = false;
        assert_eq!(decide_launch(&input), LaunchDecision::Help);
    }

    #[test]
    fn no_tui_flag_and_env_are_help() {
        let mut input = base();
        input.no_tui_flag = true;
        assert_eq!(decide_launch(&input), LaunchDecision::Help);
        input.no_tui_flag = false;
        input.no_tui_env = true;
        assert_eq!(decide_launch(&input), LaunchDecision::Help);
    }

    #[test]
    fn dumb_term_degrades() {
        let mut input = base();
        input.term = Some("dumb".into());
        assert!(matches!(
            decide_launch(&input),
            LaunchDecision::Unsupported { reason } if reason.contains("dumb")
        ));
    }

    #[test]
    fn windows_without_vt_degrades() {
        let mut input = base();
        input.windows = true;
        input.term = None;
        assert!(matches!(
            decide_launch(&input),
            LaunchDecision::Unsupported { .. }
        ));
        input.term = Some("xterm-256color".into());
        assert_eq!(decide_launch(&input), LaunchDecision::Enter);
    }

    #[test]
    fn not_wanting_tui_is_help() {
        let mut input = base();
        input.wants_tui = false;
        assert_eq!(decide_launch(&input), LaunchDecision::Help);
    }

    #[test]
    fn truthy_env_values() {
        assert!(is_truthy("1"));
        assert!(is_truthy("true"));
        assert!(is_truthy("YES"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy("false"));
        assert!(!is_truthy(""));
    }
}
