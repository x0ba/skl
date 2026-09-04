//! Locked API shapes from `apps/api/src/contracts.ts` (`@skl/api/contracts`).
//! Paths MUST use `/v1`. Do not register or call unversioned aliases.
//!
//! Read models are wrapped objects, never bare `string[]`:
//!   GET /v1/skills  → `{ skills: { name, tree_hash, updated_at }[] }`
//!   GET /v1/devices → `{ devices: { id, name, created_at, last_used_at, revoked_at }[] }`
//!   GET /v1/health  → `{ ok: true }` exactly

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

pub const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const DEVICE_TOKEN_PREFIX: &str = "skl_dt_";
pub const DEV_AUTH_PREFIX: &str = "dev:";
pub const HASH_ALG: &str = "sha256";
pub const API_PREFIX: &str = "/v1";
pub const X_CONTENT_HASH: &str = "x-content-hash";

fn deserialize_ok_true<'de, D>(deserializer: D) -> std::result::Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match bool::deserialize(deserializer)? {
        true => Ok(true),
        false => Err(serde::de::Error::custom("expected ok: true")),
    }
}

// --- Device auth -----------------------------------------------------------

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
    pub verification_uri_complete: String,
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

/// 200 body. Contract: `{ access_token, expires_in: null }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceTokenSuccess {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// 400 body from `DeviceTokenError`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceTokenErrorBody {
    pub error: DeviceTokenErrorKind,
    #[serde(default)]
    pub error_description: Option<String>,
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
    SlowDown,
    Expired,
    Denied,
}

/// POST /v1/auth/device/approve — Clerk/web only. CLI login does not call this.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceApproveRequest {
    pub user_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceApproveResponse {
    #[serde(deserialize_with = "deserialize_ok_true")]
    pub ok: bool,
    pub device_id: String,
}

// --- Sync ------------------------------------------------------------------

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
    /// ISO-8601 from the remote skill row.
    pub remote_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTreePut {
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutSkillTreeResponse {
    pub name: String,
    pub tree_hash: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutBlobJsonRequest {
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutBlobResponse {
    pub hash: String,
    pub size: u64,
}

// --- Read models -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    #[serde(deserialize_with = "deserialize_ok_true")]
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: String,
    pub tree_hash: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillsListResponse {
    pub skills: Vec<SkillSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDetail {
    pub name: String,
    pub tree_hash: String,
    pub files: BTreeMap<String, String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevicesListResponse {
    pub devices: Vec<DeviceRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_token_error_roundtrip() {
        let pending: DeviceTokenErrorBody =
            serde_json::from_str(r#"{"error":"authorization_pending"}"#).unwrap();
        assert_eq!(pending.error, DeviceTokenErrorKind::AuthorizationPending);
        assert_eq!(pending.error_description, None);

        let slow: DeviceTokenErrorBody = serde_json::from_str(
            r#"{"error":"slow_down","error_description":"Polling too frequently"}"#,
        )
        .unwrap();
        assert_eq!(slow.error, DeviceTokenErrorKind::SlowDown);
        assert_eq!(
            slow.error_description.as_deref(),
            Some("Polling too frequently")
        );
    }

    #[test]
    fn device_token_success_matches_contract() {
        let tok: DeviceTokenSuccess =
            serde_json::from_str(r#"{"access_token":"skl_dt_abc","expires_in":null}"#).unwrap();
        assert!(tok.access_token.starts_with(DEVICE_TOKEN_PREFIX));
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
        assert_eq!(DEVICE_TOKEN_PREFIX, "skl_dt_");
        assert_eq!(HASH_ALG, "sha256");
        assert_eq!(API_PREFIX, "/v1");
    }

    #[test]
    fn conflict_remote_updated_at_is_iso() {
        let conflict: SyncConflict = serde_json::from_str(
            r#"{"skill":"demo","local_tree_hash":"aaa","remote_tree_hash":"bbb","remote_updated_at":"2026-09-04T08:00:00.000Z"}"#,
        )
        .unwrap();
        assert_eq!(conflict.remote_updated_at, "2026-09-04T08:00:00.000Z");
    }

    #[test]
    fn skills_list_is_wrapped() {
        let list: SkillsListResponse = serde_json::from_str(
            r#"{"skills":[{"name":"greeter","tree_hash":"abc","updated_at":"2026-09-04T08:00:00.000Z"}]}"#,
        )
        .unwrap();
        assert_eq!(list.skills.len(), 1);
        assert_eq!(list.skills[0].name, "greeter");
    }

    #[test]
    fn devices_list_is_wrapped() {
        let list: DevicesListResponse = serde_json::from_str(
            r#"{"devices":[{"id":"d1","name":"laptop","created_at":"2026-09-04T08:00:00.000Z","last_used_at":null,"revoked_at":null}]}"#,
        )
        .unwrap();
        assert_eq!(list.devices[0].id, "d1");
        assert_eq!(list.devices[0].last_used_at, None);
    }

    #[test]
    fn health_is_ok_true() {
        let health: HealthResponse = serde_json::from_str(r#"{"ok":true}"#).unwrap();
        assert!(health.ok);
        assert!(serde_json::from_str::<HealthResponse>(r#"{"ok":false}"#).is_err());
        assert!(serde_json::from_str::<HealthResponse>(r#"{"status":"ok"}"#).is_err());
    }

    #[test]
    fn skills_and_devices_are_not_string_arrays() {
        assert!(serde_json::from_str::<SkillsListResponse>(r#"["greeter"]"#).is_err());
        assert!(serde_json::from_str::<DevicesListResponse>(r#"["d1"]"#).is_err());
    }

    #[test]
    fn put_blob_response() {
        let body: PutBlobResponse = serde_json::from_str(r#"{"hash":"abc","size":12}"#).unwrap();
        assert_eq!(body.hash, "abc");
        assert_eq!(body.size, 12);
    }
}
