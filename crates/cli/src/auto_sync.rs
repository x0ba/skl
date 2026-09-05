//! Piggyback auto-sync for furnace verbs.
//!
//! Hammer stacks smoke / docs / extra fail-soft / throttle cases on this API:
//!
//! - [`is_due`] — `last_sync_at` **and** `last_auto_sync_attempt_at` vs
//!   [`crate::config::SyncPrefs::frequency_secs`] (default 900s).
//! - [`maybe_run`] — skip / run / fail-soft. Never returns `Err`.
//! - Background conflicts use [`ConflictMode::KeepRemote`] (no TTY).
//!
//! Callers (`login` / `init` / `use` / `unuse` / `capture` / `status` / optional `list`)
//! ignore the result so the parent verb stays successful.

use crate::auth;
use crate::config::{self, Paths, SyncPrefs};
use crate::hooks::conflict::ConflictMode;
use crate::local::db::LocalDb;
use crate::sync::{self, SyncOptions, SyncOutcome};

/// Meta key: unix seconds of the last auto-sync *attempt* (success or fail).
pub const META_LAST_AUTO_SYNC_ATTEMPT_AT: &str = "last_auto_sync_attempt_at";

pub const SKIP_DISABLED: &str = "disabled";
pub const SKIP_NOT_DUE: &str = "not_due";
pub const SKIP_NO_INDEX: &str = "no_index";
pub const SKIP_NOT_LOGGED_IN: &str = "not_logged_in";

/// Result of a piggyback attempt. Parent verbs treat every variant as success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoSyncResult {
    Skipped { why: &'static str },
    Ran(SyncOutcome),
    FailedSoft { err: String },
}

/// True when auto-sync should start an attempt at `now` (unix seconds).
///
/// Due when:
/// - `prefs.auto` is true, and
/// - `last_sync_at` is missing or older than `prefs.frequency_secs`, and
/// - `last_auto_sync_attempt_at` is missing or older than `prefs.frequency_secs`
///   (failed attempts still throttle).
pub fn is_due(db: &LocalDb, prefs: &SyncPrefs, now: i64) -> bool {
    if !prefs.auto {
        return false;
    }
    let freq = prefs.frequency_secs as i64;
    if !stamp_is_stale(db.get_meta("last_sync_at").ok().flatten(), now, freq) {
        return false;
    }
    if !stamp_is_stale(
        db.get_meta(META_LAST_AUTO_SYNC_ATTEMPT_AT).ok().flatten(),
        now,
        freq,
    ) {
        return false;
    }
    true
}

