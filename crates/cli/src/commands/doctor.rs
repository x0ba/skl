//! `skl doctor` — agent skill paths, keyring/config/state.db, API health.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::api::ApiClient;
use crate::auth::{self, TokenPresence, KEYRING_ACCOUNT, KEYRING_SERVICE, TOKEN_ENV};
use crate::config::{self, Paths, SkillRoot};
use crate::error::Result;
use crate::local::db::{LocalDb, SyncSummary};
use crate::local::linker::{self, WINDOWS_SYMLINK_NOTE};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub writable: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootStatus {
    pub source: String,
    pub status: PathStatus,
    pub skill_count: Option<u64>,
    /// `None` when the root is missing (nothing to probe).
    pub symlink: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Ok,
    Unreachable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub api_base: String,
    pub health: HealthStatus,
    pub token: TokenPresence,
    pub token_env_set: bool,
    pub keyring_service: String,
    pub keyring_account: String,
    pub config: Option<PathStatus>,
    pub state_db: Option<PathStatus>,
    pub local_skills: Option<u64>,
    /// Last successful `skl sync` / auto-sync. Display only — doctor never calls maybe_run.
    pub last_sync: Option<(i64, SyncSummary)>,
    pub roots: Vec<RootStatus>,
    pub symlink: bool,
    pub symlink_detail: String,
    pub windows_note: Option<String>,
    /// Activated skill modes from cwd `skills.toml` (last symlink vs copy).
    pub project_manifest: Option<PathBuf>,
    pub project_modes: Vec<(String, String)>,
    /// Warn-only: M0 layout needs `skl migrate targets` (never auto-mutates).
    pub m0_targets_warning: Option<String>,
    /// Warn-only: `skills.toml` still lists host-absolute `path`s.
    pub absolute_paths_warning: Option<String>,
}

pub async fn run(api_base: String) -> Result<()> {
    let home = config::home_dir()?;
    let paths = Paths::resolve().ok();
    let report = collect(&api_base, &home, paths.as_ref()).await;
    print_report(&report);
    if let Some(paths) = paths.as_ref() {
        config::maybe_prompt_sticky_extras(paths)?;
    }
    Ok(())
}

pub async fn collect(api_base: &str, home: &Path, paths: Option<&Paths>) -> DoctorReport {
    let health = probe_health(api_base).await;
    let token = auth::token_presence();
    let token_env_set = std::env::var(TOKEN_ENV)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    let (config, state_db, local_skills, last_sync) = match paths {
        Some(paths) => {
            let config = inspect_file(&paths.config_file);
            let state_db = inspect_file(&paths.db_file);
            let (local_skills, last_sync) = if paths.db_file.exists() {
                match LocalDb::open(&paths.db_file) {
                    Ok(db) => (db.skill_count().ok(), db.last_sync_summary().ok().flatten()),
                    Err(_) => (None, None),
                }
            } else {
                (None, None)
            };
            (Some(config), Some(state_db), local_skills, last_sync)
        }
        None => (None, None, None, None),
    };

    let roots = config::skill_roots(home)
        .into_iter()
        .map(|root| inspect_root(root))
        .collect();

    let probe = linker::probe_symlink(&std::env::temp_dir());
    let windows_note = if cfg!(windows) || !probe.ok {
        Some(WINDOWS_SYMLINK_NOTE.to_string())
    } else {
        None
    };
    let (project_manifest, project_modes, m0_targets_warning, absolute_paths_warning) =
        project_link_modes();

    DoctorReport {
        api_base: api_base.to_string(),
        health,
        token,
        token_env_set,
        keyring_service: KEYRING_SERVICE.to_string(),
        keyring_account: KEYRING_ACCOUNT.to_string(),
        config,
        state_db,
        local_skills,
        last_sync,
        roots,
        symlink: probe.ok,
        symlink_detail: probe.detail,
        windows_note,
        project_manifest,
        project_modes,
        m0_targets_warning,
        absolute_paths_warning,
    }
}

