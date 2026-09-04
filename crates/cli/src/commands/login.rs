use std::time::{Duration, Instant};

use crate::api::{ApiClient, DeviceTokenPoll};
use crate::auth;
use crate::config::{self, Paths};
use crate::error::{Result, SklError};

pub async fn run(api_base: String, dev_user: Option<String>) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;

    if let Some(user) = dev_user {
        return store_dev_user(&paths, api_base, &user);
    }

    let client = ApiClient::new(&api_base)?;
    let client_name = default_client_name();

    eprintln!("Requesting device code from {api_base} ...");
    let code = client.request_device_code(Some(client_name)).await?;

    eprintln!();
    eprintln!("To approve this device, open:");
    eprintln!("  {}", code.verification_uri_complete);
    eprintln!("And enter code:  {}", code.user_code);
    eprintln!();
    eprintln!("Waiting for approval (poll /v1/auth/device/token) ...");

    let started = Instant::now();
    let expires = Duration::from_secs(code.expires_in.max(1));
    let mut interval = Duration::from_secs(code.interval.max(1));

    loop {
        if started.elapsed() >= expires {
            return Err(SklError::DeviceAuthExpired);
        }
        tokio::time::sleep(interval).await;
        match client.poll_device_token(&code.device_code).await? {
            DeviceTokenPoll::Pending => {}
            DeviceTokenPoll::SlowDown => {
                interval += Duration::from_secs(5);
            }
            DeviceTokenPoll::Expired => return Err(SklError::DeviceAuthExpired),
            DeviceTokenPoll::Denied => return Err(SklError::DeviceAuthDenied),
            DeviceTokenPoll::Success(token) => {
                auth::store_device_token(&token.access_token)?;
                let mut cfg = config::load(&paths).unwrap_or_default();
                cfg.api_base = Some(api_base.clone());
                config::save(&paths, &cfg)?;
                eprintln!("Logged in. Device token stored in OS keyring");
                eprintln!(
                    "  service={}  account={}",
                    auth::KEYRING_SERVICE,
                    auth::KEYRING_ACCOUNT
                );
                eprintln!("  api_base saved to {}", paths.config_file.display());
                return Ok(());
            }
        }
    }
}

fn store_dev_user(paths: &Paths, api_base: String, user: &str) -> Result<()> {
    let token = auth::format_dev_token(user)?;
    auth::store_device_token(&token)?;
    let mut cfg = config::load(paths).unwrap_or_default();
    cfg.api_base = Some(api_base.clone());
    config::save(paths, &cfg)?;
    eprintln!("Stored local dev token (no device poll)");
    eprintln!("  Authorization: Bearer {token}");
    eprintln!("  api_base {api_base} → {}", paths.config_file.display());
    eprintln!("API accepts this only when CLERK_SECRET_KEY is unset (ALLOW_DEV_AUTH).");
    Ok(())
}

fn default_client_name() -> String {
    let host = whoami::fallible::hostname().unwrap_or_else(|_| "unknown".into());
    format!("skl@{host}")
}
