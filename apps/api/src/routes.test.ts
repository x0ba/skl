import { describe, expect, it } from "vitest";
import { app } from "./app";
import { API_ROUTES } from "./contracts";

const LOCKED = [
  "GET /v1/health",
  "POST /v1/auth/device/code",
  "POST /v1/auth/device/token",
  "POST /v1/auth/device/approve",
  "GET /v1/devices",
  "DELETE /v1/devices/:id",
  "POST /v1/sync",
  "PUT /v1/blobs/:hash",
  "GET /v1/blobs/:hash",
  "PUT /v1/skills/:name/tree",
  "GET /v1/skills",
  "GET /v1/skills/:name",
];

const ALLOWED_ROOT = new Set(["/", "/*"]);

describe("route mount lock", () => {
  it("registers every API route under /v1 only", () => {
    const table = app.routes.map((route) => `${route.method} ${route.path}`);
    for (const route of LOCKED) {
      expect(table, `missing ${route}`).toContain(route);
    }
    for (const entry of table) {
      const path = entry.split(" ")[1] ?? "";
      if (ALLOWED_ROOT.has(path)) {
        continue;
      }
      expect(path.startsWith("/v1/"), `${entry} is not under /v1`).toBe(true);
    }
  });

  it("exports the locked /v1 contract paths", () => {
    expect(API_ROUTES.health).toBe("/v1/health");
    expect(API_ROUTES.deviceCode).toBe("/v1/auth/device/code");
    expect(API_ROUTES.deviceToken).toBe("/v1/auth/device/token");
    expect(API_ROUTES.deviceApprove).toBe("/v1/auth/device/approve");
    expect(API_ROUTES.devices).toBe("/v1/devices");
    expect(API_ROUTES.sync).toBe("/v1/sync");
    expect(API_ROUTES.blob).toBe("/v1/blobs/:hash");
    expect(API_ROUTES.skillTree).toBe("/v1/skills/:name/tree");
    expect(API_ROUTES.skills).toBe("/v1/skills");
    expect(API_ROUTES.skill).toBe("/v1/skills/:name");
  });

  it("serves GET /v1/health", async () => {
    const res = await app.request("/v1/health");
    expect([200, 503]).toContain(res.status);
  });

  it("does not serve unversioned /health", async () => {
    const res = await app.request("/health");
    expect(res.status).toBe(404);
  });
});
