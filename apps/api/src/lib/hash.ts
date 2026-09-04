import { createHash, randomBytes, timingSafeEqual } from "node:crypto";

export function sha256Hex(data: Uint8Array | string): string {
  const hash = createHash("sha256");
  hash.update(data);
  return hash.digest("hex");
}

export function hashToken(token: string): string {
  return sha256Hex(token);
}

export function randomHex(bytes: number): string {
  return randomBytes(bytes).toString("hex");
}

export function randomBase64Url(bytes: number): string {
  return randomBytes(bytes).toString("base64url");
}

export function hexEqual(a: string, b: string): boolean {
  const left = Buffer.from(a, "utf8");
  const right = Buffer.from(b, "utf8");
  if (left.length !== right.length) {
    return false;
  }
  return timingSafeEqual(left, right);
}

const HASH_RE = /^[a-f0-9]{64}$/;

export function isSha256Hex(value: string): boolean {
  return HASH_RE.test(value);
}

export function normalizeHash(value: string): string {
  return value.trim().toLowerCase();
}
