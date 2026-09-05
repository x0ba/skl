//! Process-level: non-TTY / SKL_NO_TUI never enters raw mode (CI-safe).

use std::io::Write;
use std::process::{Command, Stdio};

fn skl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_skl"))
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
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:") || stdout.contains("Usage:"), "{stdout}");
    assert!(stdout.contains("list") || stdout.contains("List"), "{stdout}");
}

#[test]
fn no_tui_flag_prints_help() {
    let out = skl()
        .args(["--no-tui"])
        .stdin(Stdio::null())
        .output()
        .expect("run skl");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"), "{stdout}");
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
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"), "{stdout}");
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
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"), "{stdout}");
}
