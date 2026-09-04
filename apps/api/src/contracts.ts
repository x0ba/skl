/**
 * skl API contracts — furnace (CLI + device-approve page) imports this file.
 *
 * ALL routes are under /v1. There are no unversioned aliases.
 *
 *   POST   /v1/auth/device/code
 *   POST   /v1/auth/device/token
 *   POST   /v1/auth/device/approve
 *   GET    /v1/devices
 *   DELETE /v1/devices/:id
 *   POST   /v1/sync
 *   PUT    /v1/blobs/:hash
 *   GET    /v1/blobs/:hash
 *   PUT    /v1/skills/:name/tree
 *   GET    /v1/skills
 *   GET    /v1/skills/:name
 *   GET    /v1/health
 *
 * Auth:
 *   - Device token: `Authorization: Bearer skl_dt_<hex>`
 *   - Clerk session JWT: `Authorization: Bearer <jwt>`
 *   - Local only (no CLERK_SECRET_KEY): `Authorization: Bearer dev:<clerk_user_id>`
 *
 * Tokens: long-lived device access_token only. No refresh_token.
 *
 * Content addressing (E2EE-ready):
 *   - Blob hash = lowercase hex SHA-256 of the raw bytes. Server stores bytes
 *     opaquely and never interprets them.
 *   - Tree hash = lowercase hex SHA-256 of the canonical file map:
 *       sort paths lexicographically as UTF-8
 *       join each entry as `${path}\0${hash}` with `\n` between entries
 *       empty tree => SHA-256 of the empty string
 */

export const API_PREFIX = "/v1" as const;
export const DEVICE_TOKEN_PREFIX = "skl_dt_" as const;
export const DEV_AUTH_PREFIX = "dev:" as const;
export const HASH_ALG = "sha256" as const;

export const API_ROUTES = {
  health: "/v1/health",
  deviceCode: "/v1/auth/device/code",
  deviceToken: "/v1/auth/device/token",
  deviceApprove: "/v1/auth/device/approve",
  devices: "/v1/devices",
  device: "/v1/devices/:id",
  sync: "/v1/sync",
  blob: "/v1/blobs/:hash",
  skills: "/v1/skills",
  skill: "/v1/skills/:name",
  skillTree: "/v1/skills/:name/tree",
} as const;

export function devicePath(id: string): string {
  return `/v1/devices/${id}`;
}

export function blobPath(hash: string): string {
  return `/v1/blobs/${hash}`;
}

export function skillPath(name: string): string {
  return `/v1/skills/${encodeURIComponent(name)}`;
}

export function skillTreePath(name: string): string {
  return `/v1/skills/${encodeURIComponent(name)}/tree`;
}

/** Machine-readable error body used by every JSON error response. */
export type ErrorBody = {
  error: string;
  error_description?: string;
};

export type HealthResponse = {
  ok: true;
  db: boolean;
};

export type DeviceCodeRequest = {
  /** Human label shown on the approve page and stored on the device. */
  device_name?: string;
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
};

/**
 * RFC 8628-style poll errors. HTTP 400.
 * `slow_down` means the client must increase its poll interval.
 */
export type DeviceTokenErrorCode =
  | "authorization_pending"
  | "slow_down"
  | "expired_token"
  | "access_denied"
  | "invalid_grant";

export type DeviceTokenError = {
  error: DeviceTokenErrorCode;
  error_description?: string;
};

/** Issued once. Server stores only a hash of access_token. No refresh_token. */
export type DeviceTokenSuccess = {
  access_token: string;
  token_type: "Bearer";
  device_id: string;
};

export type DeviceTokenResponse = DeviceTokenSuccess | DeviceTokenError;

export type DeviceApproveRequest = {
  user_code: string;
  device_name?: string;
};

export type DeviceApproveResponse = {
  approved: true;
  device_name: string;
};

export type DeviceRecord = {
  id: string;
  name: string;
  created_at: string;
  revoked_at: string | null;
  /** True when the caller authenticated with this device token. */
  current: boolean;
};

export type DevicesListResponse = {
  devices: DeviceRecord[];
};

export type DeviceRevokeResponse = {
  revoked: true;
};

/**
 * Per-skill client snapshot.
 * A bare string is accepted as a `tree_hash` shorthand.
 */
export type ClientSkillState = {
  tree_hash: string;
  /** Last tree_hash the client successfully synced from the server. */
  base_tree_hash?: string;
  /** Optional path → blob hash map for file-level upload/download sets. */
  files?: Record<string, string>;
};

export type SyncRequest = {
  skills: Record<string, ClientSkillState | string>;
};

export type SkillFile = {
  path: string;
  hash: string;
};

export type SyncDownloadSkill = {
  name: string;
  tree_hash: string;
  version_id: string;
  updated_at: string;
  files: SkillFile[];
};

export type SyncConflict = {
  skill: string;
  local_tree_hash: string;
  remote_tree_hash: string;
  remote_updated_at: string;
};

export type SyncResponse = {
  upload: {
    skills: string[];
    blobs: string[];
  };
  download: {
    skills: SyncDownloadSkill[];
    blobs: string[];
  };
  conflicts: SyncConflict[];
};

export type PutSkillTreeRequest = {
  tree_hash: string;
  metadata?: Record<string, unknown>;
  files: SkillFile[] | Record<string, string>;
};

export type PutSkillTreeResponse = {
  name: string;
  version_id: string;
  tree_hash: string;
  updated_at: string;
};

export type SkillSummary = {
  name: string;
  tree_hash: string;
  version_id: string;
  metadata: Record<string, unknown>;
  updated_at: string;
};

export type SkillsListResponse = {
  skills: SkillSummary[];
};

export type SkillDetailResponse = SkillSummary & {
  files: SkillFile[];
};

export type PutBlobResponse = {
  hash: string;
  size: number;
};

export type MissingBlobsError = ErrorBody & {
  error: "missing_blobs";
  hashes: string[];
};
