import { and, desc, eq } from "drizzle-orm";
import { Hono } from "hono";
import type { DeviceRecord, DeviceRevokeResponse, DevicesListResponse } from "../contracts";
import { db } from "../db";
import { devices } from "../db/schema";
import type { AuthVariables } from "../lib/auth";
import { getAuth, requireAuth } from "../lib/auth";
import { iso, jsonError } from "../lib/http";

export const deviceRoutes = new Hono<{ Variables: AuthVariables }>();

deviceRoutes.use("/devices", requireAuth);
deviceRoutes.use("/devices/*", requireAuth);

deviceRoutes.get("/devices", async (c) => {
  const auth = getAuth(c);
  const rows = await db
    .select()
    .from(devices)
    .where(eq(devices.userId, auth.userId))
    .orderBy(desc(devices.createdAt));

  const body: DevicesListResponse = {
    devices: rows.map((row): DeviceRecord => ({
      id: row.id,
      name: row.name,
      created_at: iso(row.createdAt),
      revoked_at: row.revokedAt ? iso(row.revokedAt) : null,
      current: auth.deviceId === row.id,
    })),
  };
  return c.json(body);
});

deviceRoutes.delete("/devices/:id", async (c) => {
  const auth = getAuth(c);
  const id = c.req.param("id");
  const rows = await db
    .select()
    .from(devices)
    .where(and(eq(devices.id, id), eq(devices.userId, auth.userId)))
    .limit(1);
  const device = rows[0];
  if (!device) {
    return jsonError(c, 404, "device_not_found");
  }

  if (!device.revokedAt) {
    await db
      .update(devices)
      .set({ revokedAt: new Date() })
      .where(eq(devices.id, device.id));
  }

  const body: DeviceRevokeResponse = { revoked: true };
  return c.json(body);
});
