import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { app } from "./app";
import { DEVICE_GRANT_TYPE, DEVICE_TOKEN_PREFIX } from "./contracts";
import { sql } from "./db";
import { migrate, resetForTests } from "./db/migrate";
import { sha256Hex } from "./lib/hash";
import { computeTreeHash } from "./lib/tree";

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

    const health = await app.request("/health");
    expect(health.status).toBe(200);
    expect(await json(health)).toEqual({ ok: true });

    const start = await app.request("/auth/device/code", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ client_name: "laptop" }),
    });
    expect(start.status).toBe(200);
    const started = await json(start);
    const deviceCode = String(started.device_code);
    const userCode = String(started.user_code);
    expect(started.verification_uri).toContain("/device");
    expect(started.verification_uri_complete).toContain("user_code=");
    expect(started.interval).toBe(5);

    const pending = await app.request("/auth/device/token", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        device_code: deviceCode,
        grant_type: DEVICE_GRANT_TYPE,
      }),
    });
    expect(pending.status).toBe(400);
    expect(await json(pending)).toMatchObject({ error: "authorization_pending" });

    const approve = await app.request("/auth/device/approve", {
      method: "POST",
      headers: { "content-type": "application/json", ...clerkAuth },
      body: JSON.stringify({ user_code: userCode }),
    });
    expect(approve.status).toBe(200);
    const approved = await json(approve);
    expect(approved.ok).toBe(true);
    const deviceId = String(approved.device_id);
    expect(deviceId).toBeTruthy();

    const tokenRes = await app.request("/auth/device/token", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        device_code: deviceCode,
        grant_type: DEVICE_GRANT_TYPE,
      }),
    });
    expect(tokenRes.status).toBe(200);
    const issued = await json(tokenRes);
    expect(issued.refresh_token).toBeUndefined();
    expect(issued.token_type).toBeUndefined();
    const accessToken = String(issued.access_token);
    expect(accessToken.startsWith(DEVICE_TOKEN_PREFIX)).toBe(true);
    const deviceAuth = { Authorization: `Bearer ${accessToken}` };

    const content = new TextEncoder().encode("# hello\n");
    const blobHash = sha256Hex(content);
    const treeHash = computeTreeHash({ "SKILL.md": blobHash });

    const emptySync = await app.request("/sync", {
      method: "POST",
      headers: { "content-type": "application/json", ...deviceAuth },
      body: JSON.stringify({
        skills: { greeter: { tree_hash: treeHash, files: { "SKILL.md": blobHash } } },
      }),
    });
    expect(emptySync.status).toBe(200);
    expect(await json(emptySync)).toEqual({
      upload: [blobHash],
      download: [],
      conflicts: [],
      missing_skills: [],
    });

    const putBlob = await app.request(`/blobs/${blobHash}`, {
      method: "PUT",
      headers: { ...deviceAuth, "content-type": "application/json" },
      body: JSON.stringify({ content_base64: Buffer.from(content).toString("base64") }),
    });
    expect(putBlob.status).toBe(201);

    const putTree = await app.request("/skills/greeter/tree", {
      method: "PUT",
      headers: { "content-type": "application/json", ...deviceAuth },
      body: JSON.stringify({
        tree_hash: treeHash,
        files: { "SKILL.md": blobHash },
      }),
    });
    expect(putTree.status).toBe(200);
    expect((await json(putTree)).tree_hash).toBe(treeHash);

    const listed = await app.request("/skills", { headers: deviceAuth });
    expect(listed.status).toBe(200);
    const listBody = await json(listed);
    const skills = listBody.skills as Array<{ name: string; updated_at: string }>;
    expect(skills[0]).toMatchObject({ name: "greeter", tree_hash: treeHash });
    expect(skills[0]?.updated_at).toBeTruthy();

    const detail = await app.request("/skills/greeter", { headers: deviceAuth });
    expect(await json(detail)).toMatchObject({
      name: "greeter",
      tree_hash: treeHash,
      files: { "SKILL.md": blobHash },
    });

    const machineB = await app.request("/sync", {
      method: "POST",
      headers: { "content-type": "application/json", ...clerkAuth },
      body: JSON.stringify({ skills: {} }),
    });
    expect(machineB.status).toBe(200);
    const pull = await json(machineB);
    expect(pull.missing_skills).toEqual(["greeter"]);
    expect(pull.download).toEqual([
      { hash: blobHash, skills: ["greeter"], paths: ["SKILL.md"] },
    ]);

    const otherHash = sha256Hex("other");
    const conflictRes = await app.request("/sync", {
      method: "POST",
      headers: { "content-type": "application/json", ...deviceAuth },
      body: JSON.stringify({
        skills: { greeter: { tree_hash: otherHash, files: { "SKILL.md": otherHash } } },
      }),
    });
    expect(conflictRes.status).toBe(200);
    const conflictBody = await json(conflictRes);
    expect(conflictBody.conflicts).toEqual([
      {
        skill: "greeter",
        local_tree_hash: otherHash,
        remote_tree_hash: treeHash,
      },
    ]);

    const devicesRes = await app.request("/devices", { headers: deviceAuth });
    expect(devicesRes.status).toBe(200);
    const devices = (await json(devicesRes)).devices as Array<{
      id: string;
      last_used_at: string | null;
    }>;
    expect(devices[0]?.id).toBe(deviceId);
    expect(devices[0]?.last_used_at).toBeTruthy();

    const revoke = await app.request(`/devices/${deviceId}`, {
      method: "DELETE",
      headers: clerkAuth,
    });
    expect(revoke.status).toBe(204);

    const denied = await app.request("/skills", { headers: deviceAuth });
    expect(denied.status).toBe(401);
  });
});
