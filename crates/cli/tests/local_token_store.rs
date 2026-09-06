//! Process-level: login persists to state.db without OS keyring / DBus.
//! Env overrides win; logout clears the local store.

use std::path::Path;
use std::process::{Command, Stdio};

fn skl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_skl"))
}

fn run_skl(home: &Path, args: &[&str]) -> std::process::Output {
    let data = home.join(".local/share/skl");
    let config = home.join(".config/skl");
    skl()
        .args(args)
        .env("HOME", home)
        .env("SKL_DATA_DIR", &data)
        .env("SKL_CONFIG_DIR", &config)
        .env_remove("SKL_TOKEN")
        .env_remove("SKL_TOKEN_FILE")
        .env("SKL_NO_PROMPT", "1")
        .env("API_BASE", "http://127.0.0.1:1")
        .stdin(Stdio::null())
        .output()
        .expect("run skl")
}

#[test]
fn login_dev_user_persists_without_keyring() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let out = run_skl(home, &["login", "--dev-user", "alice"]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("store") && !stderr.to_lowercase().contains("keyring"),
        "{stderr}"
    );
    assert!(
        home.join(".local/share/skl/state.db").exists(),
        "login must create state.db"
    );
    let status = run_skl(home, &["status"]);
    assert!(status.status.success());
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("dev:alice"), "{stdout}");
}

#[test]
fn status_uses_local_store_then_logout_clears() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    assert!(run_skl(home, &["login", "--dev-user", "bob"])
        .status
        .success());
    let status = run_skl(home, &["status"]);
    assert!(status.status.success());
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("dev:bob"), "{stdout}");

    let logout = run_skl(home, &["logout"]);
    assert!(
        logout.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&logout.stderr)
    );
    let after = run_skl(home, &["status"]);
    let after_out = String::from_utf8_lossy(&after.stdout);
    assert!(
        after_out.contains("token") && after_out.contains("no"),
        "{after_out}"
    );
}

#[test]
fn skl_token_overrides_local_store() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    assert!(run_skl(home, &["login", "--dev-user", "local-user"])
        .status
        .success());
    let data = home.join(".local/share/skl");
    let config = home.join(".config/skl");
    let out = skl()
        .args(["status"])
        .env("HOME", home)
        .env("SKL_DATA_DIR", &data)
        .env("SKL_CONFIG_DIR", &config)
        .env("SKL_TOKEN", "dev:from-env")
        .env_remove("SKL_TOKEN_FILE")
        .env("SKL_NO_PROMPT", "1")
        .env("API_BASE", "http://127.0.0.1:1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dev:from-env"), "{stdout}");

    let local = run_skl(home, &["status"]);
    let local_out = String::from_utf8_lossy(&local.stdout);
    assert!(
        local_out.contains("dev:local-user"),
        "env override must not wipe the local store: {local_out}"
    );
}

#[test]
fn skl_token_file_overrides_local_store() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    assert!(run_skl(home, &["login", "--dev-user", "local-user"])
        .status
        .success());
    let token_file = home.join("token-file");
    std::fs::write(&token_file, "dev:from-file\n").unwrap();
    let data = home.join(".local/share/skl");
    let config = home.join(".config/skl");
    let out = skl()
        .args(["status"])
        .env("HOME", home)
        .env("SKL_DATA_DIR", &data)
        .env("SKL_CONFIG_DIR", &config)
        .env_remove("SKL_TOKEN")
        .env("SKL_TOKEN_FILE", &token_file)
        .env("SKL_NO_PROMPT", "1")
        .env("API_BASE", "http://127.0.0.1:1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dev:from-file"), "{stdout}");
}

#[test]
fn doctor_mentions_local_store_not_required_keyring() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    assert!(run_skl(home, &["login", "--dev-user", "doc"])
        .status
        .success());
    let out = run_skl(home, &["doctor"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("token        present"), "{stdout}");
    assert!(stdout.contains("token_store  local state.db"), "{stdout}");
    assert!(
        !stdout.contains("keyring      "),
        "doctor must not present keyring as the required store:\n{stdout}"
    );
}
