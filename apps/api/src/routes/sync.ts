import { zValidator } from "@hono/zod-validator";
import { and, eq, isNull } from "drizzle-orm";
import { Hono } from "hono";
import { z } from "zod";
import type { ClientSkillState, SyncConflict, SyncDownloadBlob, SyncResponse } from "../contracts";
import { db } from "../db";
import { skills } from "../db/schema";
import type { AuthVariables } from "../lib/auth";
import { getAuth, requireAuth } from "../lib/auth";
import { iso, jsonError } from "../lib/http";
import {
  listSkillFilesByVersion,
  missingBlobHashes,
  normalizeFileMap,
  SkillError,
} from "../lib/skills";
import { isSha256Hex, normalizeHash } from "../lib/hash";
import { computeTreeHash, isSkillName } from "../lib/tree";

const clientSkillState = z.object({
  tree_hash: z.string().min(1),
  files: z.record(z.string(), z.string()),
});

const syncBody = z.object({
  skills: z.record(z.string(), clientSkillState),
});

function normalizeClientState(name: string, value: ClientSkillState): ClientSkillState {
  if (!isSkillName(name)) {
    throw new SkillError("invalid_skill_name", 400, { skill: name });
  }
  const treeHash = normalizeHash(value.tree_hash);
  if (!isSha256Hex(treeHash)) {
    throw new SkillError("invalid_hash", 400, { skill: name });
  }
  return {
    tree_hash: treeHash,
    files: normalizeFileMap(value.files),
  };
}

export const syncRoutes = new Hono<{ Variables: AuthVariables }>();

syncRoutes.post("/sync", requireAuth, zValidator("json", syncBody), async (c) => {
  try {
    const auth = getAuth(c);
    const input = c.req.valid("json");

    const clientSkills = new Map<string, ClientSkillState>();
    const clientBlobHashes = new Set<string>();
    for (const [name, value] of Object.entries(input.skills)) {
      const state = normalizeClientState(name, value);
      clientSkills.set(name, state);
      for (const hash of Object.values(state.files)) {
        clientBlobHashes.add(hash);
      }
    }

    const serverRows = await db
      .select({ name: skills.name, currentTreeHash: skills.currentTreeHash, currentVersionId: skills.currentVersionId, updatedAt: skills.updatedAt })
      .from(skills)
      .where(and(eq(skills.userId, auth.userId), isNull(skills.deletedAt)));

    const serverByName = new Map(
      serverRows
        .filter((row) => row.currentVersionId && row.currentTreeHash)
        .map((row) => [row.name, row]),
    );

    // A matching, verified manifest already proves all its blobs exist. Avoid
    // reading its files or looking up its blob hashes again on a no-op sync.
    const upToDate = new Set<string>();
    for (const [name, state] of clientSkills) {
      if (serverByName.get(name)?.currentTreeHash === state.tree_hash &&
          computeTreeHash(state.files) === state.tree_hash) {
        upToDate.add(name);
      }
    }
    const versions = [...serverByName.values()]
      .filter((row) => !upToDate.has(row.name) &&
        (!clientSkills.has(row.name) || clientSkills.get(row.name)?.tree_hash === row.currentTreeHash))
      .map((row) => row.currentVersionId!);
    const candidateHashes = [...clientSkills.entries()]
      .filter(([name]) => !upToDate.has(name))
      .flatMap(([, state]) => Object.values(state.files));
    const [filesByVersion, missingHashes] = await Promise.all([
      listSkillFilesByVersion(versions),
      missingBlobHashes(candidateHashes),
    ]);

    const conflicts: SyncConflict[] = [];
    const missingSkills: string[] = [];
    const downloadIndex = new Map<string, { skills: Set<string>; paths: Set<string> }>();

    function addDownload(hash: string, skill: string, path: string): void {
      const entry = downloadIndex.get(hash) ?? {
        skills: new Set<string>(),
        paths: new Set<string>(),
      };
      entry.skills.add(skill);
      entry.paths.add(path);
      downloadIndex.set(hash, entry);
    }

    for (const [name, state] of clientSkills) {
      if (upToDate.has(name)) continue;
      const remote = serverByName.get(name);
      if (!remote || !remote.currentTreeHash || !remote.currentVersionId) {
        continue;
      }
      if (remote.currentTreeHash === state.tree_hash) {
        const remoteFiles = filesByVersion.get(remote.currentVersionId) ?? [];
        for (const file of remoteFiles) {
          if (!clientBlobHashes.has(file.hash)) {
            addDownload(file.hash, name, file.path);
          }
        }
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
      missingSkills.push(name);
      const remoteFiles = filesByVersion.get(remote.currentVersionId) ?? [];
      for (const file of remoteFiles) {
        addDownload(file.hash, name, file.path);
      }
    }

    const upload = missingHashes.sort();
    const download: SyncDownloadBlob[] = [...downloadIndex.entries()]
      .map(([hash, refs]) => ({
        hash,
        skills: [...refs.skills].sort(),
        paths: [...refs.paths].sort(),
      }))
      .sort((a, b) => a.hash.localeCompare(b.hash));

    const body: SyncResponse = {
      up_to_date: [...upToDate].sort(),
      upload,
      download,
      conflicts: conflicts.sort((a, b) => a.skill.localeCompare(b.skill)),
      missing_skills: missingSkills.sort(),
    };
    return c.json(body);
  } catch (error) {
    if (error instanceof SkillError) {
      return jsonError(c, error.status, error.message, undefined, error.extra);
    }
    throw error;
  }
});
