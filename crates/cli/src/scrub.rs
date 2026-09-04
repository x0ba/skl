use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::skill_tree::{slash_path, SkillFile, SkillTree};

/// How loudly a finding should stop an upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Suspicious; furnace may continue if the user passed `--allow-warnings`.
    Warn,
    /// Obvious credential. Upload must not proceed.
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    PrivateKey,
    AwsAccessKey,
    AwsSecretKey,
    GitHubToken,
    SlackToken,
    StripeLiveKey,
    OpenAiKey,
    AnthropicKey,
    GoogleApiKey,
    NpmToken,
    EnvSecret,
    Jwt,
}

impl SecretKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateKey => "private_key",
            Self::AwsAccessKey => "aws_access_key",
            Self::AwsSecretKey => "aws_secret_key",
            Self::GitHubToken => "github_token",
            Self::SlackToken => "slack_token",
            Self::StripeLiveKey => "stripe_live_key",
            Self::OpenAiKey => "openai_key",
            Self::AnthropicKey => "anthropic_key",
            Self::GoogleApiKey => "google_api_key",
            Self::NpmToken => "npm_token",
            Self::EnvSecret => "env_secret",
            Self::Jwt => "jwt",
        }
    }

    fn default_severity(self) -> Severity {
        match self {
            Self::Jwt | Self::GoogleApiKey => Severity::Warn,
            _ => Severity::Block,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub skill_name: String,
    pub path: PathBuf,
    pub kind: SecretKind,
    pub severity: Severity,
    pub line: Option<usize>,
}

impl Finding {
    pub fn location(&self) -> String {
        match self.line {
            Some(n) => format!("{}:{}", slash_path(&self.path), n),
            None => slash_path(&self.path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
}

impl ScanReport {
    pub fn blocks(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Block)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warn)
    }

    pub fn has_blocks(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Block)
    }

    pub fn has_warnings(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Warn)
    }
}

static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----").expect("private key regex")
});
static AWS_ACCESS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("aws access regex"));
static AWS_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)aws_secret_access_key\s*[=:]\s*[A-Za-z0-9/+=]{40}").expect("aws secret regex")
});
static GITHUB_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{20,})\b")
        .expect("github token regex")
});
static SLACK_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b").expect("slack token regex"));
static STRIPE_LIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bsk_live_[A-Za-z0-9]{16,}\b").expect("stripe regex"));
static ANTHROPIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bsk-ant-[A-Za-z0-9_-]{20,}\b").expect("anthropic regex"));
static OPENAI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b").expect("openai regex"));
static GOOGLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAIza[0-9A-Za-z_-]{35}\b").expect("google regex"));
static NPM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bnpm_[A-Za-z0-9]{36}\b").expect("npm regex"));
static JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").expect("jwt regex")
});
static ENV_ASSIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(api[_-]?key|secret[_-]?key|access[_-]?token|auth[_-]?token|password|private[_-]?key)\s*[=:]\s*(\S+)",
    )
    .expect("env assign regex")
});

const ENV_FILENAMES: &[&str] = &[".env", ".env.local", ".env.production", ".env.development"];

/// Scan every file in a skill tree for obvious credential patterns.
///
/// Furnace: call this (via [`crate::pre_upload_guard`]) before any upload.
pub fn scan_tree(tree: &SkillTree) -> ScanReport {
    let mut findings = Vec::new();
    for file in &tree.files {
        findings.extend(scan_skill_file(file));
    }
    ScanReport { findings }
}

pub fn scan_skill_file(file: &SkillFile) -> Vec<Finding> {
    let bytes = match std::fs::read(&file.abs_path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    scan_bytes(&file.skill_name, &file.relative_path, &bytes)
}

pub fn scan_bytes(skill_name: &str, path: &Path, bytes: &[u8]) -> Vec<Finding> {
    if looks_binary(bytes) {
        return scan_binary(skill_name, path, bytes);
    }
    let text = String::from_utf8_lossy(bytes);
    let mut findings = Vec::new();

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &PRIVATE_KEY,
            SecretKind::PrivateKey,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &AWS_ACCESS,
            SecretKind::AwsAccessKey,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &AWS_SECRET,
            SecretKind::AwsSecretKey,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &GITHUB_TOKEN,
            SecretKind::GitHubToken,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &SLACK_TOKEN,
            SecretKind::SlackToken,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &STRIPE_LIVE,
            SecretKind::StripeLiveKey,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &ANTHROPIC,
            SecretKind::AnthropicKey,
        );
        // OpenAI pattern also matches sk-ant-*; skip those lines already classified.
        if !ANTHROPIC.is_match(line) {
            push_matches(
                &mut findings,
                skill_name,
                path,
                line,
                Some(line_no),
                &OPENAI,
                SecretKind::OpenAiKey,
            );
        }
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &GOOGLE,
            SecretKind::GoogleApiKey,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &NPM,
            SecretKind::NpmToken,
        );
        push_matches(
            &mut findings,
            skill_name,
            path,
            line,
            Some(line_no),
            &JWT,
            SecretKind::Jwt,
        );

        if let Some(caps) = ENV_ASSIGN.captures(line) {
            let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if !looks_like_placeholder(value) {
                findings.push(Finding {
                    skill_name: skill_name.to_string(),
                    path: path.to_path_buf(),
                    kind: SecretKind::EnvSecret,
                    severity: env_severity(path),
                    line: Some(line_no),
                });
            }
        }
    }

    findings
}

fn scan_binary(skill_name: &str, path: &Path, bytes: &[u8]) -> Vec<Finding> {
    let text = String::from_utf8_lossy(bytes);
    let mut findings = Vec::new();
    if PRIVATE_KEY.is_match(&text) {
        findings.push(Finding {
            skill_name: skill_name.to_string(),
            path: path.to_path_buf(),
            kind: SecretKind::PrivateKey,
            severity: Severity::Block,
            line: None,
        });
    }
    findings
}

