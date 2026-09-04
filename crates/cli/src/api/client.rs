use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::types::{
    DeviceApproveRequest, DeviceApproveResponse, DeviceCodeRequest, DeviceCodeResponse,
    DeviceList, DeviceTokenErrorBody, DeviceTokenErrorKind, DeviceTokenPoll, DeviceTokenRequest,
    DeviceTokenSuccess, HealthResponse, SkillDetail, SkillList, SkillTreePut, SyncRequest,
    SyncResponse,
};
use crate::error::{Result, SklError};

/// Typed HTTP client for the locked cipher API (`/v1` prefix is required).
#[derive(Debug, Clone)]
pub struct ApiClient {
    http: Client,
    pub api_base: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(api_base: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!("skl/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            api_base: trim_slash(&api_base.into()),
            token: None,
        })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn set_token(&mut self, token: impl Into<String>) {
        self.token = Some(token.into());
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    async fn send_json<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        authed: bool,
    ) -> Result<T> {
        let mut req = self.http.request(method, self.url(path));
        if authed {
            req = self.apply_auth(req);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().await.map_err(|err| SklError::ApiUnreachable {
            url: self.url(path),
            source: err.to_string(),
        })?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(SklError::Api {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        if bytes.is_empty() {
            return serde_json::from_str("null").map_err(SklError::from);
        }
        serde_json::from_slice(&bytes).map_err(SklError::from)
    }

    pub async fn request_device_code(
        &self,
        client_name: Option<String>,
    ) -> Result<DeviceCodeResponse> {
        self.send_json(
            Method::POST,
            "/v1/auth/device/code",
            Some(&DeviceCodeRequest { client_name }),
            false,
        )
        .await
    }

    pub async fn poll_device_token(&self, device_code: &str) -> Result<DeviceTokenPoll> {
        let response = self
            .http
            .post(self.url("/v1/auth/device/token"))
            .json(&DeviceTokenRequest::new(device_code))
            .send()
            .await
            .map_err(|err| SklError::ApiUnreachable {
                url: self.url("/v1/auth/device/token"),
                source: err.to_string(),
            })?;
        let status = response.status();
        let bytes = response.bytes().await?;
        match status {
            StatusCode::OK => {
                let success: DeviceTokenSuccess = serde_json::from_slice(&bytes)?;
                Ok(DeviceTokenPoll::Success(success))
            }
            StatusCode::BAD_REQUEST => {
                let body: DeviceTokenErrorBody = serde_json::from_slice(&bytes).map_err(|_| {
                    SklError::Api {
                        status: 400,
                        body: String::from_utf8_lossy(&bytes).into_owned(),
                    }
                })?;
                Ok(match body.error {
                    DeviceTokenErrorKind::AuthorizationPending => DeviceTokenPoll::Pending,
                    DeviceTokenErrorKind::SlowDown => DeviceTokenPoll::SlowDown {
                        interval: body.interval,
                    },
                    DeviceTokenErrorKind::ExpiredToken => DeviceTokenPoll::Expired,
                    DeviceTokenErrorKind::AccessDenied => DeviceTokenPoll::Denied,
                })
            }
            other => Err(SklError::Api {
                status: other.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            }),
        }
    }

    /// Web/Clerk approve. CLI login does not call this; included for contract completeness.
    pub async fn approve_device(&self, user_code: &str) -> Result<DeviceApproveResponse> {
        self.send_json(
            Method::POST,
            "/v1/auth/device/approve",
            Some(&DeviceApproveRequest {
                user_code: user_code.to_string(),
            }),
            true,
        )
        .await
    }

    pub async fn sync(&self, body: &SyncRequest) -> Result<SyncResponse> {
        self.send_json(Method::POST, "/v1/sync", Some(body), true)
            .await
    }

    pub async fn put_blob(&self, hash: &str, bytes: Vec<u8>) -> Result<()> {
        let path = format!("/v1/blobs/{hash}");
        let response = self
            .apply_auth(self.http.put(self.url(&path)))
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .map_err(|err| SklError::ApiUnreachable {
                url: self.url(&path),
                source: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(SklError::Api { status, body });
        }
        Ok(())
    }

    pub async fn get_blob(&self, hash: &str) -> Result<Vec<u8>> {
        let path = format!("/v1/blobs/{hash}");
        let response = self
            .apply_auth(self.http.get(self.url(&path)))
            .send()
            .await
            .map_err(|err| SklError::ApiUnreachable {
                url: self.url(&path),
                source: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(SklError::Api { status, body });
        }
        Ok(response.bytes().await?.to_vec())
    }

    pub async fn put_skill_tree(&self, name: &str, body: &SkillTreePut) -> Result<()> {
        let path = format!("/v1/skills/{name}/tree");
        let _: serde_json::Value = self
            .send_json(Method::PUT, &path, Some(body), true)
            .await
            .or_else(|err| match err {
                SklError::Json(_) => Ok(serde_json::Value::Null),
                other => Err(other),
            })?;
        Ok(())
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        self.send_json::<(), _>(Method::GET, "/v1/health", None, false)
            .await
            .or_else(|err| match err {
                SklError::Json(_) => Ok(HealthResponse {
                    ok: Some(true),
                    status: Some("ok".into()),
                }),
                other => Err(other),
            })
    }

    pub async fn list_skills(&self) -> Result<SkillList> {
        self.send_json::<(), _>(Method::GET, "/v1/skills", None, true)
            .await
    }

    pub async fn get_skill(&self, name: &str) -> Result<SkillDetail> {
        let path = format!("/v1/skills/{name}");
        self.send_json::<(), _>(Method::GET, &path, None, true)
            .await
    }

    pub async fn list_devices(&self) -> Result<DeviceList> {
        self.send_json::<(), _>(Method::GET, "/v1/devices", None, true)
            .await
    }

    pub async fn delete_device(&self, id: &str) -> Result<()> {
        let path = format!("/v1/devices/{id}");
        let response = self
            .apply_auth(self.http.delete(self.url(&path)))
            .send()
            .await
            .map_err(|err| SklError::ApiUnreachable {
                url: self.url(&path),
                source: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(SklError::Api { status, body });
        }
        Ok(())
    }
}

fn trim_slash(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{DEVICE_GRANT_TYPE, DeviceCodeResponse};
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn device_code_and_token_poll() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/auth/device/code"))
            .and(body_json(json!({ "client_name": "skl@testhost" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(DeviceCodeResponse {
                device_code: "dc-1".into(),
                user_code: "ABCD-1234".into(),
                verification_uri: format!("{}/device", server.uri()),
                verification_uri_complete: Some(format!("{}/device?user_code=ABCD-1234", server.uri())),
                expires_in: 600,
                interval: 1,
            }))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/auth/device/token"))
            .and(body_json(json!({
                "device_code": "dc-1",
                "grant_type": DEVICE_GRANT_TYPE
            })))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "authorization_pending"
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "dev_tok",
                "token_type": "Bearer",
                "expires_in": null
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let code = client
            .request_device_code(Some("skl@testhost".into()))
            .await
            .unwrap();
        assert_eq!(code.user_code, "ABCD-1234");
        assert_eq!(code.device_code, "dc-1");

        assert_eq!(
            client.poll_device_token("dc-1").await.unwrap(),
            DeviceTokenPoll::Pending
        );
        match client.poll_device_token("dc-1").await.unwrap() {
            DeviceTokenPoll::Success(tok) => {
                assert_eq!(tok.access_token, "dev_tok");
                assert_eq!(tok.token_type, "Bearer");
                assert_eq!(tok.expires_in, None);
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sync_sends_bearer_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/sync"))
            .and(header("authorization", "Bearer dev_tok"))
            .and(body_json(json!({ "skills": {} })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [],
                "missing_skills": []
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap().with_token("dev_tok");
        let res = client.sync(&SyncRequest::default()).await.unwrap();
        assert!(res.upload.is_empty());
        assert!(res.conflicts.is_empty());
    }

    #[tokio::test]
    async fn paths_use_v1_prefix() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let health = client.health().await.unwrap();
        assert_eq!(health.ok, Some(true));
    }
}
