import type { Context } from "hono";
import type { ContentfulStatusCode } from "hono/utils/http-status";
import type { ErrorBody } from "../contracts";

export function jsonError(
  c: Context,
  status: ContentfulStatusCode,
  error: string,
  errorDescription?: string,
  extra?: Record<string, unknown>,
) {
  const body: ErrorBody & Record<string, unknown> = { error };
  if (errorDescription !== undefined) {
    body.error_description = errorDescription;
  }
  if (extra) {
    Object.assign(body, extra);
  }
  return c.json(body, status);
}

export function iso(date: Date): string {
  return date.toISOString();
}
