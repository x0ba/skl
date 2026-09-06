import { zValidator } from "@hono/zod-validator";
import { and, desc, eq, isNull } from "drizzle-orm";
import { Hono } from "hono";
import { z } from "zod";
import type {
  PutSkillTreeResponse,
  SkillDetailResponse,
  SkillsListResponse,
} from "../contracts";
import { db } from "../db";
import { blobs, skillFiles, skills } from "../db/schema";
import type { AuthVariables } from "../lib/auth";
import { getAuth, requireAuth } from "../lib/auth";
import { iso, jsonError } from "../lib/http";
import {
  deleteSkill,
  listSkillFiles,
  normalizeFileMap,
  parseSkillName,
  putSkillTree,
  SkillError,
} from "../lib/skills";
import { filesToRecord } from "../lib/tree";

const putTreeBody = z.object({
  tree_hash: z.string().min(1),
  files: z.record(z.string(), z.string()),
});

export const skillRoutes = new Hono<{ Variables: AuthVariables }>();

// The wildcard also matches the collection route; register auth only once.
skillRoutes.use("/skills/*", requireAuth);

function handleSkillError(c: Parameters<typeof jsonError>[0], error: unknown) {
  if (error instanceof SkillError) {
    return jsonError(c, error.status, error.message, undefined, error.extra);
  }
  throw error;
}

skillRoutes.put(
  "/skills/:name/tree",
  zValidator("json", putTreeBody),
  async (c) => {
    try {
      const auth = getAuth(c);
      const name = parseSkillName(c.req.param("name"));
      const input = c.req.valid("json");
      const files = normalizeFileMap(input.files);
      const result = await putSkillTree(auth, name, input.tree_hash, files, {});
      const body: PutSkillTreeResponse = {
        name: result.name,
        tree_hash: result.tree_hash,
        updated_at: iso(result.updated_at),
      };
      return c.json(body);
    } catch (error) {
      return handleSkillError(c, error);
    }
  },
);

skillRoutes.delete("/skills/:name", async (c) => {
  try {
    const auth = getAuth(c);
    const name = parseSkillName(c.req.param("name"));
    await deleteSkill(auth, name);
    return c.body(null, 204);
  } catch (error) {
    return handleSkillError(c, error);
  }
});

skillRoutes.get("/skills", async (c) => {
  const auth = getAuth(c);
  const rows = await db
    .select({ name: skills.name, currentTreeHash: skills.currentTreeHash, updatedAt: skills.updatedAt })
    .from(skills)
    .where(and(eq(skills.userId, auth.userId), isNull(skills.deletedAt)))
    .orderBy(desc(skills.updatedAt));

  const body: SkillsListResponse = {
    skills: rows
      .filter((row) => row.currentTreeHash)
      .map((row) => ({
        name: row.name,
        tree_hash: row.currentTreeHash as string,
        updated_at: iso(row.updatedAt),
      })),
  };
  return c.json(body);
});

skillRoutes.get("/skills/:name", async (c) => {
  try {
    const auth = getAuth(c);
    const name = parseSkillName(c.req.param("name"));
    const rows = await db
      .select()
      .from(skills)
      .where(
        and(
          eq(skills.userId, auth.userId),
          eq(skills.name, name),
          isNull(skills.deletedAt),
        ),
      )
      .limit(1);
    const skill = rows[0];
    if (!skill || !skill.currentVersionId || !skill.currentTreeHash) {
      return jsonError(c, 404, "skill_not_found");
    }
    const includeSkillMd = c.req.query("include") === "skill_md";
    const [fileList, preview] = await Promise.all([
      listSkillFiles(skill.currentVersionId),
      includeSkillMd ? db.select({ content: blobs.content }).from(skillFiles)
        .innerJoin(blobs, eq(blobs.hash, skillFiles.contentHash))
        .where(and(eq(skillFiles.versionId, skill.currentVersionId), eq(skillFiles.path, "SKILL.md")))
        .limit(1) : Promise.resolve([]),
    ]);
    const files = filesToRecord(fileList);
    const body: SkillDetailResponse = {
      ...(includeSkillMd ? { skill_md: preview[0] ? Buffer.from(preview[0].content).toString("utf8") : null } : {}),
      name: skill.name,
      tree_hash: skill.currentTreeHash,
      files,
      updated_at: iso(skill.updatedAt),
    };
    return c.json(body);
  } catch (error) {
    return handleSkillError(c, error);
  }
});
