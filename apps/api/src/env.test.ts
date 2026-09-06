import { describe, expect, it } from "vitest";
import { originVariants } from "./env";

describe("originVariants", () => {
  it("adds www for an apex https origin", () => {
    expect(originVariants("https://tryskl.fyi")).toEqual([
      "https://tryskl.fyi",
      "https://www.tryskl.fyi",
    ]);
  });

  it("adds apex for a www origin", () => {
    expect(originVariants("https://www.tryskl.fyi")).toEqual([
      "https://www.tryskl.fyi",
      "https://tryskl.fyi",
    ]);
  });

  it("does not invent www for localhost", () => {
    expect(originVariants("http://localhost:3000")).toEqual([
      "http://localhost:3000",
    ]);
  });
});
