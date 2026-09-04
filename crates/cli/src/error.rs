use std::fmt;

pub type Result<T> = std::result::Result<T, SklError>;

#[derive(Debug)]
pub enum SklError {
    NotLoggedIn,
    DeviceAuthDenied,
    DeviceAuthExpired,
    DeviceAuthFailed(String),
    Api { status: u16, body: String },
    ApiUnreachable { url: String, source: String },
    Keyring(String),
    Config(String),
    LocalState(String),
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
    Sqlite(rusqlite::Error),
}

impl fmt::Display for SklError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLoggedIn => {
                write!(f, "not logged in; run `skl login`")
            }
            Self::DeviceAuthDenied => write!(f, "device authorization was denied"),
            Self::DeviceAuthExpired => write!(f, "device authorization expired; run `skl login` again"),
            Self::DeviceAuthFailed(msg) => write!(f, "device authorization failed: {msg}"),
            Self::Api { status, body } => write!(f, "API error {status}: {body}"),
            Self::ApiUnreachable { url, source } => {
                write!(f, "cannot reach API at {url}: {source}")
            }
            Self::Keyring(msg) => {
                write!(f, "OS keyring error ({msg}); device token is stored in the system keyring (service=skl)")
            }
            Self::Config(msg) => write!(f, "config: {msg}"),
            Self::LocalState(msg) => write!(f, "local state: {msg}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Http(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::Sqlite(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for SklError {}

impl From<std::io::Error> for SklError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<reqwest::Error> for SklError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

impl From<serde_json::Error> for SklError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<rusqlite::Error> for SklError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<keyring::Error> for SklError {
    fn from(value: keyring::Error) -> Self {
        Self::Keyring(value.to_string())
    }
}
