//! `skl update` — replace this binary with the latest GitHub Release asset.

use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use reqwest::Client;
use serde::Deserialize;

use crate::error::{Result, SklError};
use crate::local::skills::{hash_bytes, normalize_hash};

const DEFAULT_DOWNLOAD_BASE: &str = "https://github.com/x0ba/skl/releases/latest/download";
const DEFAULT_RELEASES_API: &str = "https://api.github.com/repos/x0ba/skl/releases/latest";
const USER_AGENT: &str = concat!("skl/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct UpdateRequest {
    pub force: bool,
    pub dest: PathBuf,
    pub current_version: String,
    pub download_base: String,
    pub releases_api: String,
    pub target: String,
}

impl UpdateRequest {
    fn from_env(force: bool, dest: PathBuf) -> Result<Self> {
        Ok(Self {
            force,
            dest,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            download_base: env_or("SKL_DOWNLOAD_BASE", DEFAULT_DOWNLOAD_BASE),
            releases_api: env_or("SKL_RELEASES_API", DEFAULT_RELEASES_API),
            target: release_target()?.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    AlreadyCurrent {
        version: String,
        dest: PathBuf,
        checksum: String,
    },
    Updated {
        dest: PathBuf,
        from: String,
        to: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
}

pub async fn run(force: bool) -> Result<()> {
    let dest = current_binary()?;
    let outcome = apply(UpdateRequest::from_env(force, dest)?).await?;
    print_outcome(&outcome);
    Ok(())
}

pub async fn apply(req: UpdateRequest) -> Result<UpdateOutcome> {
    let http = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(USER_AGENT)
        .build()?;

    let asset = asset_name_for(&req.target);
    let download_base = trim_slash(&req.download_base);
    let sums_url = format!("{download_base}/SHA256SUMS");
    let asset_url = format!("{download_base}/{asset}");

    eprintln!(
        "==> current {} ({})",
        req.current_version,
        req.dest.display()
    );

    let latest_tag = fetch_latest_tag(&http, &req.releases_api).await;
    match &latest_tag {
        Ok(tag) => eprintln!("==> latest  {}", display_tag(tag)),
        Err(err) => eprintln!("==> latest  (GitHub Release; could not read tag: {err})"),
    }

    eprintln!("==> fetching SHA256SUMS");
    let sums = download_text(&http, &sums_url).await?;
    let expected = expected_checksum(&sums, &asset)
        .ok_or_else(|| SklError::Config(format!("SHA256SUMS has no entry for {asset}")))?;

    if !req.force {
        if let Some(current) = hash_if_exists(&req.dest)? {
            if current == expected {
                return Ok(UpdateOutcome::AlreadyCurrent {
                    version: req.current_version,
                    dest: req.dest,
                    checksum: expected,
                });
            }
        }
    } else {
        eprintln!("==> reinstalling (--force)");
    }

    eprintln!("==> downloading {asset_url}");
    let bytes = download_bytes(&http, &asset_url).await?;
    let actual = hash_bytes(&bytes);
    if actual != expected {
        return Err(SklError::Config(format!(
            "checksum mismatch for {asset} (got {actual}, want {expected})"
        )));
    }
    eprintln!("==> checksum ok ({asset})");

    replace_executable(&req.dest, &bytes)?;
    Ok(UpdateOutcome::Updated {
        dest: req.dest,
        from: req.current_version,
        to: latest_tag.ok().map(|tag| display_tag(&tag)),
    })
}

fn print_outcome(outcome: &UpdateOutcome) {
    match outcome {
        UpdateOutcome::AlreadyCurrent {
            version,
            dest,
            checksum,
        } => {
            eprintln!(
                "already up to date ({version})  {}  sha256={checksum}",
                dest.display()
            );
        }
        UpdateOutcome::Updated { dest, from, to } => {
            match to {
                Some(to) => eprintln!("==> updated {} ({from} -> {to})", dest.display()),
                None => eprintln!("==> updated {} (was {from})", dest.display()),
            }
            if let Some(reported) = probe_version(dest) {
                eprintln!("    {reported}");
            }
        }
    }
}

pub fn release_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-musl"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-gnu"),
        (os, arch) => Err(SklError::Config(format!(
            "unsupported platform {os}/{arch} for `skl update` (need a GitHub Release triple)"
        ))),
    }
}

pub fn asset_name_for(target: &str) -> String {
    if target.contains("-pc-windows-") {
        format!("skl-{target}.exe")
    } else {
        format!("skl-{target}")
    }
}

pub fn expected_checksum(sums: &str, filename: &str) -> Option<String> {
    for line in sums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name == filename {
            return Some(normalize_hash(hash));
        }
    }
    None
}

pub fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

