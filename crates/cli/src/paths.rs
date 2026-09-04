//! HTTP paths matching `apps/api/src/contracts.ts` (`API_ROUTES`).
//!
//! Named `paths` (not `api`) so this file can sit next to furnace's
//! `src/api/` HTTP client without a module clash.
//!
//! Cipher lock (all routes under `/v1`, no unversioned aliases):
//! - `POST /v1/sync`
//! - `PUT`/`GET /v1/blobs/:hash` (SHA-256 of scrubbed bytes)
//! - `PUT /v1/skills/:name/tree`
//! - `GET /v1/skills`, `GET /v1/skills/:name`
//!
//! Auth: `Authorization: Bearer <device_token>`.

/// Same value as `API_PREFIX` in `apps/api/src/contracts.ts`.
pub const API_PREFIX: &str = "/v1";

pub const AUTH_HEADER: &str = "Authorization";

pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

pub fn sync_path() -> String {
    format!("{API_PREFIX}/sync")
}

pub fn blob_path(hash: &str) -> String {
    format!("{API_PREFIX}/blobs/{hash}")
}

pub fn skills_path() -> String {
    format!("{API_PREFIX}/skills")
}

/// `GET /v1/skills/:name` — name is encodeURIComponent'd like `skillPath()`.
pub fn skill_path(name: &str) -> String {
    format!("{API_PREFIX}/skills/{}", encode_uri_component(name))
}

/// `PUT /v1/skills/:name/tree`.
pub fn skill_tree_path(name: &str) -> String {
    format!("{API_PREFIX}/skills/{}/tree", encode_uri_component(name))
}

/// JS `encodeURIComponent` for a single path segment.
fn encode_uri_component(name: &str) -> String {
    let mut out = String::new();
    for b in name.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_paths_use_the_single_prefix() {
        assert_eq!(sync_path(), format!("{API_PREFIX}/sync"));
        assert_eq!(blob_path("abc"), format!("{API_PREFIX}/blobs/abc"));
        assert_eq!(skills_path(), format!("{API_PREFIX}/skills"));
        assert_eq!(skill_path("demo"), format!("{API_PREFIX}/skills/demo"));
        assert_eq!(
            skill_tree_path("demo"),
            format!("{API_PREFIX}/skills/demo/tree")
        );
        assert!(sync_path().starts_with(API_PREFIX));
        assert!(!sync_path().contains("//"));
    }

    #[test]
    fn prefix_is_v1() {
        assert_eq!(API_PREFIX, "/v1");
        assert_eq!(sync_path(), "/v1/sync");
        assert_eq!(blob_path("ab"), "/v1/blobs/ab");
        assert_eq!(skills_path(), "/v1/skills");
        assert_eq!(skill_path("demo"), "/v1/skills/demo");
        assert_eq!(skill_tree_path("demo"), "/v1/skills/demo/tree");
        assert_eq!(bearer("tok"), "Bearer tok");
    }

    #[test]
    fn skill_name_is_percent_encoded() {
        assert_eq!(skill_path("my skill"), "/v1/skills/my%20skill");
        assert_eq!(skill_tree_path("my skill"), "/v1/skills/my%20skill/tree");
    }
}
