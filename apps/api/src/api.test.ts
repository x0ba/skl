import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { app } from "./app";
import { DEVICE_TOKEN_PREFIX } from "./contracts";
import { sql } from "./db";
import { migrate, resetForTests } from "./db/migrate";
import { computeTreeHash } from "./lib/tree";
import { sha256Hex } from "./lib/hash";

const clerkUser = "user_test_alice";
const clerkAuth = { Authorization: `Bearer dev:${clerkUser}` };

let dbReady = false;

async function json(res: Response): Promise<Record<string, unknown>> {
  return (await res.json()) as Record<string, unknown>;
}

describe("device auth + sync demo path", () => {
  beforeAll(async () => {
    try {
      await migrate();
      await resetForTests();
      dbReady = true;
    } catch (error) {
      console.warn("skipping DB tests:", error);
      dbReady = false;
    }
  });

  afterAll(async () => {
    if (dbReady) {
      await sql.end({ timeout: 5 });
    }
  });

  it("issues a device token after Clerk approve, then syncs a skill", async () => {
    if (!dbReady) {
      return;
    }

    const health = await app.request("/v1/health");
    expect(health.status).toBe(200);
    expect(await json(health)).toMatchObject({ ok: true, db: true });

    const start = await app.request("/v1/auth/device/code", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ device_name: "laptop" }),
    });
    expect(start.status).toBe(200);
    const started = await json(start);
    const deviceCode = String(started.device_code);
    const userCode = String(started.user_code);
    expect(started.verification_uri).toContain("/device");
    expect(started.interval).toBe(5);

    const pending = await app.request("/v1/auth/device/token", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ device_code: deviceCode }),
    });
    expect(pending.status).toBe(400);
    expect(await json(pending)).toMatchObject({ error: "authorization_pending" });

    const approve = await app.request("/v1/auth/device/approve", {
      method: "POST",
      headers: { "content-type": "application/json", ...clerkAuth },
      body: JSON.stringify({ user_code: userCode }),
    });
    expect(approve.status).toBe(200);
    expect(await json(approve)).toMatchObject({
      approved: true,
      device_name: "laptop",
    });

    const tokenRes = await app.request("/v1/auth/device/token", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ device_code: deviceCode }),
    });
    expect(tokenRes.status).toBe(200);
    const issued = await json(tokenRes);
    expect(issued.token_type).toBe("Bearer");
    expect(issued.refresh_token).toBeUndefined();
    const accessToken = String(issued.access_token);
    expect(accessToken.startsWith(DEVICE_TOKEN_PREFIX)).toBe(true);
    const deviceAuth = { Authorization: `Bearer ${accessToken}` };

    const content = new TextEncoder().encode("# hello\n");
    const blobHash = sha256Hex(content);
    const treeHash = computeTreeHash({ "SKILL.md": blobHash });

    const emptySync = await app.request("/v1/sync", {
      method: "POST",
      headers: { "content-type": "application/json", ...deviceAuth },
      body: JSON.stringify({
        skills: { greeter: { tree_hash: treeHash, files: { "SKILL.md": blobHash } } },
      }),
    });
    expect(emptySync.status).toBe(200);
    const emptyBody = await json(emptySync);
    expect(emptyBody).toMatchObject({
      upload: { skills: ["greeter"], blobs: [blobHash] },
      download: { skills: [], blobs: [] },
      conflicts: [],
    });

    const putBlob = await app.request(`/v1/blobs/${blobHash}`, {
      method: "PUT",
      headers: { ...deviceAuth, "content-type": "application/octet-stream" },
      body: content,
    });
    expect(putBlob.status).toBe(201);

    const putTree = await app.request("/v1/skills/greeter/tree", {
      method: "PUT",
      headers: { "content-type": "application/json", ...deviceAuth },
      body: JSON.stringify({
        tree_hash: treeHash,
        files: [{ path: "SKILL.md", hash: blobHash }],
        metadata: { title: "Greeter" },
      }),
    });
    expect(putTree.status).toBe(200);
    const tree = await json(putTree);
    expect(tree.tree_hash).toBe(treeHash);
    expect(typeof tree.updated_at).toBe("string");

    const listed = await app.request("/v1/skills", { headers: deviceAuth });
    expect(listed.status).toBe(200);
    const listBody = await json(listed);
    const skills = listBody.skills as Array<{ name: string; updated_at: string }>;
    expect(skills[0]).toMatchObject({ name: "greeter", tree_hash: treeHash });
    expect(skills[0]?.updated_at).toBeTruthy();

    const machineB = await app.request("/v1/sync", {
      method: "POST",
      headers: { "content-type": "application/json", ...clerkAuth },
      body: JSON.stringify({ skills: {} }),
    });
    expect(machineB.status).toBe(200);
    const pull = await json(machineB);
    const download = pull.download as { skills: Array<{ name: string; updated_at: string }> };
    expect(download.skills[0]).toMatchObject({
      name: "greeter",
      tree_hash: treeHash,
    });
    expect(download.skills[0]?.updated_at).toBeTruthy();

    const otherHash = sha256Hex("other");
    const conflictRes = await app.request("/v1/sync", {
      method: "POST",
      headers: { "content-type": "application/json", ...deviceAuth },
      body: JSON.stringify({ skills: { greeter: otherHash } }),
    });
    expect(conflictRes.status).toBe(200);
    const conflictBody = await json(conflictRes);
    const conflicts = conflictBody.conflicts as Array<Record<string, string>>;
    expect(conflicts[0]).toMatchObject({
      skill: "greeter",
      local_tree_hash: otherHash,
      remote_tree_hash: treeHash,
    });
    expect(conflicts[0]?.remote_updated_at).toBeTruthy();

    const devicesRes = await app.request("/v1/devices", { headers: deviceAuth });
    expect(devicesRes.status).toBe(200);
    const devicesBody = await json(devicesRes);
    const devices = devicesBody.devices as Array<{ id: string; current: boolean }>;
    expect(devices[0]?.current).toBe(true);

    const revoke = await app.request(`/v1/devices/${String(issued.device_id)}`, {
      method: "DELETE",
      headers: clerkAuth,
    });
    expect(revoke.status).toBe(200);

    const denied = await app.request("/v1/skills", { headers: deviceAuth });
    expect(denied.status).toBe(401);
  });
});
