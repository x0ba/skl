//! Process-level: auto-sync hides HTTP request lines; explicit `--keep-remote` stays loud.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn tree_hash(files: &[(&str, &str)]) -> String {
    let mut rows: Vec<String> = files
        .iter()
        .map(|(path, hash)| format!("{path}\0{hash}"))
        .collect();
    rows.sort();
    hash_bytes(rows.join("\n").as_bytes())
}

fn seed_home_skill(home: &Path, name: &str, body: &[u8]) -> String {
    let dir = home.join(".claude/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), body).unwrap();
    hash_bytes(body)
}

fn skl(home: &Path, api_base: &str, args: &[&str]) -> Output {
    let data = home.join(".local/share/skl");
    let config = home.join(".config/skl");
    Command::new(env!("CARGO_BIN_EXE_skl"))
        .arg("--api-base")
        .arg(api_base)
        .args(args)
        .env("HOME", home)
        .env("SKL_DATA_DIR", &data)
        .env("SKL_CONFIG_DIR", &config)
        .env("SKL_TOKEN", "dev:alice")
        .env_remove("SKL_TOKEN_FILE")
        .env("SKL_NO_PROMPT", "1")
        .env("SKL_NO_TUI", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run skl")
}

async fn skl_async(home: &Path, api_base: &str, args: &[&str]) -> Output {
    let home = home.to_path_buf();
    let api_base = api_base.to_string();
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        skl(&home, &api_base, &args)
    })
    .await
    .expect("join skl")
}

fn has_request_line(stderr: &str, api_base: &str) -> bool {
    let repost = format!("re-POST {api_base}/v1/sync");
    stderr.lines().any(|line| {
        line.starts_with("POST ")
            || line.starts_with("PUT /v1/")
            || line.starts_with("GET /v1/")
            || line.starts_with(&repost)
    })
}

fn stderr_text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_sync_hides_request_lines_but_still_transfers() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let body = b"# hello\n";
    let blob_hash = seed_home_skill(home, "greeter", body);
    let tree = tree_hash(&[("SKILL.md", blob_hash.as_str())]);

    Mock::given(method("POST"))
        .and(path("/v1/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "upload": [blob_hash],
            "download": [],
            "conflicts": [],
            "missing_skills": []
        })))
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path(format!("/v1/blobs/{blob_hash}")))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "hash": blob_hash,
            "size": body.len()
        })))
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/v1/skills/greeter/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "greeter",
            "tree_hash": tree,
            "updated_at": "2026-09-04T08:00:00.000Z"
        })))
        .mount(&server)
        .await;

    let out = skl_async(home, &server.uri(), &["init"]).await;
    let stderr = stderr_text(&out);
    assert!(
        out.status.success(),
        "stderr={stderr}\nstdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains("sync done  uploaded=1  downloaded=0  pushed=1  conflicts=0"),
        "{stderr}"
    );
    assert!(
        !has_request_line(&stderr, &server.uri()),
        "auto-sync must hide request lines:\n{stderr}"
    );

    let received = server.received_requests().await.unwrap();
    assert!(
        received
            .iter()
            .any(|r| r.method.as_str() == "POST" && r.url.path() == "/v1/sync"),
        "expected POST /v1/sync, got {received:?}"
    );
    assert!(
        received.iter().any(|r| {
            r.method.as_str() == "PUT" && r.url.path() == format!("/v1/blobs/{blob_hash}")
        }),
        "expected PUT /v1/blobs/{blob_hash}, got {received:?}"
    );
    assert!(
        received
            .iter()
            .any(|r| r.method.as_str() == "PUT" && r.url.path() == "/v1/skills/greeter/tree"),
        "expected PUT /v1/skills/greeter/tree, got {received:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_keep_remote_sync_keeps_request_lines() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    seed_home_skill(home, "greeter", b"# hello\n");

    Mock::given(method("POST"))
        .and(path("/v1/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "up_to_date": ["greeter"],
            "upload": [],
            "download": [],
            "conflicts": [],
            "missing_skills": []
        })))
        .mount(&server)
        .await;

    let init = skl_async(home, &server.uri(), &["init"]).await;
    assert!(init.status.success(), "stderr={}", stderr_text(&init));

    let out = skl_async(home, &server.uri(), &["sync", "--keep-remote"]).await;
    let stderr = stderr_text(&out);
    assert!(
        out.status.success(),
        "stderr={stderr}\nstdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains(&format!("POST {}/v1/sync  (1 skill(s))", server.uri())),
        "{stderr}"
    );
}
