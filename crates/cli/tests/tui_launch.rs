//! Process-level: non-TTY / piped / CI / SKL_NO_TUI never enter fullscreen.
//!
//! Hammer coverage stacked on furnace launch policy. A PTY (`script` / python)
//! is required to prove `--no-tui` on a real TTY — that lives in
//! `scripts/smoke-tui.sh`. These tests stay CI-safe (no raw mode).

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// CSI used by crossterm `EnterAlternateScreen` (`?1049h`).
const ALT_SCREEN: &[u8] = b"\x1b[?1049h";
const ALT_SCREEN_DECSET: &[u8] = b"\x1b[?1049";

fn skl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_skl"))
}

fn assert_help_never_fullscreen(out: &Output) {
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage:"),
        "expected clap help, got: {stdout}"
    );
    for (label, bytes) in [("stdout", out.stdout.as_slice()), ("stderr", out.stderr.as_slice())]
    {
        assert!(
            !contains_seq(bytes, ALT_SCREEN) && !contains_seq(bytes, ALT_SCREEN_DECSET),
            "{label} entered alternate screen (fullscreen):\n{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

fn contains_seq(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn no_tui_env_prints_help_and_exits() {
    let out = skl()
        .env("SKL_NO_TUI", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run skl");
    assert_help_never_fullscreen(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("list") || stdout.contains("List"), "{stdout}");
}

#[test]
fn no_tui_flag_prints_help() {
    let out = skl()
        .args(["--no-tui"])
        .stdin(Stdio::null())
        .output()
        .expect("run skl");
    assert_help_never_fullscreen(&out);
}

#[test]
fn help_flag_still_works() {
    let out = skl()
        .args(["-h"])
        .stdin(Stdio::null())
        .output()
        .expect("run skl");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"), "{stdout}");
    assert!(stdout.contains("tui") || stdout.contains("TUI") || stdout.contains("ui"), "{stdout}");
}

#[test]
fn help_subcommand_still_works() {
    let out = skl()
        .args(["help"])
        .stdin(Stdio::null())
        .output()
        .expect("run skl help");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"), "{stdout}");
}

#[test]
fn explicit_tui_on_non_tty_prints_help() {
    let out = skl()
        .args(["tui"])
        .env("SKL_NO_TUI", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run skl tui");
    assert_help_never_fullscreen(&out);
}

#[test]
fn list_subcommand_still_runs_without_tui() {
    let tmp = tempfile::tempdir().unwrap();
    let out = skl()
        .args(["list"])
        .env("HOME", tmp.path())
        .env("SKL_CONFIG_DIR", tmp.path().join("cfg"))
        .env("SKL_DATA_DIR", tmp.path().join("data"))
        .env("SKL_NO_TUI", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run skl list");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("no local skill index") || combined.contains("no local skills"),
        "{combined}"
    );
}

#[test]
fn stdin_pipe_never_hangs() {
    let mut child = skl()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn skl");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        let _ = stdin.write_all(b"");
    }
    let started = Instant::now();
    let out = child.wait_with_output().expect("wait");
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "piped stdin hung for {:?}",
        started.elapsed()
    );
    assert_help_never_fullscreen(&out);
}

#[test]
fn ci_env_piped_prints_help_never_fullscreen() {
    let out = skl()
        .env("CI", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run skl under CI=");
    assert_help_never_fullscreen(&out);
}

#[test]
fn stdout_pipe_never_enters_fullscreen() {
    let out = skl()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run skl with piped stdout");
    assert_help_never_fullscreen(&out);
}