fn push_matches(
    out: &mut Vec<Finding>,
    skill_name: &str,
    path: &Path,
    line: &str,
    line_no: Option<usize>,
    re: &Regex,
    kind: SecretKind,
) {
    if re.is_match(line) {
        out.push(Finding {
            skill_name: skill_name.to_string(),
            path: path.to_path_buf(),
            kind,
            severity: kind.default_severity(),
            line: line_no,
        });
    }
}

fn env_severity(path: &Path) -> Severity {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if ENV_FILENAMES.contains(&name) {
        Severity::Block
    } else {
        Severity::Warn
    }
}

fn looks_like_placeholder(value: &str) -> bool {
    let trimmed = value.trim_matches(|c| c == '"' || c == '\'');
    if trimmed.len() < 8 {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.contains("example")
        || lower.contains("placeholder")
        || lower.contains("changeme")
        || lower.contains("your-")
        || lower.contains("your_")
        || lower.starts_with("xxx")
        || lower.starts_with('<')
        || lower.starts_with("${")
        || lower == "todo"
        || lower.chars().all(|c| c == 'x' || c == 'X')
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }
    let sample = &bytes[..bytes.len().min(1024)];
    let odd = sample.iter().filter(|b| **b < 9 && **b != b'\t').count();
    odd > sample.len() / 10
}

/// Result of the pre-hash / pre-upload secret scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadDecision {
    Allow,
    AllowWithWarnings { warnings: Vec<Finding> },
    Block { report: ScanReport },
}

fn decide(findings: Vec<Finding>, allow_warnings: bool) -> UploadDecision {
    let report = ScanReport { findings };
    if report.has_blocks() {
        return UploadDecision::Block { report };
    }
    if report.has_warnings() {
        if allow_warnings {
            return UploadDecision::AllowWithWarnings {
                warnings: report.findings,
            };
        }
        return UploadDecision::Block { report };
    }
    UploadDecision::Allow
}

/// Scrub file bytes. Call this before hashing or PUT /v1/blobs/:hash.
pub fn guard_bytes(skill: &str, path: &Path, bytes: &[u8]) -> UploadDecision {
    decide(scan_bytes(skill, path, bytes), false)
}

pub fn guard_bytes_with(
    skill: &str,
    path: &Path,
    bytes: &[u8],
    allow_warnings: bool,
) -> UploadDecision {
    decide(scan_bytes(skill, path, bytes), allow_warnings)
}

pub fn print_report(report: &ScanReport, out: &mut impl std::io::Write) -> std::io::Result<()> {
    for finding in &report.findings {
        let level = match finding.severity {
            Severity::Block => "blocked",
            Severity::Warn => "warning",
        };
        writeln!(
            out,
            "{level}: {} / {} ({})",
            finding.skill_name,
            finding.location(),
            finding.kind.as_str()
        )?;
    }
    let blocks = report.blocks().count();
    let warns = report.warnings().count();
    writeln!(out, "scrub: {blocks} blocked, {warns} warnings")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<SecretKind> {
        scan_bytes("demo", Path::new("SKILL.md"), text.as_bytes())
            .into_iter()
            .map(|f| f.kind)
            .collect()
    }

    #[test]
    fn detects_private_key_block() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nfake\n-----END RSA PRIVATE KEY-----\n";
        assert!(kinds(pem).contains(&SecretKind::PrivateKey));
    }

    #[test]
    fn detects_aws_access_key() {
        // Split so repo scanners do not treat the test as a live credential.
        let key = format!("{}{}{}", "AK", "IA", "TESTKEY012345678");
        assert!(kinds(&key).contains(&SecretKind::AwsAccessKey));
    }

    #[test]
    fn detects_github_pat() {
        let token = format!("{}{}", "ghp_", "a".repeat(36));
        assert!(kinds(&token).contains(&SecretKind::GitHubToken));
    }

    #[test]
    fn detects_slack_and_stripe() {
        let slack = format!("{}{}", "xoxb-", "1234567890-abcdef");
        let stripe = format!("{}{}", "sk_live_", "abcdefghijklmnopqrstuv");
        assert!(kinds(&slack).contains(&SecretKind::SlackToken));
        assert!(kinds(&stripe).contains(&SecretKind::StripeLiveKey));
    }

    #[test]
    fn anthropic_is_not_also_openai() {
        let key = format!("{}{}", "sk-ant-", "api03-abcdefghijklmnopqrstuvwxyz");
        let found = kinds(&key);
        assert!(found.contains(&SecretKind::AnthropicKey));
        assert!(!found.contains(&SecretKind::OpenAiKey));
    }

    #[test]
    fn placeholder_env_is_ignored() {
        assert!(kinds("API_KEY=your-api-key-here").is_empty());
        assert!(kinds("API_KEY=${SECRET}").is_empty());
        assert!(kinds("API_KEY=xxxx").is_empty());
    }

    #[test]
    fn env_file_assignment_blocks() {
        let findings = scan_bytes(
            "demo",
            Path::new(".env"),
            b"API_KEY=super-secret-value-12345\n",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Block);
        assert_eq!(findings[0].kind, SecretKind::EnvSecret);
    }

    #[test]
    fn jwt_is_warning_only() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcde";
        let findings = scan_bytes("demo", Path::new("notes.md"), jwt.as_bytes());
        assert_eq!(findings[0].kind, SecretKind::Jwt);
        assert_eq!(findings[0].severity, Severity::Warn);
    }

    #[test]
    fn clean_markdown_is_quiet() {
        assert!(kinds("# My skill\n\nUse an API key from the dashboard.\n").is_empty());
    }
}
