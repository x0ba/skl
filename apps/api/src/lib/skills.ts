import { and, eq, inArray } from "drizzle-orm";
import { db } from "../db";
import { blobs, skillFiles, skills, skillVersions } from "../db/schema";
import type { AuthContext } from "./auth";
import { computeTreeHash, filesToList, filesToRecord, isSafeFilePath, isSkillName } from "./tree";
import { isSha256Hex, normalizeHash } from "./hash";

export class SkillError extends Error {
  readonly status: 400 | 404 | 409;
  readonly extra?: Record<string, unknown>;

  constructor(
    message: string,
    status: 400 | 404 | 409 = 400,
    extra?: Record<string, unknown>,
  ) {
    super(message);
    this.name = "SkillError";
    this.status = status;
    this.extra = extra;
  }
}

export function parseSkillName(name: string): string {
  const decoded = decodeURIComponent(name);
  if (!isSkillName(decoded)) {
    throw new SkillError("invalid_skill_name");
  }
  return decoded;
}

export function normalizeFileMap(
  files: { path: string; hash: string }[] | Record<string, string>,
): Record<string, string> {
  const record = filesToRecord(files);
  const normalized: Record<string, string> = {};
  for (const [path, hash] of Object.entries(record)) {
    if (!isSafeFilePath(path)) {
      throw new SkillError("invalid_file_path", 400, { path });
    }
    const nextHash = normalizeHash(hash);
    if (!isSha256Hex(nextHash)) {
      throw new SkillError("invalid_hash", 400, { path, hash });
    }
    normalized[path] = nextHash;
  }
  return normalized;
}

export async function missingBlobHashes(hashes: string[]): Promise<string[]> {
  const unique = [...new Set(hashes)];
  if (unique.length === 0) {
    return [];
  }
  const existing = await db
    .select({ hash: blobs.hash })
    .from(blobs)
    .where(inArray(blobs.hash, unique));
  const have = new Set(existing.map((row) => row.hash));
  return unique.filter((hash) => !have.has(hash));
}

export async function putSkillTree(
  auth: AuthContext,
  name: string,
  treeHash: string,
  files: Record<string, string>,
  metadata: Record<string, unknown>,
) {
  const computed = computeTreeHash(files);
  const expected = normalizeHash(treeHash);
  if (!isSha256Hex(expected)) {
    throw new SkillError("invalid_hash");
  }
  if (computed !== expected) {
    throw new SkillError("tree_hash_mismatch", 400, {
      expected: computed,
    });
  }

  const missing = await missingBlobHashes(Object.values(files));
  if (missing.length > 0) {
    throw new SkillError("missing_blobs", 400, { hashes: missing });
  }

  const existing = await db
    .select()
    .from(skills)
    .where(and(eq(skills.userId, auth.userId), eq(skills.name, name)))
    .limit(1);
  let skill = existing[0];

  if (skill && skill.currentTreeHash === expected && skill.currentVersionId) {
    return {
      name: skill.name,
      version_id: skill.currentVersionId,
      tree_hash: expected,
      updated_at: skill.updatedAt,
    };
  }

  const now = new Date();
  if (!skill) {
    const inserted = await db
      .insert(skills)
      .values({
        userId: auth.userId,
        name,
        metadata,
        currentTreeHash: expected,
        updatedAt: now,
      })
      .returning();
    const created = inserted[0];
    if (!created) {
      throw new SkillError("skill_create_failed");
    }
    skill = created;
  } else {
    await db
      .update(skills)
      .set({ metadata, updatedAt: now })
      .where(eq(skills.id, skill.id));
  }

  const versions = await db
    .insert(skillVersions)
    .values({
      skillId: skill.id,
      treeHash: expected,
      createdByDeviceId: auth.deviceId,
      metadata,
    })
    .returning();
  const version = versions[0];
  if (!version) {
    throw new SkillError("version_create_failed");
  }

  const fileRows = filesToList(files).map((file) => ({
    versionId: version.id,
    path: file.path,
    contentHash: file.hash,
  }));
  if (fileRows.length > 0) {
    await db.insert(skillFiles).values(fileRows);
  }

  const updated = await db
    .update(skills)
    .set({
      currentVersionId: version.id,
      currentTreeHash: expected,
      metadata,
      updatedAt: now,
    })
    .where(eq(skills.id, skill.id))
    .returning();

  const next = updated[0] ?? skill;
  return {
    name: next.name,
    version_id: version.id,
    tree_hash: expected,
    updated_at: next.updatedAt,
  };
}

export async function listSkillFiles(versionId: string) {
  const rows = await db
    .select({
      path: skillFiles.path,
      hash: skillFiles.contentHash,
    })
    .from(skillFiles)
    .where(eq(skillFiles.versionId, versionId));
  return rows
    .map((row) => ({ path: row.path, hash: row.hash }))
    .sort((a, b) => a.path.localeCompare(b.path));
}
