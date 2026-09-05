import { API_BASE } from "./config";
import {
  API_ROUTES,
  devicePath,
  type DeviceApproveRequest,
  type DeviceApproveResponse,
  type DevicesListResponse,
  type ErrorBody,
  type SkillDetailResponse,
  type SkillsListResponse,
  skillPath,
} from "./contracts";

export class ApiError extends Error {
  readonly status: number;
  readonly error: string;
  readonly error_description?: string;

  constructor(status: number, error: string, error_description?: string) {
    super(error_description ?? error);
    this.name = "ApiError";
    this.status = status;
    this.error = error;
    this.error_description = error_description;
  }
}

function isErrorBody(value: unknown): value is ErrorBody {
  return (
    typeof value === "object" &&
    value !== null &&
    "error" in value &&
    typeof (value as { error: unknown }).error === "string"
  );
}

async function readError(res: Response): Promise<ApiError> {
  const text = await res.text();
  if (!text) {
    return new ApiError(res.status, res.statusText || "request_failed");
  }
  try {
    const parsed: unknown = JSON.parse(text);
    if (isErrorBody(parsed)) {
      return new ApiError(res.status, parsed.error, parsed.error_description);
    }
  } catch {
    return new ApiError(res.status, "request_failed", text);
  }
  return new ApiError(res.status, "request_failed", text);
}

async function apiFetch<T>(
  path: string,
  token: string,
  init: RequestInit = {},
): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${token}`);
  if (init.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers,
  });

  if (!res.ok) {
    throw await readError(res);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

export async function approveDevice(
  token: string,
  body: DeviceApproveRequest,
): Promise<DeviceApproveResponse> {
  return apiFetch<DeviceApproveResponse>(API_ROUTES.deviceApprove, token, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function listSkills(token: string): Promise<SkillsListResponse> {
  return apiFetch<SkillsListResponse>(API_ROUTES.skills, token);
}

export async function getSkill(
  token: string,
  name: string,
): Promise<SkillDetailResponse> {
  return apiFetch<SkillDetailResponse>(skillPath(name), token);
}

export async function listDevices(token: string): Promise<DevicesListResponse> {
  return apiFetch<DevicesListResponse>(API_ROUTES.devices, token);
}

export async function revokeDevice(token: string, id: string): Promise<void> {
  await apiFetch<void>(devicePath(id), token, { method: "DELETE" });
}

export function describeApproveError(error: ApiError): string {
  if (error.status === 404 || error.error === "unknown_user_code") {
    return "Invalid code. No pending device for that user_code.";
  }
  if (error.status === 410 || error.error === "expired_token") {
    return "This code has expired. Run `skl login` again for a new user_code.";
  }
  if (error.error === "already_approved") {
    return "This code was already approved.";
  }
  if (error.status === 401 || error.error === "missing_authorization") {
    return "Sign in or set a local Bearer token before approving.";
  }
  return error.error_description ?? error.message;
}

/**
 * Sentences for the API's machine-readable error codes. The API often returns
 * a bare `error` with no `error_description`, and a raw code like
 * `skill_not_found` is not something to put in front of a person.
 */
const ERROR_SENTENCES: Record<string, string> = {
  skill_not_found: "No skill with that name in this account.",
  device_not_found: "That device no longer exists.",
  missing_authorization: "No credentials were sent with the request.",
  invalid_token: "That bearer token was rejected. It may have been revoked.",
  expired_token: "That code has expired.",
  unknown_user_code: "No pending device matches that code.",
  already_approved: "That code was already approved.",
};

export function describeApiError(error: unknown): string {
  if (error instanceof ApiError) {
    return (
      error.error_description ?? ERROR_SENTENCES[error.error] ?? error.message
    );
  }
  if (error instanceof Error) {
    // A failed fetch surfaces as a bare TypeError with no useful detail.
    if (error.name === "TypeError") {
      return `Could not reach the API at ${API_BASE}.`;
    }
    return error.message;
  }
  return "Request failed";
}
