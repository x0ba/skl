import { randomUUID } from "node:crypto";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { app } from "./app";
import { sql } from "./db";
import { migrate } from "./db/migrate";
import { sha256Hex, hashToken } from "./lib/hash";
import { computeTreeHash } from "./lib/tree";

// No truncation: fixtures belong to one unique account and are removed afterwards.
describe.skipIf(!process.env.DATABASE_URL)("performance regressions (Postgres)", () => {
  const userId = randomUUID();
  const clerkId = `perf-test-${userId}`;
  const token = `skl_dt_${randomUUID()}`;
  const headers = { Authorization: `Bearer ${token}`, "Content-Type": "application/json" };
  const content = Buffer.from(`# Performance fixture ${userId}\n`);
  const hash = sha256Hex(content);
  const files = { "SKILL.md": hash };
  const treeHash = computeTreeHash(files);
  const state = Object.fromEntries(Array.from({ length: 30 }, (_, i) => [
    `skill-${i}`, { tree_hash: treeHash, files },
  ]));
  let queries: string[] = [];

  beforeAll(async () => {
    await migrate();
    await sql`INSERT INTO users (id, clerk_user_id) VALUES (${userId}, ${clerkId})`;
    await sql`INSERT INTO devices (user_id, name, token_hash, last_used_at)
      VALUES (${userId}, 'perf', ${hashToken(token)}, now())`;
    await sql`INSERT INTO blobs (hash, content, size_bytes) VALUES (${hash}, ${content}, ${content.length})`;
    for (const name of Object.keys(state)) {
      const skillId = randomUUID();
      const versionId = randomUUID();
      await sql`INSERT INTO skills (id, user_id, name, current_version_id, current_tree_hash)
        VALUES (${skillId}, ${userId}, ${name}, ${versionId}, ${treeHash})`;
      await sql`INSERT INTO skill_versions (id, skill_id, tree_hash) VALUES (${versionId}, ${skillId}, ${treeHash})`;
      await sql`INSERT INTO skill_files (version_id, path, content_hash) VALUES (${versionId}, 'SKILL.md', ${hash})`;
    }
    sql.options.debug = (_connection, query) => { queries.push(query); };
  });
  afterAll(async () => {
    sql.options.debug = false;
    await sql`DELETE FROM users WHERE id = ${userId}`;
    await sql`DELETE FROM blobs WHERE hash = ${hash}`;
    await sql.end();
  });
  async function sync(skills: unknown) {
    queries = [];
    const response = await app.request("/v1/sync", { method: "POST", headers, body: JSON.stringify({ skills }) });
    expect(response.status).toBe(200);
    return response.json();
  }

  it("plans an unchanged library in two queries without device writes", async () => {
    const body = await sync(state);
    expect(body.up_to_date).toEqual(Object.keys(state).sort());
    expect(body.upload).toEqual([]);
    expect(body.download).toEqual([]);
    expect(queries).toHaveLength(2);
  });

  it("loads all missing manifests in one query and deduplicates shared blobs", async () => {
    const body = await sync({});
    expect(body.missing_skills).toHaveLength(30);
    expect(body.download).toEqual([{ hash, paths: ["SKILL.md"], skills: Object.keys(state).sort() }]);
    expect(queries).toHaveLength(3);
  });

  it("does not trust a matching tree hash with an incomplete manifest", async () => {
    const body = await sync({ ...state, "skill-0": { tree_hash: treeHash, files: {} } });
    expect(body.up_to_date).not.toContain("skill-0");
    // The other manifests supply this shared blob; no duplicate download is needed.
    expect(body.download).toEqual([]);
    expect(queries).toHaveLength(3);
  });

  it("authenticates a skills listing once", async () => {
    queries = [];
    expect((await app.request("/v1/skills", { headers })).status).toBe(200);
    expect(queries).toHaveLength(2);
  });

  it("returns the optional preview together with its manifest", async () => {
    const response = await app.request("/v1/skills/skill-0?include=skill_md", { headers });
    expect(await response.json()).toMatchObject({ files, skill_md: content.toString("utf8") });
    const legacy = await app.request("/v1/skills/skill-0", { headers });
    expect(await legacy.json()).not.toHaveProperty("skill_md");
  });

  it("accepts concurrent duplicate blob uploads atomically", async () => {
    const bytes = Buffer.from(`concurrent-${userId}`);
    const digest = sha256Hex(bytes);
    try {
      const responses = await Promise.all(Array.from({ length: 6 }, () => app.request(`/v1/blobs/${digest}`, {
        method: "PUT", headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/octet-stream" }, body: bytes,
      })));
      expect(responses.map(r => r.status).sort()).toEqual([200, 200, 200, 200, 200, 201]);
    } finally {
      await sql`DELETE FROM blobs WHERE hash = ${digest}`;
    }
  });

  it("records activity at most once per interval and still honors immediate revocation", async () => {
    await sql`UPDATE devices SET last_used_at = NULL WHERE user_id = ${userId}`;
    await app.request("/v1/skills", { headers });
    queries = [];
    await app.request("/v1/skills", { headers });
    expect(queries.some(query => query.startsWith('update "devices"'))).toBe(false);
    await sql`UPDATE devices SET revoked_at = now() WHERE user_id = ${userId}`;
    expect((await app.request("/v1/skills", { headers })).status).toBe(401);
  });
});
