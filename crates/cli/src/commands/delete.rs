//! `skl delete` — stop managing a skill in SKL. Files on disk are left alone.

use crate::api::ApiClient;
use crate::auth;
use crate::error::{Result, SklError};

pub async fn run(names: &[String], api_base: &str) -> Result<()> {
    if names.is_empty() {
        return Err(SklError::LocalState(
            "specify at least one skill: `skl delete <skill>`".into(),
        ));
    }

    let token = auth::load_device_token()?;
    let client = ApiClient::new(api_base)?.with_token(token);

    for name in names {
        match client.delete_skill(name).await {
            Ok(()) => eprintln!("unmanaged {name}  (files on disk kept)"),
            Err(SklError::Api { status: 404, .. }) => {
                return Err(SklError::LocalState(format!(
                    "skill `{name}` is not managed by SKL"
                )));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}
