import { readdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { sql } from "./index";

export async function migrate(): Promise<void> {
  const dir = join(dirname(fileURLToPath(import.meta.url)), "../../drizzle");
  const files = (await readdir(dir))
    .filter((name) => name.endsWith(".sql"))
    .sort();

  await sql`
    CREATE TABLE IF NOT EXISTS schema_migrations (
      id text PRIMARY KEY,
      applied_at timestamptz NOT NULL DEFAULT now()
    )
  `;

  for (const file of files) {
    const applied = await sql<{ id: string }[]>`
      SELECT id FROM schema_migrations WHERE id = ${file}
    `;
    if (applied.length > 0) {
      continue;
    }
    const contents = await readFile(join(dir, file), "utf8");
    await sql.begin(async (tx) => {
      await tx.unsafe(contents);
      await tx`INSERT INTO schema_migrations (id) VALUES (${file})`;
    });
    console.log(`applied migration ${file}`);
  }
}

export async function resetForTests(): Promise<void> {
  await sql.unsafe(`
    TRUNCATE
      skill_files,
      skill_versions,
      skills,
      blobs,
      device_authorizations,
      devices,
      users
    RESTART IDENTITY CASCADE
  `);
}
