use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog;
use crate::checklist;
use crate::error::{Result, SklError};
use crate::local::linker;

/// Default API origin (`apps/api` listens on PORT=8787).
/// Override with `--api-base` or `API_BASE`.
pub const DEFAULT_API_BASE: &str = "http://localhost:8787";

/// Default piggyback interval (15 minutes). Also the attempt throttle.
pub const DEFAULT_SYNC_FREQUENCY_SECS: u64 = 900;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub api_base: Option<String>,
    /// Piggyback auto-sync (`[sync]` in `~/.config/skl/config.toml`).
    /// Defaults: `auto = true`, `frequency_secs = 900`.
    #[serde(default)]
    pub sync: SyncPrefs,
    /// Sticky extra dests for `skl use` (`~/.config/skl/config.toml`).
    #[serde(default, skip_serializing_if = "TargetPrefs::is_unset")]
    pub targets: TargetPrefs,
}

/// `[sync]` — piggyback hash-sync on login/init/use/unuse/status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncPrefs {
    /// When false, `maybe_run` always skips (explicit `skl sync` still works).
    #[serde(default = "default_sync_auto")]
    pub auto: bool,
    /// Minimum seconds between a successful sync *or* a failed attempt.
    #[serde(default = "default_sync_frequency_secs")]
    pub frequency_secs: u64,
}

impl Default for SyncPrefs {
    fn default() -> Self {
        Self {
            auto: default_sync_auto(),
            frequency_secs: default_sync_frequency_secs(),
        }
    }
}

fn default_sync_auto() -> bool {
    true
}

fn default_sync_frequency_secs() -> u64 {
    DEFAULT_SYNC_FREQUENCY_SECS
}

/// User-level extra link targets. Canonical `agents` is never stored here.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TargetPrefs {
    /// Extra dests (custom catalog ids, e.g. `claude-code`). Empty = `.agents/skills` only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
    /// Soft-prompt on `init`/`doctor` already shown (or declined).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prompted: bool,
}

impl TargetPrefs {
    fn is_unset(&self) -> bool {
        self.extra.is_empty() && !self.prompted
    }
}

impl Config {
    pub fn api_base(&self) -> String {
        self.api_base
            .clone()
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
    }

    pub fn sticky_extras(&self) -> Vec<String> {
        linker::filter_extra_ids(&self.targets.extra)
    }
}

pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        let config_dir = match std::env::var_os("SKL_CONFIG_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => dirs::config_dir()
                .ok_or_else(|| SklError::Config("cannot resolve XDG config dir".into()))?
                .join("skl"),
        };
        let data_dir = match std::env::var_os("SKL_DATA_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => dirs::data_dir()
                .ok_or_else(|| SklError::Config("cannot resolve XDG data dir".into()))?
                .join("skl"),
        };
        Ok(Self {
            config_file: config_dir.join("config.toml"),
            db_file: data_dir.join("state.db"),
            config_dir,
            data_dir,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    /// Canonical personal skill library: `{data_dir}/skills`.
    ///
    /// Default `data_dir` is `~/.local/share/skl` (XDG), so the library is
    /// `~/.local/share/skl/skills/`. `SKL_DATA_DIR` overrides the data dir.
    /// This is **not** `~/.agents/skills` (project link dest / init discovery).
    pub fn library_dir(&self) -> PathBuf {
        self.data_dir.join("skills")
    }

    /// `{data_dir}/skills/<name>`.
    pub fn library_skill(&self, name: &str) -> PathBuf {
        self.library_dir().join(name)
    }
}

pub fn load(paths: &Paths) -> Result<Config> {
    if !paths.config_file.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(&paths.config_file)?;
    let mut cfg: Config = toml::from_str(&raw).map_err(|err| SklError::Config(err.to_string()))?;
    let (extras, warns) = linker::migrate_extra_ids(&cfg.targets.extra);
    if extras != cfg.targets.extra {
        for warn in &warns {
            eprintln!("warn: {warn}");
        }
        cfg.targets.extra = extras;
        let _ = save(paths, &cfg);
    }
    Ok(cfg)
}

pub fn save(paths: &Paths, config: &Config) -> Result<()> {
    paths.ensure()?;
    let raw = toml::to_string_pretty(config).map_err(|err| SklError::Config(err.to_string()))?;
    fs::write(&paths.config_file, raw)?;
    Ok(())
}

/// Effective API base: flag/env wins, then config, then default.
pub fn resolve_api_base(cli_or_env: Option<&str>, config: &Config) -> String {
    if let Some(value) = cli_or_env {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trim_trailing_slash(trimmed);
        }
    }
    trim_trailing_slash(&config.api_base())
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

/// Soft-prompt extras once. Skip in CI / non-TTY / yes-mode so `init`/`doctor` never hang.
pub fn maybe_prompt_sticky_extras(paths: &Paths) -> Result<Config> {
    let mut cfg = load(paths).unwrap_or_default();
    if cfg.targets.prompted || !cfg.targets.extra.is_empty() {
        return Ok(cfg);
    }
    if !should_prompt_sticky_extras() {
        return Ok(cfg);
    }
    let home = home_dir().unwrap_or_else(|_| PathBuf::from("."));
    let candidates = catalog::soft_prompt_candidates(&home, &cfg.targets.extra);
    match checklist::run_extras_checklist(&candidates) {
        Ok(extras) => {
            cfg.targets.extra = linker::filter_extra_ids(&extras);
            cfg.targets.prompted = true;
            save(paths, &cfg)?;
        }
        Err(_) => {
            // EOF / write failure: do not set prompted; never block the command.
        }
    }
    Ok(cfg)
}

fn should_prompt_sticky_extras() -> bool {
    if std::env::var_os("CI").is_some()
        || std::env::var_os("SKL_NO_PROMPT").is_some()
        || std::env::var_os("SKL_YES").is_some()
    {
        return false;
    }
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Parse the init/doctor extras prompt (`claude,cursor` / `none` / empty).
pub fn parse_extra_prompt_line(input: &str) -> Result<Vec<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "n" | "none" | "skip" | "no"
        )
    {
        return Ok(Vec::new());
    }
    let parts: Vec<String> = trimmed
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect();
    linker::normalize_extra_ids(&parts)
}

/// Add extra ids to sticky prefs (deduped, validated).
pub fn add_sticky_extras(paths: &Paths, ids: &[String]) -> Result<Config> {
    let extras = linker::normalize_extra_ids(ids)?;
    let mut cfg = load(paths).unwrap_or_default();
    cfg.targets.extra = linker::merge_extra_ids(&[&cfg.targets.extra, &extras]);
    save(paths, &cfg)?;
    Ok(cfg)
}

/// Remove extra ids from sticky prefs. Accepts aliases (`claude`) and leftover universal ids.
pub fn remove_sticky_extras(paths: &Paths, ids: &[String]) -> Result<Config> {
    let drop: Vec<String> = ids
        .iter()
        .map(|id| catalog::canonicalize_id(id.trim()).to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect();
    let mut cfg = load(paths).unwrap_or_default();
    cfg.targets.extra.retain(|id| {
        !drop
            .iter()
            .any(|d| d == &id.to_ascii_lowercase() || (*d == "claude" && id == "claude-code"))
    });
    save(paths, &cfg)?;
    Ok(cfg)
}

pub fn home_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home));
    }
    dirs::home_dir().ok_or_else(|| SklError::Config("cannot resolve $HOME".into()))
}

/// Home skill libraries scanned by `skl init` and `skl doctor`.
///
/// Unique catalog globals plus `~/.agents/skills` and `~/.config/agents/skills`
/// (deduped by path). New canonical files prefer `~/.agents/skills`.
pub fn skill_roots(home: &Path) -> Vec<SkillRoot> {
    catalog::unique_global_roots(home)
}

