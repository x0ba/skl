use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SklError};

/// Default API origin (`apps/api` listens on PORT=8787).
/// Override with `--api-base` or `API_BASE`.
pub const DEFAULT_API_BASE: &str = "http://localhost:8787";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub api_base: Option<String>,
}

impl Config {
    pub fn api_base(&self) -> String {
        self.api_base
            .clone()
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
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
        };
        assert_eq!(
            resolve_api_base(Some("http://flag.example/"), &cfg),
            "http://flag.example"
        );
        assert_eq!(resolve_api_base(None, &cfg), "http://from-config.example");
        assert_eq!(
            resolve_api_base(None, &Config::default()),
            DEFAULT_API_BASE
        );
    }
}