fn current_binary() -> Result<PathBuf> {
    let dest = std::env::current_exe()
        .map_err(|err| SklError::Config(format!("cannot resolve this skl binary: {err}")))?;
    Ok(dest.canonicalize().unwrap_or(dest))
}

fn hash_if_exists(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(hash_bytes(&fs::read(path)?)))
}

fn replace_executable(dest: &Path, bytes: &[u8]) -> Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| SklError::Config(format!("cannot update {}", dest.display())))?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }

    // Hold the lock for the whole staging + rename so two updaters cannot
    // interleave mutations of the destination.
    let _lock = acquire_update_lock(dest)?;
    let tmp = write_staging_file(dest, bytes)?;

    if let Err(err) = apply_exec_perms(&tmp, dest.exists().then_some(dest)) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = ensure_runnable(&tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(SklError::Config(format!(
            "downloaded update is not runnable; left {} unchanged ({err})",
            dest.display()
        )));
    }

    if let Err(err) = install_staged(dest, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

fn acquire_update_lock(dest: &Path) -> Result<File> {
    let path = sibling(dest, ".lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)?;
    file.lock().map_err(|err| {
        SklError::Config(format!("cannot lock {} for update: {err}", path.display()))
    })?;
    Ok(file)
}

fn write_staging_file(dest: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let pid = std::process::id();
    for seq in 0u32..1024 {
        let tmp = sibling(dest, &format!(".tmp.{pid}.{seq}"));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => {
                // Close the create handle before writing so Linux execve of the
                // staged file cannot fail with ETXTBSY (text file busy).
                drop(file);
                fs::write(&tmp, bytes)?;
                return Ok(tmp);
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(SklError::Config(format!(
        "cannot create a unique staging file next to {}",
        dest.display()
    )))
}

fn install_staged(dest: &Path, tmp: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let old = sibling(dest, &format!(".old.{}", std::process::id()));
        if old.exists() {
            let _ = fs::remove_file(&old);
        }
        if dest.exists() {
            fs::rename(dest, &old)?;
        }
        if let Err(err) = fs::rename(tmp, dest) {
            if old.exists() {
                let _ = fs::rename(&old, dest);
            }
            return Err(err.into());
        }
        let _ = fs::remove_file(&old);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(tmp, dest)?;
        Ok(())
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn apply_exec_perms(path: &Path, like: Option<&Path>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = like
            .and_then(|src| fs::metadata(src).ok())
            .map(|meta| meta.permissions().mode())
            .unwrap_or(0o755)
            | 0o111;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        let _ = like;
    }
    Ok(())
}

async fn fetch_latest_tag(http: &Client, url: &str) -> Result<String> {
    let bytes = download_bytes(http, url).await?;
    let release: GithubRelease = serde_json::from_slice(&bytes)?;
    if release.tag_name.trim().is_empty() {
        return Err(SklError::Config(
            "latest release is missing tag_name".into(),
        ));
    }
    Ok(release.tag_name)
}

async fn download_text(http: &Client, url: &str) -> Result<String> {
    let bytes = download_bytes(http, url).await?;
    String::from_utf8(bytes)
        .map_err(|err| SklError::Config(format!("invalid UTF-8 from {url}: {err}")))
}

async fn download_bytes(http: &Client, url: &str) -> Result<Vec<u8>> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|err| SklError::ApiUnreachable {
            url: url.to_string(),
            source: err.to_string(),
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SklError::Api {
            status: status.as_u16(),
            body: if body.trim().is_empty() {
                format!("GET {url}")
            } else {
                body
            },
        });
    }
    Ok(response.bytes().await?.to_vec())
}

fn probe_version(dest: &Path) -> Option<String> {
    let output = ensure_runnable(dest).ok()?;
    output
}

fn ensure_runnable(path: &Path) -> Result<Option<String>> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|err| SklError::Config(format!("new binary could not be started: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("no output");
        return Err(SklError::Config(format!(
            "new binary failed `--version` ({detail})"
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().map(str::trim).unwrap_or_default();
    if line.is_empty() {
        Ok(None)
    } else {
        Ok(Some(line.to_string()))
    }
}

fn display_tag(tag: &str) -> String {
    match parse_version(tag) {
        Some((major, minor, patch)) => format!("{major}.{minor}.{patch}"),
        None => tag.trim().trim_start_matches('v').to_string(),
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn trim_slash(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn checksum_parser_accepts_gnu_and_bsd_lines() {
        let sums = "\
# comment
abcDEF0123456789  skl-x86_64-unknown-linux-musl
deadbeef  *skl-aarch64-unknown-linux-musl
";
        assert_eq!(
            expected_checksum(sums, "skl-x86_64-unknown-linux-musl").as_deref(),
            Some("abcdef0123456789")
        );
        assert_eq!(
            expected_checksum(sums, "skl-aarch64-unknown-linux-musl").as_deref(),
            Some("deadbeef")
        );
        assert!(expected_checksum(sums, "skl-missing").is_none());
    }

    #[test]
    fn parse_version_strips_v_and_prerelease() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v2.0.0-rc.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("v3"), Some((3, 0, 0)));
        assert!(parse_version("not-a-version").is_none());
    }

    #[test]
    fn asset_names_match_installer() {
        assert_eq!(
            asset_name_for("x86_64-unknown-linux-musl"),
            "skl-x86_64-unknown-linux-musl"
        );
        assert_eq!(
            asset_name_for("x86_64-pc-windows-gnu"),
            "skl-x86_64-pc-windows-gnu.exe"
        );
    }

    #[test]
    fn host_target_is_a_release_triple() {
        let target = release_target().unwrap();
        assert!(
            target.contains("linux") || target.contains("apple") || target.contains("windows"),
            "{target}"
        );
        assert!(asset_name_for(target).starts_with("skl-"));
    }

    #[cfg(unix)]
    fn unix_cli_shim(version: &str) -> Vec<u8> {
        format!("#!/bin/sh\necho 'skl {version}'\n").into_bytes()
    }

    #[cfg(unix)]
    fn write_unix_cli_shim(dest: &Path, version: &str) -> Vec<u8> {
        use std::os::unix::fs::PermissionsExt;
        let bytes = unix_cli_shim(version);
        fs::write(dest, &bytes).unwrap();
        fs::set_permissions(dest, fs::Permissions::from_mode(0o755)).unwrap();
        bytes
    }

    #[cfg(unix)]
    fn staging_leftovers(dest: &Path) -> Vec<PathBuf> {
        let name = dest.file_name().unwrap().to_string_lossy();
        let prefix = format!("{name}.tmp.");
        fs::read_dir(dest.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .map(|n| n.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false)
            })
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn replace_executable_overwrites_dest() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("skl");
        write_unix_cli_shim(&dest, "0.1.0");
        let next = unix_cli_shim("9.9.9");
        replace_executable(&dest, &next).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), next);
        assert!(staging_leftovers(&dest).is_empty());
        assert_eq!(probe_version(&dest).as_deref(), Some("skl 9.9.9"));
    }

    #[cfg(unix)]
    #[test]
    fn unusable_download_leaves_existing_binary() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("skl");
        let current = write_unix_cli_shim(&dest, "0.1.0");
        let err = replace_executable(&dest, b"not-an-executable").unwrap_err();
        assert!(err.to_string().contains("not runnable"), "{err}");
        assert_eq!(fs::read(&dest).unwrap(), current);
        assert!(staging_leftovers(&dest).is_empty());
        assert_eq!(probe_version(&dest).as_deref(), Some("skl 0.1.0"));
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_replaces_use_unique_staging_and_keep_a_runnable_dest() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("skl");
        write_unix_cli_shim(&dest, "0.0.0");
        let first = unix_cli_shim("1.0.0");
        let second = unix_cli_shim("2.0.0");
        std::thread::scope(|scope| {
            scope.spawn(|| replace_executable(&dest, &first).unwrap());
            scope.spawn(|| replace_executable(&dest, &second).unwrap());
        });
        let got = fs::read(&dest).unwrap();
        assert!(
            got == first || got == second,
            "dest was neither payload: {}",
            String::from_utf8_lossy(&got)
        );
        assert!(staging_leftovers(&dest).is_empty());
        let version = probe_version(&dest).expect("dest must still launch");
        assert!(
            version == "skl 1.0.0" || version == "skl 2.0.0",
            "{version}"
        );
    }

    #[tokio::test]
    async fn skips_when_checksum_already_matches() {
        let server = MockServer::start().await;
        let payload = b"already-installed";
        let hash = hash_bytes(payload);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skl");
        fs::write(&dest, payload).unwrap();

        Mock::given(method("GET"))
            .and(path("/SHA256SUMS"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("{hash}  skl-test-triple\n")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v0.1.0"
            })))
            .mount(&server)
            .await;

        let outcome = apply(UpdateRequest {
            force: false,
            dest: dest.clone(),
            current_version: "0.1.0".into(),
            download_base: server.uri(),
            releases_api: format!("{}/repos/latest", server.uri()),
            target: "test-triple".into(),
        })
        .await
        .unwrap();

        match outcome {
            UpdateOutcome::AlreadyCurrent { checksum, .. } => assert_eq!(checksum, hash),
            other => panic!("expected already current, got {other:?}"),
        }
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[tokio::test]
    async fn downloads_and_replaces_when_checksum_differs() {
        let server = MockServer::start().await;
        let payload: &'static [u8] = {
            #[cfg(unix)]
            {
                b"#!/bin/sh\necho 'skl 9.9.9'\n"
            }
            #[cfg(not(unix))]
            {
                b"fresh-release-bytes"
            }
        };
        let hash = hash_bytes(payload);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skl");
        fs::write(&dest, b"stale").unwrap();

        Mock::given(method("GET"))
            .and(path("/SHA256SUMS"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("{hash}  skl-test-triple\n")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/skl-test-triple"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(payload))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v9.9.9"
            })))
            .mount(&server)
            .await;

        let outcome = apply(UpdateRequest {
            force: false,
            dest: dest.clone(),
            current_version: "0.1.0".into(),
            download_base: server.uri(),
            releases_api: format!("{}/repos/latest", server.uri()),
            target: "test-triple".into(),
        })
        .await
        .unwrap();

        match outcome {
            UpdateOutcome::Updated { from, to, .. } => {
                assert_eq!(from, "0.1.0");
                assert_eq!(to.as_deref(), Some("9.9.9"));
            }
            other => panic!("expected updated, got {other:?}"),
        }
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[tokio::test]
    async fn force_redownloads_matching_checksum() {
        let server = MockServer::start().await;
        let payload: &'static [u8] = {
            #[cfg(unix)]
            {
                b"#!/bin/sh\necho 'skl 0.1.0'\n"
            }
            #[cfg(not(unix))]
            {
                b"reinstall-me"
            }
        };
        let hash = hash_bytes(payload);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skl");
        fs::write(&dest, payload).unwrap();

        Mock::given(method("GET"))
            .and(path("/SHA256SUMS"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("{hash}  skl-test-triple\n")),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/skl-test-triple"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(payload))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v0.1.0"
            })))
            .mount(&server)
            .await;

        let outcome = apply(UpdateRequest {
            force: true,
            dest: dest.clone(),
            current_version: "0.1.0".into(),
            download_base: server.uri(),
            releases_api: format!("{}/repos/latest", server.uri()),
            target: "test-triple".into(),
        })
        .await
        .unwrap();

        assert!(matches!(outcome, UpdateOutcome::Updated { .. }));
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_unusable_download_and_keeps_existing() {
        let server = MockServer::start().await;
        let payload = b"checksum-valid-but-not-a-binary";
        let hash = hash_bytes(payload);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skl");
        let current = write_unix_cli_shim(&dest, "0.1.0");

        Mock::given(method("GET"))
            .and(path("/SHA256SUMS"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("{hash}  skl-test-triple\n")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/skl-test-triple"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(payload))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v9.9.9"
            })))
            .mount(&server)
            .await;

        let err = apply(UpdateRequest {
            force: true,
            dest: dest.clone(),
            current_version: "0.1.0".into(),
            download_base: server.uri(),
            releases_api: format!("{}/repos/latest", server.uri()),
            target: "test-triple".into(),
        })
        .await
        .unwrap_err();

        assert!(err.to_string().contains("not runnable"), "{err}");
        assert_eq!(fs::read(&dest).unwrap(), current);
        assert_eq!(probe_version(&dest).as_deref(), Some("skl 0.1.0"));
    }

    #[tokio::test]
    async fn refuses_checksum_mismatch_and_keeps_existing() {
        let server = MockServer::start().await;
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skl");
        fs::write(&dest, b"keep-me").unwrap();

        Mock::given(method("GET"))
            .and(path("/SHA256SUMS"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "0000000000000000000000000000000000000000000000000000000000000000  skl-test-triple\n",
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/skl-test-triple"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v1.0.0"
            })))
            .mount(&server)
            .await;

        let err = apply(UpdateRequest {
            force: true,
            dest: dest.clone(),
            current_version: "0.1.0".into(),
            download_base: server.uri(),
            releases_api: format!("{}/repos/latest", server.uri()),
            target: "test-triple".into(),
        })
        .await
        .unwrap_err();

        assert!(err.to_string().contains("checksum mismatch"), "{err}");
        assert_eq!(fs::read(&dest).unwrap(), b"keep-me");
    }

    #[tokio::test]
    async fn missing_sums_entry_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/SHA256SUMS"))
            .respond_with(ResponseTemplate::new(200).set_body_string("abc  other-file\n"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v1.0.0"
            })))
            .mount(&server)
            .await;

        let dest = tempfile::tempdir().unwrap().path().join("skl");
        let err = apply(UpdateRequest {
            force: false,
            dest,
            current_version: "0.1.0".into(),
            download_base: server.uri(),
            releases_api: format!("{}/repos/latest", server.uri()),
            target: "test-triple".into(),
        })
        .await
        .unwrap_err();
        assert!(err.to_string().contains("no entry"), "{err}");
    }
}