pub use crate::catalog::SkillRoot;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_prefers_cli_over_config() {
        let cfg = Config {
            api_base: Some("http://from-config.example".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_api_base(Some("http://flag.example/"), &cfg),
            "http://flag.example"
        );
        assert_eq!(resolve_api_base(None, &cfg), "http://from-config.example");
        assert_eq!(resolve_api_base(None, &Config::default()), DEFAULT_API_BASE);
    }

    #[test]
    fn missing_targets_table_deserializes_empty_extras() {
        let cfg: Config = toml::from_str("api_base = \"http://x\"\n").unwrap();
        assert!(cfg.targets.extra.is_empty());
        assert!(!cfg.targets.prompted);
        assert!(cfg.sync.auto);
        assert_eq!(cfg.sync.frequency_secs, DEFAULT_SYNC_FREQUENCY_SECS);
    }

    #[test]
    fn sync_table_partial_keeps_frequency_default() {
        let cfg: Config = toml::from_str("[sync]\nauto = false\n").unwrap();
        assert!(!cfg.sync.auto);
        assert_eq!(cfg.sync.frequency_secs, DEFAULT_SYNC_FREQUENCY_SECS);
    }

    #[test]
    fn parse_extra_prompt_accepts_custom_and_none() {
        assert_eq!(
            parse_extra_prompt_line("claude-code").unwrap(),
            ["claude-code"]
        );
        assert_eq!(parse_extra_prompt_line("claude").unwrap(), ["claude-code"]);
        assert!(parse_extra_prompt_line("none").unwrap().is_empty());
        assert!(parse_extra_prompt_line("").unwrap().is_empty());
        assert!(parse_extra_prompt_line("cursor").is_err());
        assert!(parse_extra_prompt_line("nope").is_err());
    }

    #[test]
    fn skill_roots_include_ensured_stores_and_non_trio_catalog() {
        let home = Path::new("/tmp/skl-home");
        let roots = skill_roots(home);
        let pairs: Vec<(&str, PathBuf)> = roots
            .iter()
            .map(|root| (root.source, root.path.clone()))
            .collect();
        assert!(pairs.contains(&("agents", home.join(".agents").join("skills"))));
        assert!(pairs.contains(&(
            "xdg-agents",
            home.join(".config").join("agents").join("skills")
        )));
        assert!(pairs.contains(&("claude-code", home.join(".claude").join("skills"))));
        assert!(pairs.contains(&("cursor", home.join(".cursor").join("skills"))));
        assert!(pairs.contains(&("codex", home.join(".codex").join("skills"))));
        assert!(
            pairs.iter().any(|(src, path)| *src == "windsurf"
                && path == &home.join(".codeium").join("windsurf").join("skills")),
            "non-trio catalog global missing: {pairs:?}"
        );
        assert!(roots.len() > 5);
    }

    #[test]
    fn library_dir_is_under_data_dir_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: data_dir.clone(),
            db_file: data_dir.join("state.db"),
        };
        assert_eq!(paths.library_dir(), data_dir.join("skills"));
        assert_eq!(
            paths.library_skill("greeter"),
            data_dir.join("skills").join("greeter")
        );
        assert!(
            !paths
                .library_dir()
                .to_string_lossy()
                .contains(".agents/skills"),
            "personal library must not be ~/.agents/skills"
        );
    }

    #[test]
    fn sticky_extras_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        add_sticky_extras(&paths, &["claude".into()]).unwrap();
        let cfg = load(&paths).unwrap();
        assert_eq!(cfg.sticky_extras(), ["claude-code"]);
        assert!(add_sticky_extras(&paths, &["cursor".into()]).is_err());
        remove_sticky_extras(&paths, &["claude-code".into()]).unwrap();
        assert!(load(&paths).unwrap().sticky_extras().is_empty());
    }

    #[test]
    fn load_migrates_claude_alias_and_drops_universal_extras() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("cfg"),
            config_file: tmp.path().join("cfg/config.toml"),
            data_dir: tmp.path().join("data"),
            db_file: tmp.path().join("data/state.db"),
        };
        paths.ensure().unwrap();
        std::fs::write(
            &paths.config_file,
            "[targets]\nextra = [\"claude\", \"cursor\", \"codex\"]\n",
        )
        .unwrap();
        let cfg = load(&paths).unwrap();
        assert_eq!(cfg.sticky_extras(), ["claude-code"]);
        let raw = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(raw.contains("claude-code"), "{raw}");
        assert!(!raw.contains("cursor"), "{raw}");
        assert!(!raw.contains("codex"), "{raw}");
    }
}
