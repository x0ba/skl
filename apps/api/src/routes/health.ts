import { Hono } from "hono";
import type { HealthResponse } from "../contracts";
import { sql } from "../db";

export const healthRoutes = new Hono();

healthRoutes.get("/health", async (c) => {
  let dbUp = false;
  try {
    await sql`SELECT 1`;
    dbUp = true;
  } catch {
    dbUp = false;
  }
  const body: HealthResponse = { ok: true, db: dbUp };
  return c.json(body, dbUp ? 200 : 503);
});
