/**
 * Web-side copy of the furnace-facing types in `apps/api/src/contracts.ts`.
 * Keep shapes and `/v1` paths aligned with that file.
 */

export const API_ROUTES = {
  deviceApprove: "/v1/auth/device/approve",
  devices: "/v1/devices",
  skills: "/v1/skills",
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

export type ErrorBody = {
  error: string;
  error_description?: string;
};

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

export type SkillSummary = {
  name: string;
  tree_hash: string;
  updated_at: string;
};

export type SkillsListResponse = {
  skills: SkillSummary[];
};

/** Maps a skill-relative path to the lowercase hex SHA-256 of its contents. */
export type FileHashMap = Record<string, string>;

export type SkillDetailResponse = {
  name: string;
  tree_hash: string;
  files: FileHashMap;
  updated_at: string;
};
