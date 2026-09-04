//! Locked API shapes (Bob/cipher hard lock). Paths MUST use `/v1`.
//!
//! Device auth:
//!   POST /v1/auth/device/code
//!   POST /v1/auth/device/token
//!   POST /v1/auth/device/approve   (web/Clerk; CLI does not call this)
//!
//! Sync:
//!   POST /v1/sync
//!   PUT  /v1/blobs/:hash
//!   GET  /v1/blobs/:hash
//!   PUT  /v1/skills/:name/tree
//!
//! Read models:
//!   GET    /v1/skills
//!   GET    /v1/skills/:name
//!   GET    /v1/devices
//!   DELETE /v1/devices/:id
//!   GET    /v1/health

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceCodeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceTokenRequest {
    pub device_code: String,
    pub grant_type: String,
}

impl DeviceTokenRequest {
    pub fn new(device_code: impl Into<String>) -> Self {
        Self {
            device_code: device_code.into(),
            grant_type: DEVICE_GRANT_TYPE.to_string(),
        }
    }
}

/// 200 body. `expires_in` is null for a long-lived device token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceTokenSuccess {
    pub access_token: String,
    pub token_type: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// 400 body. `slow_down` may include a new `interval`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceTokenErrorBody {
    pub error: DeviceTokenErrorKind,
    #[serde(default)]
    pub interval: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTokenErrorKind {
    AuthorizationPending,
    SlowDown,
    ExpiredToken,
    AccessDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceTokenPoll {
    Success(DeviceTokenSuccess),
    Pending,
    SlowDown { interval: Option<u64> },
    Expired,
    Denied,
}

/// POST /v1/auth/device/approve — Clerk/web only. Typed here so the client matches the lock.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceApproveRequest {
    pub user_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceApproveResponse {
    pub ok: bool,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTree {
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SyncRequest {
    pub skills: BTreeMap<String, SkillTree>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncResponse {
    pub upload: Vec<String>,
    pub download: Vec<SyncDownload>,
    pub conflicts: Vec<SyncConflict>,
    pub missing_skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncDownload {
    pub hash: String,
    pub skills: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncConflict {
    pub skill: String,
    pub local_tree_hash: String,
    pub remote_tree_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTreePut {
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub name: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    pub name: String,
    #[serde(default)]
    pub tree_hash: Option<String>,
    #[serde(default)]
    pub files: Option<BTreeMap<String, String>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SkillList {
    Names(Vec<String>),
    Objects(Vec<SkillSummary>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSummary {
    pub id: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeviceList {
    Ids(Vec<String>),
    Objects(Vec<DeviceSummary>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_token_error_roundtrip() {
        let pending: DeviceTokenErrorBody =
            serde_json::from_str(r#"{"error":"authorization_pending"}"#).unwrap();
        assert_eq!(pending.error, DeviceTokenErrorKind::AuthorizationPending);
        assert_eq!(pending.interval, None);

        let slow: DeviceTokenErrorBody =
            serde_json::from_str(r#"{"error":"slow_down","interval":10}"#).unwrap();
        assert_eq!(slow.error, DeviceTokenErrorKind::SlowDown);
        assert_eq!(slow.interval, Some(10));
    }

    #[test]
    fn device_token_success_null_expires() {
        let tok: DeviceTokenSuccess = serde_json::from_str(
            r#"{"access_token":"dev_abc","token_type":"Bearer","expires_in":null}"#,
        )
        .unwrap();
        assert_eq!(tok.access_token, "dev_abc");
        assert_eq!(tok.token_type, "Bearer");
        assert_eq!(tok.expires_in, None);
    }

    #[test]
    fn sync_request_shape() {
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".into(), "aaa".into());
        let mut skills = BTreeMap::new();
        skills.insert(
            "demo".into(),
            SkillTree {
                tree_hash: "bbb".into(),
                files,
            },
        );
        let body = serde_json::to_value(SyncRequest { skills }).unwrap();
        assert_eq!(body["skills"]["demo"]["tree_hash"], "bbb");
        assert_eq!(body["skills"]["demo"]["files"]["SKILL.md"], "aaa");
    }

    #[test]
    fn token_request_grant_type() {
        let req = DeviceTokenRequest::new("dc");
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["grant_type"], DEVICE_GRANT_TYPE);
        assert_eq!(v["device_code"], "dc");
    }
}
