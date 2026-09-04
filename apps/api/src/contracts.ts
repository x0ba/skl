/**
 * skl API contracts — furnace (CLI + device-approve page) mirrors this file.
 *
 * Unversioned paths only (no /v1):
 *
 *   POST   /auth/device/code
 *   POST   /auth/device/token
 *   POST   /auth/device/approve
 *   GET    /devices
 *   DELETE /devices/:id
 *   POST   /sync
 *   PUT    /blobs/:hash
 *   GET    /blobs/:hash
 *   PUT    /skills/:name/tree
 *   GET    /skills
 *   GET    /skills/:name
 *   GET    /health
 *
 * Auth: `Authorization: Bearer` Clerk JWT (web) or device token (CLI).
 * Local only (no CLERK_SECRET_KEY): `Authorization: Bearer dev:<clerk_user_id>`.
 * Store only hashed device tokens. No refresh_token.
 *
 * Content addressing (E2EE-ready):
 *   - Blob hash = lowercase hex SHA-256 of the raw bytes.
 *   - Tree hash = SHA-256 of sorted `${path}\0${hash}` lines (`\n`-joined).
 *     Empty tree => SHA-256 of the empty string.
 */

export const DEVICE_TOKEN_PREFIX = "skl_dt_" as const;
export const DEV_AUTH_PREFIX = "dev:" as const;
export const HASH_ALG = "sha256" as const;
export const DEVICE_GRANT_TYPE =
  "urn:ietf:params:oauth:grant-type:device_code" as const;

export const API_ROUTES = {
  health: "/health",
  deviceCode: "/auth/device/code",
  deviceToken: "/auth/device/token",
  deviceApprove: "/auth/device/approve",
  devices: "/devices",
  device: "/devices/:id",
  sync: "/sync",
  blob: "/blobs/:hash",
  skills: "/skills",
  skill: "/skills/:name",
  skillTree: "/skills/:name/tree",
} as const;

export function devicePath(id: string): string {
  return `/devices/${id}`;
}

export function blobPath(hash: string): string {
  return `/blobs/${hash}`;
}

export function skillPath(name: string): string {
  return `/skills/${encodeURIComponent(name)}`;
}

export function skillTreePath(name: string): string {
  return `/skills/${encodeURIComponent(name)}/tree`;
}

export type ErrorBody = {
  error: string;
  error_description?: string;
};

export type HealthResponse = {
  ok: true;
};

export type DeviceCodeRequest = {
  client_name?: string;
};

export type DeviceCodeResponse = {
  device_code: string;
  user_code: string;
  verification_uri: string;
  verification_uri_complete: string;
  expires_in: number;
  interval: number;
};

export type DeviceTokenRequest = {
  device_code: string;
  grant_type: typeof DEVICE_GRANT_TYPE;
};

export type DeviceTokenErrorCode =
  | "authorization_pending"
  | "slow_down"
  | "expired_token"
  | "access_denied";

export type DeviceTokenError = {
  error: DeviceTokenErrorCode;
  error_description?: string;
};

/** Issued once. Server stores only a hash of access_token. */
export type DeviceTokenSuccess = {
  access_token: string;
};

export type DeviceTokenResponse = DeviceTokenSuccess | DeviceTokenError;

export type DeviceApproveRequest = {
  user_code: string;
  device_name?: string;
};

export type DeviceApproveResponse = {
  ok: true;
  device_id: string;
};

export type DeviceRecord = {
  id: string;
  name: string;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
};

export type DevicesListResponse = {
  devices: DeviceRecord[];
};

export type FileHashMap = Record<string, string>;

export type ClientSkillState = {
  tree_hash: string;
  files: FileHashMap;
};

export type SyncRequest = {
  skills: Record<string, ClientSkillState>;
};

export type SyncDownloadBlob = {
  hash: string;
  skills: string[];
  paths: string[];
};

export type SyncConflict = {
  skill: string;
  local_tree_hash: string;
  remote_tree_hash: string;
};

export type SyncResponse = {
  upload: string[];
  download: SyncDownloadBlob[];
  conflicts: SyncConflict[];
  missing_skills: string[];
};

export type PutSkillTreeRequest = {
  tree_hash: string;
  files: FileHashMap;
};

export type PutSkillTreeResponse = {
  name: string;
  tree_hash: string;
  updated_at: string;
};

export type SkillSummary = {
  name: string;
  tree_hash: string;
  updated_at: string;
};

export type SkillsListResponse = {
  skills: SkillSummary[];
};

export type SkillDetailResponse = {
  name: string;
  tree_hash: string;
  files: FileHashMap;
  updated_at: string;
};

export type PutBlobJsonRequest = {
  content_base64: string;
};

export type PutBlobResponse = {
  hash: string;
  size: number;
};