async fn probe_health(api_base: &str) -> HealthStatus {
    let client = match ApiClient::new(api_base) {
        Ok(client) => client,
        Err(err) => return HealthStatus::Unreachable(err.to_string()),
    };
    match tokio::time::timeout(HEALTH_TIMEOUT, client.health()).await {
        Ok(Ok(body)) if body.ok => HealthStatus::Ok,
        Ok(Ok(_)) => HealthStatus::Unreachable("GET /v1/health did not return {ok:true}".into()),
        Ok(Err(err)) => HealthStatus::Unreachable(err.to_string()),
        Err(_) => HealthStatus::Unreachable("timed out after 3s".into()),
    }
}

fn inspect_root(root: SkillRoot) -> RootStatus {
    let skill_count = if root.path.is_dir() {
        Some(count_skill_dirs(&root.path))
    } else {
        None
    };
    let status = inspect_dir(&root.path);
    let symlink = if status.exists && root.path.is_dir() {
        Some(linker::probe_symlink(&root.path).ok)
    } else {
        None
    };
    RootStatus {
        source: root.source.to_string(),
        status,
        skill_count,
        symlink,
    }
}

fn project_link_modes() -> (
    Option<PathBuf>,
    Vec<(String, String)>,
    Option<String>,
    Option<String>,
) {
    let Ok(cwd) = std::env::current_dir() else {
        return (None, Vec::new(), None, None);
    };
    let warning = linker::m0_targets_warning(&cwd);
    let abs_warning = linker::absolute_paths_warning(&cwd);
    let path = linker::manifest_path(&cwd);
    if !path.exists() {
        return (None, Vec::new(), warning, abs_warning);
    }
    let Ok(manifest) = linker::load_manifest(&cwd) else {
        return (Some(path), Vec::new(), warning, abs_warning);
    };
    let modes = manifest
        .skills
        .into_iter()
        .map(|skill| (skill.name, skill.mode))
        .collect();
    (Some(path), modes, warning, abs_warning)
}

fn inspect_dir(path: &Path) -> PathStatus {
    if path.is_dir() {
        PathStatus {
            path: path.to_path_buf(),
            exists: true,
            writable: is_writable(path),
            detail: "dir".into(),
        }
    } else if path.exists() {
        PathStatus {
            path: path.to_path_buf(),
            exists: true,
            writable: is_writable(path),
            detail: "not a directory".into(),
        }
    } else {
        let parent_writable = path
            .parent()
            .map(|parent| parent.is_dir() && is_writable(parent))
            .unwrap_or(false);
        PathStatus {
            path: path.to_path_buf(),
            exists: false,
            writable: parent_writable,
            detail: if parent_writable {
                "missing (parent writable)".into()
            } else {
                "missing".into()
            },
        }
    }
}

fn inspect_file(path: &Path) -> PathStatus {
    if path.exists() {
        PathStatus {
            path: path.to_path_buf(),
            exists: true,
            writable: is_writable(path),
            detail: "exists".into(),
        }
    } else {
        let parent_writable = path
            .parent()
            .map(|parent| parent.is_dir() && is_writable(parent))
            .unwrap_or(false);
        PathStatus {
            path: path.to_path_buf(),
            exists: false,
            writable: parent_writable,
            detail: if parent_writable {
                "missing (parent writable)".into()
            } else {
                "missing".into()
            },
        }
    }
}

fn is_writable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => !meta.permissions().readonly(),
        Err(_) => false,
    }
}

fn count_skill_dirs(root: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| !name.starts_with('.'))
                .unwrap_or(false)
        })
        .count() as u64
}

