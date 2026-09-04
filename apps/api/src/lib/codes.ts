import { randomInt } from "node:crypto";
import { randomBase64Url } from "./hash";

/** Crockford-ish alphabet: no I, O, 0, 1. */
const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

export function generateUserCode(): { display: string; normalized: string } {
  let normalized = "";
  for (let i = 0; i < 8; i += 1) {
    normalized += ALPHABET[randomInt(ALPHABET.length)];
  }
  const display = `${normalized.slice(0, 4)}-${normalized.slice(4)}`;
  return { display, normalized };
}

export function normalizeUserCode(input: string): string {
  return input.trim().toUpperCase().replace(/[^A-Z0-9]/g, "");
}

export function generateDeviceCode(): string {
  return randomBase64Url(32);
}

export const DEFAULT_DEVICE_EXPIRES_IN = 600;
export const DEFAULT_DEVICE_INTERVAL = 5;
