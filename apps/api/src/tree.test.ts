import { describe, expect, it } from "vitest";
import { computeTreeHash, filesToList, isSafeFilePath, isSkillName } from "./lib/tree";
import { sha256Hex } from "./lib/hash";

describe("computeTreeHash", () => {
  it("hashes the empty tree as sha256 of empty string", () => {
    expect(computeTreeHash({})).toBe(sha256Hex(""));
  });

  it("is order-independent", () => {
    const a = computeTreeHash({ "b.md": "bb", "a.md": "aa" });
    const b = computeTreeHash({ "a.md": "aa", "b.md": "bb" });
    expect(a).toBe(b);
    expect(a).toBe(sha256Hex("a.md\0aa\nb.md\0bb"));
  });
});

describe("filesToList", () => {
  it("sorts paths", () => {
    expect(filesToList({ z: "1", a: "2" })).toEqual([
      { path: "a", hash: "2" },
      { path: "z", hash: "1" },
    ]);
  });
});

describe("name and path guards", () => {
  it("accepts skill slugs", () => {
    expect(isSkillName("my-skill")).toBe(true);
    expect(isSkillName("Org.skill_1")).toBe(true);
    expect(isSkillName("../x")).toBe(false);
    expect(isSkillName("")).toBe(false);
  });

  it("rejects unsafe file paths", () => {
    expect(isSafeFilePath("SKILL.md")).toBe(true);
    expect(isSafeFilePath("scripts/run.sh")).toBe(true);
    expect(isSafeFilePath("../secret")).toBe(false);
    expect(isSafeFilePath("/etc/passwd")).toBe(false);
  });
});
