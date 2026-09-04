import { zValidator } from "@hono/zod-validator";
import { and, desc, eq } from "drizzle-orm";
import { Hono } from "hono";
import { z } from "zod";
import type {
  PutSkillTreeResponse,
  SkillDetailResponse,
  SkillsListResponse,
} from "../contracts";
import { db } from "../db";
import { skills } from "../db/schema";
import type { AuthVariables } from "../lib/auth";
import { getAuth, requireAuth } from "../lib/auth";
import { iso, jsonError } from "../lib/http";
import {
  listSkillFiles,
  normalizeFileMap,
  parseSkillName,
  putSkillTree,
  SkillError,
} from "../lib/skills";

const fileList = z.array(
  z.object({
    path: z.string().min(1),
    hash: z.string().min(1),
  }),
);

const putTreeBody = z.object({
  tree_hash: z.string().min(1),
  metadata: z.record(z.string(), z.unknown()).optional(),
  files: z.union([fileList, z.record(z.string(), z.string())]),
});

export const skillRoutes = new Hono<{ Variables: AuthVariables }>();

skillRoutes.use("/skills", requireAuth);
skillRoutes.use("/skills/*", requireAuth);

function handleSkillError(c: Parameters<typeof jsonError>[0], error: unknown) {
  if (error instanceof SkillError) {
    return jsonError(
      c,
      error.status,
      error.message,
      undefined,
      error.extra,
    );
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
      const result = await putSkillTree(
        auth,
        name,
        input.tree_hash,
        files,
        input.metadata ?? {},
      );
      const body: PutSkillTreeResponse = {
        name: result.name,
        version_id: result.version_id,
        tree_hash: result.tree_hash,
        updated_at: iso(result.updated_at),
      };
      return c.json(body);
    } catch (error) {
      return handleSkillError(c, error);
    }
  },
);

skillRoutes.get("/skills", async (c) => {
  const auth = getAuth(c);
  const rows = await db
    .select()
    .from(skills)
    .where(eq(skills.userId, auth.userId))
    .orderBy(desc(skills.updatedAt));

  const body: SkillsListResponse = {
    skills: rows
      .filter((row) => row.currentVersionId && row.currentTreeHash)
      .map((row) => ({
        name: row.name,
        tree_hash: row.currentTreeHash as string,
        version_id: row.currentVersionId as string,
        metadata: row.metadata,
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
      .where(and(eq(skills.userId, auth.userId), eq(skills.name, name)))
      .limit(1);
    const skill = rows[0];
    if (!skill || !skill.currentVersionId || !skill.currentTreeHash) {
      return jsonError(c, 404, "skill_not_found");
    }
    const files = await listSkillFiles(skill.currentVersionId);
    const body: SkillDetailResponse = {
      name: skill.name,
      tree_hash: skill.currentTreeHash,
      version_id: skill.currentVersionId,
      metadata: skill.metadata,
      updated_at: iso(skill.updatedAt),
      files,
    };
    return c.json(body);
  } catch (error) {
    return handleSkillError(c, error);
  }
});