fn print_report(report: &DoctorReport) {
    println!("== API");
    println!("api_base     {}", report.api_base);
    match &report.health {
        HealthStatus::Ok => println!("health       ok  GET /v1/health {{ok:true}}"),
        HealthStatus::Unreachable(err) => {
            println!("health       unreachable  GET /v1/health  {err}")
        }
    }

    println!();
    println!("== Auth");
    let env_note = if report.token_env_set {
        format!("  ({TOKEN_ENV} override)")
    } else {
        String::new()
    };
    match &report.token {
        TokenPresence::Present { preview } => {
            println!(
                "keyring      present  service={} account={}  token={preview}{env_note}",
                report.keyring_service, report.keyring_account
            );
        }
        TokenPresence::Absent => {
            println!(
                "keyring      absent   service={} account={}  (run `skl login`){env_note}",
                report.keyring_service, report.keyring_account
            );
        }
        TokenPresence::Error(msg) => {
            println!(
                "keyring      error    service={} account={}  {msg}{env_note}",
                report.keyring_service, report.keyring_account
            );
        }
    }

    println!();
    println!("== State");
    match &report.config {
        Some(status) => println!(
            "config       {}  {}  writable={}",
            status.path.display(),
            status.detail,
            yn(status.writable)
        ),
        None => println!("config       (cannot resolve XDG config dir)"),
    }
    match &report.state_db {
        Some(status) => {
            let skills = match report.local_skills {
                Some(n) => format!("  skills={n}"),
                None if status.exists => "  skills=?".into(),
                None => String::new(),
            };
            println!(
                "state.db     {}  {}  writable={}{skills}",
                status.path.display(),
                status.detail,
                yn(status.writable)
            );
        }
        None => println!("state.db     (cannot resolve XDG data dir)"),
    }
    match &report.last_sync {
        Some((at, summary)) => println!(
            "last_sync    at={at}  uploaded={}  downloaded={}  pushed={}  conflicts={}  missing_skills={}",
            summary.uploaded,
            summary.downloaded,
            summary.pushed,
            summary.conflicts,
            summary.missing_skills
        ),
        None => println!("last_sync    (none)"),
    }

    println!();
    println!("== Agent skill roots");
    for root in &report.roots {
        let skills = match root.skill_count {
            Some(n) => format!("  skills={n}"),
            None => String::new(),
        };
        let writable = if root.status.exists {
            format!("writable={}", yn(root.status.writable))
        } else {
            root.status.detail.clone()
        };
        let link = match root.symlink {
            Some(true) => "  symlink=yes",
            Some(false) => "  symlink=no (use will copy)",
            None => "",
        };
        println!(
            "{:<18} {}  {}{skills}{link}",
            root.source,
            root.status.path.display(),
            writable
        );
    }

    println!();
    println!("== Linking");
    if report.symlink {
        println!("symlink      ok");
    } else {
        println!(
            "symlink      unavailable  {}  → use will copy",
            report.symlink_detail
        );
    }
    if let Some(note) = &report.windows_note {
        println!("windows      {note}");
    }
    match (&report.project_manifest, report.project_modes.is_empty()) {
        (Some(path), true) => println!("project      {}  (no activated skills)", path.display()),
        (Some(path), false) => {
            let modes = report
                .project_modes
                .iter()
                .map(|(name, mode)| format!("{name}={mode}"))
                .collect::<Vec<_>>()
                .join("  ");
            println!("project      {}  {modes}", path.display());
        }
        (None, _) => {}
    }
    if let Some(warning) = &report.m0_targets_warning {
        println!("warn         {warning}");
    }
    if let Some(warning) = &report.absolute_paths_warning {
        println!("warn         {warning}");
    }
}

