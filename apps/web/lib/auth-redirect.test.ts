import { describe, expect, it } from "vitest";
import {
  clerkSignInRedirects,
  clerkSignUpRedirects,
  deviceApproveHref,
  formatDeviceUserCode,
  normalizeDeviceUserCode,
  redirectFromSearchParams,
  safeRedirectPath,
  withRedirectUrl,
} from "./auth-redirect";

describe("device user codes", () => {
  it("normalizes punctuation and case", () => {
    expect(normalizeDeviceUserCode("ab-cd 23-45")).toBe("ABCD2345");
    expect(formatDeviceUserCode("abcd2345")).toBe("ABCD-2345");
  });

  it("builds the CLI return path with a display code", () => {
    expect(deviceApproveHref("")).toBe("/device");
    expect(deviceApproveHref("abcd2345")).toBe("/device?user_code=ABCD-2345");
  });
});

describe("safeRedirectPath", () => {
  it("keeps same-origin relative paths and query strings", () => {
    expect(safeRedirectPath("/device?user_code=ABCD-2345")).toBe(
      "/device?user_code=ABCD-2345",
    );
  });

  it("collapses absolute URLs to path + search", () => {
    expect(
      safeRedirectPath("https://tryskl.fyi/device?user_code=ABCD-2345"),
    ).toBe("/device?user_code=ABCD-2345");
  });

  it("rejects protocol-relative and non-path values", () => {
    expect(safeRedirectPath("//evil.example/phish")).toBeUndefined();
    expect(safeRedirectPath("javascript:alert(1)")).toBeUndefined();
    expect(safeRedirectPath("")).toBeUndefined();
  });

  it("reads redirect_url from Clerk search params", () => {
    expect(
      redirectFromSearchParams({
        redirect_url: "/device?user_code=ABCD-2345",
      }),
    ).toBe("/device?user_code=ABCD-2345");
  });
});

describe("clerk redirect props", () => {
  it("threads the return path through sign-in and sign-up", () => {
    const returnTo = deviceApproveHref("ABCD2345");
    expect(withRedirectUrl("/sign-in", returnTo)).toBe(
      "/sign-in?redirect_url=%2Fdevice%3Fuser_code%3DABCD-2345",
    );
    expect(clerkSignInRedirects(returnTo).signUpUrl).toBe(
      withRedirectUrl("/sign-up", returnTo),
    );
    expect(clerkSignUpRedirects(returnTo).signInUrl).toBe(
      withRedirectUrl("/sign-in", returnTo),
    );
    expect(clerkSignInRedirects(undefined)).toEqual({});
  });
});
