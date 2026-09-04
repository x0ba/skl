import { Hono } from "hono";
import type { HealthResponse } from "../contracts";
import { sql } from "../db";

export const healthRoutes = new Hono();

healthRoutes.get("/health", async (c) => {
  try {
    await sql`SELECT 1`;
  } catch {
    return c.json({ ok: false as const }, 503);
  }
  const body: HealthResponse = { ok: true };
  return c.json(body, 200);
});