fn stamp_is_stale(raw: Option<String>, now: i64, freq: i64) -> bool {
    match raw.and_then(|s| s.parse::<i64>().ok()) {
        Some(at) => now.saturating_sub(at) >= freq,
        None => true,
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Fail-soft piggyback. Loads `[sync]` from `paths`, then maybe runs hash-sync.
///
/// `reason` is the parent verb (`login`, `init`, `use`, `unuse`, `capture`, `status`, `list`).
pub async fn maybe_run(api_base: &str, paths: &Paths, reason: &str) -> AutoSyncResult {
    maybe_run_with(api_base, paths, reason, None).await
}

async fn maybe_run_with(
    api_base: &str,
    paths: &Paths,
    reason: &str,
    token_override: Option<&str>,
) -> AutoSyncResult {
    let prefs = match config::load(paths) {
        Ok(cfg) => cfg.sync,
        Err(err) => {
            return fail_soft(paths, reason, err.to_string());
        }
    };
    if !prefs.auto {
        return AutoSyncResult::Skipped { why: SKIP_DISABLED };
    }
    if !paths.db_file.exists() {
        return AutoSyncResult::Skipped { why: SKIP_NO_INDEX };
    }
    let db = match LocalDb::open(&paths.db_file) {
        Ok(db) => db,
        Err(err) => return fail_soft(paths, reason, err.to_string()),
    };
    let now = unix_now();
    if !is_due(&db, &prefs, now) {
        return AutoSyncResult::Skipped { why: SKIP_NOT_DUE };
    }
    let token = match token_override.map(str::to_string) {
        Some(token) => token,
        None => match auth::load_device_token() {
            Ok(token) => token,
            Err(_) => {
                return AutoSyncResult::Skipped {
                    why: SKIP_NOT_LOGGED_IN,
                };
            }
        },
    };
    if let Err(err) = db.set_meta(META_LAST_AUTO_SYNC_ATTEMPT_AT, &now.to_string()) {
        return fail_soft(paths, reason, err.to_string());
    }
    let home = match config::home_dir() {
        Ok(home) => home,
        Err(err) => return fail_soft(paths, reason, err.to_string()),
    };
    match sync::run_with_opts(
        api_base,
        &token,
        paths,
        &home,
        SyncOptions {
            conflict: ConflictMode::KeepRemote,
            allow_warnings: false,
        },
    )
    .await
    {
        Ok(outcome) => AutoSyncResult::Ran(outcome),
        Err(err) => fail_soft(paths, reason, err.to_string()),
    }
}

fn fail_soft(paths: &Paths, reason: &str, err: String) -> AutoSyncResult {
    if let Ok(db) = LocalDb::open(&paths.db_file) {
        let _ = db.record_sync_error(&err);
    }
    eprintln!("auto-sync ({reason}): {err} (ignored)");
    AutoSyncResult::FailedSoft { err }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Paths};
    use crate::local::db::LocalDb;
    use crate::local::linker;
    use crate::local::skills::{hash_skill_dir, DiscoveredSkill};
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn paths_for(tmp: &std::path::Path) -> Paths {
        Paths {
            config_dir: tmp.join("cfg"),
            config_file: tmp.join("cfg/config.toml"),
            data_dir: tmp.join("data"),
            db_file: tmp.join("data/state.db"),
        }
    }

    fn prefs(auto: bool, frequency_secs: u64) -> SyncPrefs {
        SyncPrefs {
            auto,
            frequency_secs,
        }
    }

    fn open_db(tmp: &std::path::Path) -> (Paths, LocalDb) {
        let paths = paths_for(tmp);
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let db = LocalDb::open(&paths.db_file).unwrap();
        (paths, db)
    }

    #[test]
    fn is_due_when_never_synced() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, db) = open_db(tmp.path());
        assert!(is_due(&db, &prefs(true, 900), 1_000_000));
    }

    #[test]
    fn is_due_false_when_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, db) = open_db(tmp.path());
        assert!(!is_due(&db, &prefs(false, 900), 1_000_000));
    }

    #[test]
    fn is_due_false_when_last_sync_recent() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, db) = open_db(tmp.path());
        db.set_meta("last_sync_at", "999200").unwrap();
        assert!(!is_due(&db, &prefs(true, 900), 1_000_000));
    }

    #[test]
    fn attempt_throttle_blocks_even_when_last_sync_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, db) = open_db(tmp.path());
        db.set_meta("last_sync_at", "1000").unwrap();
        db.set_meta(META_LAST_AUTO_SYNC_ATTEMPT_AT, "999200")
            .unwrap();
        assert!(!is_due(&db, &prefs(true, 900), 1_000_000));
        db.set_meta(META_LAST_AUTO_SYNC_ATTEMPT_AT, "998000")
            .unwrap();
        assert!(is_due(&db, &prefs(true, 900), 1_000_000));
    }

    #[test]
    fn is_due_at_exact_frequency_boundary() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, db) = open_db(tmp.path());
        db.set_meta("last_sync_at", "999100").unwrap();
        assert!(is_due(&db, &prefs(true, 900), 1_000_000));
    }

    #[tokio::test]
    async fn maybe_run_skips_disabled_and_missing_index() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_for(tmp.path());
        paths.ensure().unwrap();
        let mut cfg = Config::default();
        cfg.sync.auto = false;
        config::save(&paths, &cfg).unwrap();
        match maybe_run("http://127.0.0.1:1", &paths, "status").await {
            AutoSyncResult::Skipped { why } => assert_eq!(why, SKIP_DISABLED),
            other => panic!("{other:?}"),
        }

        cfg.sync.auto = true;
        config::save(&paths, &cfg).unwrap();
        match maybe_run("http://127.0.0.1:1", &paths, "status").await {
            AutoSyncResult::Skipped { why } => assert_eq!(why, SKIP_NO_INDEX),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn maybe_run_skips_when_not_due() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = open_db(tmp.path());
        paths.ensure().unwrap();
        config::save(&paths, &Config::default()).unwrap();
        let now = unix_now();
        db.set_meta("last_sync_at", &now.to_string()).unwrap();
        match maybe_run("http://127.0.0.1:1", &paths, "use").await {
            AutoSyncResult::Skipped { why } => assert_eq!(why, SKIP_NOT_DUE),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn maybe_run_fail_soft_writes_attempt_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = open_db(tmp.path());
        paths.ensure().unwrap();
        config::save(&paths, &Config::default()).unwrap();
        match maybe_run_with("http://127.0.0.1:1", &paths, "use", Some("dev:alice")).await {
            AutoSyncResult::FailedSoft { err } => {
                assert!(!err.is_empty(), "{err}");
            }
            other => panic!("{other:?}"),
        }
        let attempt = db
            .get_meta(META_LAST_AUTO_SYNC_ATTEMPT_AT)
            .unwrap()
            .expect("attempt stamp");
        assert!(attempt.parse::<i64>().unwrap() > 0);
        let issue = db.last_sync_error().unwrap().expect("sync issue");
        assert!(!issue.is_empty(), "{issue}");
        match maybe_run_with("http://127.0.0.1:1", &paths, "use", Some("dev:alice")).await {
            AutoSyncResult::Skipped { why } => assert_eq!(why, SKIP_NOT_DUE),
            other => panic!("second attempt should throttle, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fail_soft_does_not_poison_use() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("proj");
        let skill_dir = home.join(".claude/skills/greeter");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "hi").unwrap();
        std::fs::create_dir_all(&project).unwrap();

        let (paths, db) = open_db(tmp.path());
        paths.ensure().unwrap();
        config::save(&paths, &Config::default()).unwrap();
        let tree = hash_skill_dir(&skill_dir).unwrap();
        db.replace_import(&[DiscoveredSkill {
            name: "greeter".into(),
            source: "claude".into(),
            path: skill_dir.clone(),
            tree,
        }])
        .unwrap();

        let skill = crate::commands::use_cmd::resolve_skill("greeter", &home, Some(&paths.db_file))
            .unwrap();
        linker::activate_with_extras(&project, &home, &skill, &[]).unwrap();
        assert!(project.join(".agents/skills/greeter").exists());

        let piggyback =
            maybe_run_with("http://127.0.0.1:1", &paths, "use", Some("dev:alice")).await;
        assert!(
            matches!(piggyback, AutoSyncResult::FailedSoft { .. }),
            "{piggyback:?}"
        );
        assert!(
            project.join(".agents/skills/greeter").exists(),
            "use links must survive FailedSoft"
        );
        assert!(project.join("skills.toml").exists());
    }

    #[tokio::test]
    async fn maybe_run_ran_on_empty_sync() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = open_db(tmp.path());
        paths.ensure().unwrap();
        config::save(&paths, &Config::default()).unwrap();
        db.replace_import(&[]).unwrap();

        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .and(header("authorization", "Bearer dev:alice"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [],
                "missing_skills": []
            })))
            .mount(&server)
            .await;

        let result = maybe_run_with(&server.uri(), &paths, "status", Some("dev:alice")).await;
        match result {
            AutoSyncResult::Ran(outcome) => {
                assert!(outcome.uploaded.is_empty());
                assert_eq!(outcome.conflicts, 0);
            }
            other => panic!("{other:?}"),
        }
        assert!(db.last_sync_summary().unwrap().is_some());
    }
}
