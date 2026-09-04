import { zValidator } from "@hono/zod-validator";
import { eq } from "drizzle-orm";
import { Hono } from "hono";
import { z } from "zod";
import type {
  ClientSkillState,
  SyncConflict,
  SyncDownloadSkill,
  SyncResponse,
} from "../contracts";
import { db } from "../db";
import { skills } from "../db/schema";
import type { AuthVariables } from "../lib/auth";
import { getAuth, requireAuth } from "../lib/auth";
import { iso, jsonError } from "../lib/http";
import {
  listSkillFiles,
  missingBlobHashes,
  normalizeFileMap,
  SkillError,
} from "../lib/skills";
import { isSkillName } from "../lib/tree";
import { isSha256Hex, normalizeHash } from "../lib/hash";

const clientSkillState = z.union([
  z.string().min(1),
  z.object({
    tree_hash: z.string().min(1),
    base_tree_hash: z.string().min(1).optional(),
    files: z.record(z.string(), z.string()).optional(),
  }),
]);

const syncBody = z.object({
  skills: z.record(z.string(), clientSkillState),
});

function normalizeClientState(
  name: string,
  value: string | ClientSkillState,
): ClientSkillState {
  if (!isSkillName(name)) {
    throw new SkillError("invalid_skill_name", 400, { skill: name });
  }
  const state: ClientSkillState =
    typeof value === "string" ? { tree_hash: value } : value;
  const treeHash = normalizeHash(state.tree_hash);
  if (!isSha256Hex(treeHash)) {
    throw new SkillError("invalid_hash", 400, { skill: name });
  }
  const base = state.base_tree_hash
    ? normalizeHash(state.base_tree_hash)
    : undefined;
  if (base !== undefined && !isSha256Hex(base)) {
    throw new SkillError("invalid_hash", 400, { skill: name, field: "base_tree_hash" });
  }
  return {
    tree_hash: treeHash,
    base_tree_hash: base,
    files: state.files ? normalizeFileMap(state.files) : undefined,
  };
}

export const syncRoutes = new Hono<{ Variables: AuthVariables }>();

syncRoutes.post("/sync", requireAuth, zValidator("json", syncBody), async (c) => {
  try {
    const auth = getAuth(c);
    const input = c.req.valid("json");

    const clientSkills = new Map<string, ClientSkillState>();
    for (const [name, value] of Object.entries(input.skills)) {
      clientSkills.set(name, normalizeClientState(name, value));
    }

    const serverRows = await db
      .select()
      .from(skills)
      .where(eq(skills.userId, auth.userId));

    const serverByName = new Map(
      serverRows
        .filter((row) => row.currentVersionId && row.currentTreeHash)
        .map((row) => [row.name, row]),
    );

    const uploadSkills: string[] = [];
    const downloadSkills: SyncDownloadSkill[] = [];
    const conflicts: SyncConflict[] = [];
    const clientBlobHashes = new Set<string>();
    const downloadBlobHashes = new Set<string>();

    for (const [name, state] of clientSkills) {
      if (state.files) {
        for (const hash of Object.values(state.files)) {
          clientBlobHashes.add(hash);
        }
      }

      const remote = serverByName.get(name);
      if (!remote || !remote.currentTreeHash || !remote.currentVersionId) {
        uploadSkills.push(name);
        continue;
      }

      if (remote.currentTreeHash === state.tree_hash) {
        continue;
      }

      if (state.base_tree_hash && state.base_tree_hash === remote.currentTreeHash) {
        uploadSkills.push(name);
        continue;
      }

      conflicts.push({
        skill: name,
        local_tree_hash: state.tree_hash,
        remote_tree_hash: remote.currentTreeHash,
        remote_updated_at: iso(remote.updatedAt),
      });
    }

    for (const [name, remote] of serverByName) {
      if (clientSkills.has(name)) {
        continue;
      }
      if (!remote.currentVersionId || !remote.currentTreeHash) {
        continue;
      }
      const files = await listSkillFiles(remote.currentVersionId);
      for (const file of files) {
        downloadBlobHashes.add(file.hash);
      }
      downloadSkills.push({
        name,
        tree_hash: remote.currentTreeHash,
        version_id: remote.currentVersionId,
        updated_at: iso(remote.updatedAt),
        files,
      });
    }

    const uploadBlobs = await missingBlobHashes([...clientBlobHashes]);
    const downloadBlobs: string[] = [];
    for (const hash of downloadBlobHashes) {
      if (!clientBlobHashes.has(hash)) {
        downloadBlobs.push(hash);
      }
    }

    const body: SyncResponse = {
      upload: {
        skills: uploadSkills.sort(),
        blobs: uploadBlobs.sort(),
      },
      download: {
        skills: downloadSkills.sort((a, b) => a.name.localeCompare(b.name)),
        blobs: downloadBlobs.sort(),
      },
      conflicts: conflicts.sort((a, b) => a.skill.localeCompare(b.skill)),
    };
    return c.json(body);
  } catch (error) {
    if (error instanceof SkillError) {
      return jsonError(c, error.status, error.message, undefined, error.extra);
    }
    throw error;
  }
});
