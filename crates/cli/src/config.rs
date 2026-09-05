use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SklError};
use crate::local::linker::{self, EXTRA_TARGET_IDS};

/// Default API origin (`apps/api` listens on PORT=8787).
/// Override with `--api-base` or `API_BASE`.
pub const DEFAULT_API_BASE: &str = "http://localhost:8787";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub api_base: Option<String>,
    /// Sticky extra dests for `skl use` (`~/.config/skl/config.toml`).
    #[serde(default, skip_serializing_if = "TargetPrefs::is_unset")]
    pub targets: TargetPrefs,
}

/// User-level extra link targets. Canonical `agents` is never stored here.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TargetPrefs {
    /// Extra dests (`claude` / `cursor` / `codex`). Empty = `.agents/skills` only.
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
}

pub fn load(paths: &Paths) -> Result<Config> {
    if !paths.config_file.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(&paths.config_file)?;
    toml::from_str(&raw).map_err(|err| SklError::Config(err.to_string()))
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

/// Soft-prompt extras once. Skip in CI / non-TTY so `init`/`doctor` never hang.
pub fn maybe_prompt_sticky_extras(paths: &Paths) -> Result<Config> {
    let mut cfg = load(paths).unwrap_or_default();
    if cfg.targets.prompted || !cfg.targets.extra.is_empty() {
        return Ok(cfg);
    }
    if !should_prompt_sticky_extras() {
        return Ok(cfg);
    }
    match prompt_sticky_extras() {
        Ok(extras) => {
            cfg.targets.extra = extras;
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
    if std::env::var_os("CI").is_some() || std::env::var_os("SKL_NO_PROMPT").is_some() {
        return false;
    }
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn prompt_sticky_extras() -> Result<Vec<String>> {
    use std::io::{self, BufRead, Write};

    let mut stderr = io::stderr();
    writeln!(stderr)?;
    writeln!(
        stderr,
        "Extra agent dests for `skl use`? Canonical is always `.agents/skills`."
    )?;
    writeln!(stderr, "  claude  → .claude/skills")?;
    writeln!(stderr, "  cursor  → .cursor/skills")?;
    writeln!(stderr, "  codex   → .codex/skills")?;
    write!(
        stderr,
        "Enter a comma list ({ids}), or press Enter for none: ",
        ids = EXTRA_TARGET_IDS.join(", ")
    )?;
    stderr.flush()?;

    let mut line = String::new();
    let n = io::BufReader::new(io::stdin()).read_line(&mut line)?;
    if n == 0 {
        return Ok(Vec::new());
    }
    parse_extra_prompt_line(&line)
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

/// Remove extra ids from sticky prefs.
pub fn remove_sticky_extras(paths: &Paths, ids: &[String]) -> Result<Config> {
    let extras = linker::normalize_extra_ids(ids)?;
    let mut cfg = load(paths).unwrap_or_default();
    cfg.targets
        .extra
        .retain(|id| !extras.iter().any(|drop| drop == id));
    save(paths, &cfg)?;
    Ok(cfg)
}

pub fn home_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home));
    }
    dirs::home_dir().ok_or_else(|| SklError::Config("cannot resolve $HOME".into()))
}

pub fn skill_roots(home: &Path) -> Vec<SkillRoot> {
    vec![
        SkillRoot {
            source: "claude",
            path: home.join(".claude").join("skills"),
        },
        SkillRoot {
            source: "cursor",
            path: home.join(".cursor").join("skills"),
        },
        SkillRoot {
            source: "codex",
            path: home.join(".codex").join("skills"),
        },
    ]
}

#[derive(Debug, Clone)]
pub struct SkillRoot {
    pub source: &'static str,
    pub path: PathBuf,
}

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
    }

    #[test]
    fn parse_extra_prompt_accepts_comma_list_and_none() {
        assert_eq!(
            parse_extra_prompt_line("claude, cursor").unwrap(),
            ["claude", "cursor"]
        );
        assert!(parse_extra_prompt_line("none").unwrap().is_empty());
        assert!(parse_extra_prompt_line("").unwrap().is_empty());
        assert!(parse_extra_prompt_line("nope").is_err());
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
        add_sticky_extras(&paths, &["claude".into(), "cursor".into()]).unwrap();
        let cfg = load(&paths).unwrap();
        assert_eq!(cfg.sticky_extras(), ["claude", "cursor"]);
        remove_sticky_extras(&paths, &["cursor".into()]).unwrap();
        assert_eq!(load(&paths).unwrap().sticky_extras(), ["claude"]);
    }
}
