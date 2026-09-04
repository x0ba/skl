export const API_BASE = (
  process.env.NEXT_PUBLIC_API_BASE ?? "http://localhost:8787"
).replace(/\/$/, "");

export const DEFAULT_DEV_USER_ID =
  process.env.NEXT_PUBLIC_DEV_USER_ID?.trim() || "local-dev";

export const DEFAULT_DEV_TOKEN = `dev:${DEFAULT_DEV_USER_ID}`;

export function isClerkEnabled(): boolean {
  return Boolean(process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim());
}
