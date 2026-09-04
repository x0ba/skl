import type { SkillFile } from "../contracts";
import { sha256Hex } from "./hash";

export function filesToRecord(
  files: SkillFile[] | Record<string, string>,
): Record<string, string> {
  if (!Array.isArray(files)) {
    return { ...files };
  }
  const record: Record<string, string> = {};
  for (const file of files) {
    record[file.path] = file.hash;
  }
  return record;
}

export function filesToList(files: Record<string, string>): SkillFile[] {
  return Object.keys(files)
    .sort()
    .map((path) => {
      const hash = files[path];
      if (hash === undefined) {
        throw new Error(`Missing hash for path ${path}`);
      }
      return { path, hash };
    });
}

/**
 * Canonical tree hash. Must stay in lockstep with the spec in contracts.ts.
 */
export function computeTreeHash(files: Record<string, string>): string {
  const paths = Object.keys(files).sort();
  if (paths.length === 0) {
    return sha256Hex("");
  }
  const canonical = paths
    .map((path) => {
      const hash = files[path];
      if (hash === undefined) {
        throw new Error(`Missing hash for path ${path}`);
      }
      return `${path}\0${hash}`;
    })
    .join("\n");
  return sha256Hex(canonical);
}

const SKILL_NAME_RE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;

export function isSkillName(value: string): boolean {
  return SKILL_NAME_RE.test(value);
}

export function isSafeFilePath(path: string): boolean {
  if (path.length === 0 || path.length > 512) {
    return false;
  }
  if (path.startsWith("/") || path.startsWith("\\")) {
    return false;
  }
  const parts = path.split("/");
  return parts.every((part) => part !== "" && part !== "." && part !== "..");
}
