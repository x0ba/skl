use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::types::{
    DeviceApproveRequest, DeviceApproveResponse, DeviceCodeRequest, DeviceCodeResponse,
    DeviceTokenErrorBody, DeviceTokenErrorKind, DeviceTokenPoll, DeviceTokenRequest,
    DeviceTokenSuccess, DevicesListResponse, HealthResponse, PutBlobResponse, PutSkillTreeResponse,
    SkillDetail, SkillTreePut, SkillsListResponse, SyncRequest, SyncResponse,
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
    pub async fn approve_device(
        &self,
        user_code: &str,
        device_name: Option<String>,
    ) -> Result<DeviceApproveResponse> {
        self.send_json(
            Method::POST,
            "/v1/auth/device/approve",
            Some(&DeviceApproveRequest {
                user_code: user_code.to_string(),
                device_name,
            }),
            true,
        )
        .await
    }

    pub async fn sync(&self, body: &SyncRequest) -> Result<SyncResponse> {
        self.send_json(Method::POST, "/v1/sync", Some(body), true)
            .await
    }

    /// PUT /v1/blobs/:hash with raw octets. API also accepts `{ content_base64 }`.
    pub async fn put_blob(&self, hash: &str, bytes: Vec<u8>) -> Result<PutBlobResponse> {
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
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(SklError::Api {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).into_owned(),
            });
        }
        serde_json::from_slice(&body).map_err(SklError::from)
    }

    /// GET /v1/blobs/:hash → `application/octet-stream`.
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

    pub async fn put_skill_tree(
        &self,
        name: &str,
        body: &SkillTreePut,
    ) -> Result<PutSkillTreeResponse> {
        let path = format!("/v1/skills/{}/tree", encode_path_segment(name));
        self.send_json(Method::PUT, &path, Some(body), true).await
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        self.send_json::<(), _>(Method::GET, "/v1/health", None, false)
            .await
    }

    pub async fn list_skills(&self) -> Result<SkillsListResponse> {
        self.send_json::<(), _>(Method::GET, "/v1/skills", None, true)
            .await
    }

    pub async fn get_skill(&self, name: &str) -> Result<SkillDetail> {
        let path = format!("/v1/skills/{}", encode_path_segment(name));
        self.send_json::<(), _>(Method::GET, &path, None, true)
            .await
    }

    pub async fn list_devices(&self) -> Result<DevicesListResponse> {
        self.send_json::<(), _>(Method::GET, "/v1/devices", None, true)
            .await
    }

    pub async fn delete_device(&self, id: &str) -> Result<()> {
        let path = format!("/v1/devices/{}", encode_path_segment(id));
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

/// Skill names are already slug-safe; encode anything outside unreserved.
fn encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
                verification_uri_complete: Some(format!(
                    "{}/device?user_code=ABCD-1234",
                    server.uri()
                )),
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
                "access_token": "skl_dt_tok",
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
                assert_eq!(tok.access_token, "skl_dt_tok");
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
            .and(header("authorization", "Bearer dev:alice"))
            .and(body_json(json!({ "skills": {} })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload": [],
                "download": [],
                "conflicts": [],
                "missing_skills": []
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri())
            .unwrap()
            .with_token("dev:alice");
        let res = client.sync(&SyncRequest::default()).await.unwrap();
        assert!(res.upload.is_empty());
        assert!(res.conflicts.is_empty());
    }

    #[tokio::test]
    async fn blob_put_raw_and_get_octets() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/blobs/aabb"))
            .and(header("content-type", "application/octet-stream"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "hash": "aabb",
                "size": 5
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/blobs/aabb"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(b"hello"),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap().with_token("dev:alice");
        let put = client.put_blob("aabb", b"hello".to_vec()).await.unwrap();
        assert_eq!(put.hash, "aabb");
        assert_eq!(put.size, 5);
        assert_eq!(client.get_blob("aabb").await.unwrap(), b"hello");
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
        assert!(health.ok);
    }

    #[tokio::test]
    async fn list_skills_wrapped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/skills"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "skills": [{
                    "name": "greeter",
                    "tree_hash": "abc",
                    "updated_at": "2026-09-04T08:00:00.000Z"
                }]
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap().with_token("dev:alice");
        let list = client.list_skills().await.unwrap();
        assert_eq!(list.skills[0].name, "greeter");
    }
}
