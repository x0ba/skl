import { describe, expect, it } from "vitest";
import { app } from "./app";
import { API_ROUTES } from "./contracts";

const LOCKED = [
  "GET /health",
  "POST /auth/device/code",
  "POST /auth/device/token",
  "POST /auth/device/approve",
  "GET /devices",
  "DELETE /devices/:id",
  "POST /sync",
  "PUT /blobs/:hash",
  "GET /blobs/:hash",
  "PUT /skills/:name/tree",
  "GET /skills",
  "GET /skills/:name",
];

describe("route mount lock", () => {
  it("registers the furnace contract paths and no /v1 aliases", () => {
    const table = app.routes.map((route) => `${route.method} ${route.path}`);
    for (const route of LOCKED) {
      expect(table, `missing ${route}`).toContain(route);
    }
    for (const entry of table) {
      const path = entry.split(" ")[1] ?? "";
      expect(path.startsWith("/v1"), `${entry} is versioned`).toBe(false);
    }
  });

  it("exports the locked unversioned contract paths", () => {
    expect(API_ROUTES.health).toBe("/health");
    expect(API_ROUTES.deviceCode).toBe("/auth/device/code");
    expect(API_ROUTES.deviceToken).toBe("/auth/device/token");
    expect(API_ROUTES.deviceApprove).toBe("/auth/device/approve");
    expect(API_ROUTES.devices).toBe("/devices");
    expect(API_ROUTES.sync).toBe("/sync");
    expect(API_ROUTES.blob).toBe("/blobs/:hash");
    expect(API_ROUTES.skillTree).toBe("/skills/:name/tree");
    expect(API_ROUTES.skills).toBe("/skills");
    expect(API_ROUTES.skill).toBe("/skills/:name");
  });

  it("serves GET /health", async () => {
    const res = await app.request("/health");
    expect([200, 503]).toContain(res.status);
  });

  it("does not serve /v1/health", async () => {
    const res = await app.request("/v1/health");
    expect(res.status).toBe(404);
  });
});
