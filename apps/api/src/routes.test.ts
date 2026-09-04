import { describe, expect, it } from "vitest";
import { app } from "./app";
import { API_ROUTES } from "./contracts";

const ALLOWED_ROOT = new Set(["/", "/*"]);

describe("route mount lock", () => {
  it("registers every API route under /v1 only", () => {
    const paths = app.routes.map((route) => route.path);
    const apiPaths = paths.filter((path) => !ALLOWED_ROOT.has(path));

    expect(apiPaths.length).toBeGreaterThan(0);
    for (const path of apiPaths) {
      expect(path.startsWith("/v1/"), `${path} is not under /v1`).toBe(true);
      expect(path.startsWith("/v1/v1"), `${path} is double-prefixed`).toBe(false);
    }

    const joined = paths.join("\n");
    for (const unversioned of [
      "\n/health",
      "\n/auth/",
      "\n/devices",
      "\n/sync",
      "\n/blobs",
      "\n/skills",
    ]) {
      expect(joined.includes(unversioned)).toBe(false);
    }
  });

  it("exports the locked /v1 contract paths", () => {
    for (const path of Object.values(API_ROUTES)) {
      expect(path.startsWith("/v1/")).toBe(true);
    }
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

  it("serves GET / as a pointer, not the health endpoint", async () => {
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { prefix: string; health: string };
    expect(body.prefix).toBe("/v1");
    expect(body.health).toBe("/v1/health");
  });

  it("does not serve unversioned /health", async () => {
    const res = await app.request("/health");
    expect(res.status).toBe(404);
  });
});
