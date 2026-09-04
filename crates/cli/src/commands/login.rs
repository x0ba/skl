use std::time::{Duration, Instant};

use crate::api::{ApiClient, DeviceTokenPoll};
use crate::auth;
use crate::config::{self, Paths};
use crate::error::{Result, SklError};

pub async fn run(api_base: String) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;

    let client = ApiClient::new(&api_base)?;
    let client_name = default_client_name();

    eprintln!("Requesting device code from {api_base} ...");
    let code = client.request_device_code(Some(client_name)).await?;

    eprintln!();
    eprintln!("To approve this device, open:");
    if let Some(complete) = &code.verification_uri_complete {
        eprintln!("  {complete}");
    } else {
        eprintln!("  {}", code.verification_uri);
    }
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
            DeviceTokenPoll::SlowDown {
                interval: new_interval,
            } => {
                interval = match new_interval {
                    Some(secs) => Duration::from_secs(secs.max(1)),
                    None => interval + Duration::from_secs(5),
                };
            }
            DeviceTokenPoll::Expired => return Err(SklError::DeviceAuthExpired),
            DeviceTokenPoll::Denied => return Err(SklError::DeviceAuthDenied),
            DeviceTokenPoll::Success(token) => {
                if token.token_type != "Bearer" {
                    eprintln!(
                        "warning: expected token_type \"Bearer\", got {:?}",
                        token.token_type
                    );
                }
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

fn default_client_name() -> String {
    let host = whoami::fallible::hostname().unwrap_or_else(|_| "unknown".into());
    format!("skl@{host}")
}
