// Use a disposable local database. This script seeds only its own user and cleans it up.
import { randomUUID } from "node:crypto";
import { app } from "../src/app";
import { sql } from "../src/db";
import { migrate } from "../src/db/migrate";
import { computeTreeHash } from "../src/lib/tree";
import { sha256Hex } from "../src/lib/hash";

const url = new URL(process.env.DATABASE_URL ?? "");
if (!["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)) {
  throw new Error("Benchmark requires an explicitly configured local DATABASE_URL");
}
await migrate();
const userId = randomUUID();
const clerkId = `perf-${userId}`;
const skills: Record<string, { tree_hash: string; files: Record<string, string> }> = {};
const hash = sha256Hex(Buffer.from("performance fixture"));
await sql`INSERT INTO users (id, clerk_user_id) VALUES (${userId}, ${clerkId})`;
try {
  await sql`INSERT INTO blobs (hash, content, size_bytes) VALUES (${hash}, ${Buffer.from("performance fixture")}, 19) ON CONFLICT DO NOTHING`;
  for (let i = 0; i < 100; i++) {
    const skillId = randomUUID();
    const versionId = randomUUID();
    const files = Object.fromEntries(Array.from({ length: 10 }, (_, j) => [`file-${j}.md`, hash]));
    const treeHash = computeTreeHash(files);
    skills[`skill-${i}`] = { tree_hash: treeHash, files };
    await sql`INSERT INTO skills (id, user_id, name, current_version_id, current_tree_hash) VALUES (${skillId}, ${userId}, ${`skill-${i}`}, ${versionId}, ${treeHash})`;
    await sql`INSERT INTO skill_versions (id, skill_id, tree_hash) VALUES (${versionId}, ${skillId}, ${treeHash})`;
    await sql`INSERT INTO skill_files ${sql(Object.keys(files).map(path => ({ version_id: versionId, path, content_hash: hash })))}`;
  }
  for (const [scenario, state] of [["unchanged-100-skills", skills], ["pull-100-skills", {}]] as const) {
    const samples = [];
    let queries = 0;
    sql.options.debug = () => { queries++; };
    for (let i = 0; i < 6; i++) {
      queries = 0;
      const start = performance.now();
      const response = await app.request("/v1/sync", {
        method: "POST", headers: { Authorization: `Bearer dev:${clerkId}`, "Content-Type": "application/json" },
        body: JSON.stringify({ skills: state }),
      });
      if (response.status !== 200) throw new Error(await response.text());
      await response.json();
      if (i > 0) samples.push(performance.now() - start);
    }
    samples.sort((a, b) => a - b);
    console.log(JSON.stringify({ scenario, queries, median_ms: Number(samples[2].toFixed(2)) }));
  }
} finally {
  sql.options.debug = false;
  await sql`DELETE FROM users WHERE id = ${userId}`;
  await sql.end();
}