fn yn(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::db::LocalDb;
    use crate::local::skills::{hash_skill_dir, DiscoveredSkill};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn reports_existing_roots_and_health() {
        let home = tempfile::tempdir().unwrap();
        let claude = home.path().join(".claude/skills/foo");
        fs::create_dir_all(&claude).unwrap();
        fs::write(claude.join("SKILL.md"), "foo").unwrap();
        fs::create_dir_all(home.path().join(".cursor/skills")).unwrap();
        let agents = home.path().join(".agents/skills/greeter");
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("SKILL.md"), "hi").unwrap();

        let data = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: data.path().join("cfg"),
            config_file: data.path().join("cfg/config.toml"),
            data_dir: data.path().join("data"),
            db_file: data.path().join("data/state.db"),
        };
        paths.ensure().unwrap();
        let tree = hash_skill_dir(&claude).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "foo".into(),
            source: "claude".into(),
            path: claude,
            tree,
        }])
        .unwrap();

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
            .mount(&server)
            .await;

        let report = collect(&server.uri(), home.path(), Some(&paths)).await;
        assert_eq!(report.health, HealthStatus::Ok);
        assert_eq!(report.local_skills, Some(1));
        assert!(report.last_sync.is_none());
        assert!(report.roots.len() > 5);
        let claude_root = report
            .roots
            .iter()
            .find(|r| r.source == "claude-code")
            .unwrap();
        assert!(claude_root.status.exists);
        assert_eq!(claude_root.skill_count, Some(1));
        assert_eq!(claude_root.symlink, Some(true));
        let cursor_root = report.roots.iter().find(|r| r.source == "cursor").unwrap();
        assert!(cursor_root.status.exists);
        assert_eq!(cursor_root.skill_count, Some(0));
        assert_eq!(cursor_root.symlink, Some(true));
        let codex_root = report.roots.iter().find(|r| r.source == "codex").unwrap();
        assert!(!codex_root.status.exists);
        assert_eq!(codex_root.symlink, None);
        let agents_root = report.roots.iter().find(|r| r.source == "agents").unwrap();
        assert!(agents_root.status.exists);
        assert_eq!(agents_root.skill_count, Some(1));
        assert_eq!(agents_root.symlink, Some(true));
        let xdg_root = report
            .roots
            .iter()
            .find(|r| r.source == "xdg-agents")
            .unwrap();
        assert!(!xdg_root.status.exists);
        assert_eq!(xdg_root.symlink, None);
        assert!(!report.config.as_ref().unwrap().exists);
        assert!(report.state_db.as_ref().unwrap().exists);
        assert!(report.symlink, "host temp dir should allow symlinks");
        assert_eq!(report.symlink_detail, "ok");
        assert!(report.windows_note.is_none());
        assert!(report.m0_targets_warning.is_none());
    }

    #[tokio::test]
    async fn reports_both_universal_home_roots_when_planted() {
        let home = tempfile::tempdir().unwrap();
        let agents = home.path().join(".agents/skills/greeter");
        let xdg = home.path().join(".config/agents/skills/notes");
        fs::create_dir_all(&agents).unwrap();
        fs::create_dir_all(&xdg).unwrap();
        fs::write(agents.join("SKILL.md"), "hi").unwrap();
        fs::write(xdg.join("SKILL.md"), "notes").unwrap();

        let report = collect("http://127.0.0.1:1", home.path(), None).await;
        assert!(report.roots.len() > 5);
        let agents_root = report.roots.iter().find(|r| r.source == "agents").unwrap();
        assert!(agents_root.status.exists);
        assert_eq!(agents_root.skill_count, Some(1));
        assert_eq!(
            agents_root.status.path,
            home.path().join(".agents").join("skills")
        );
        let xdg_root = report
            .roots
            .iter()
            .find(|r| r.source == "xdg-agents")
            .unwrap();
        assert!(xdg_root.status.exists);
        assert_eq!(xdg_root.skill_count, Some(1));
        assert_eq!(
            xdg_root.status.path,
            home.path().join(".config").join("agents").join("skills")
        );
        let claude_root = report
            .roots
            .iter()
            .find(|r| r.source == "claude-code")
            .unwrap();
        assert!(!claude_root.status.exists);
        assert!(report.roots.iter().any(|r| r.source == "windsurf"));
    }

    #[tokio::test]
    async fn health_unreachable_when_server_down() {
        let home = tempfile::tempdir().unwrap();
        let report = collect("http://127.0.0.1:1", home.path(), None).await;
        match report.health {
            HealthStatus::Unreachable(msg) => assert!(!msg.is_empty(), "{msg}"),
            HealthStatus::Ok => panic!("expected unreachable"),
        }
        assert!(report.roots.len() > 5);
        assert!(report.roots.iter().any(|r| r.source == "windsurf"));
    }
}
