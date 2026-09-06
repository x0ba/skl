import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceApproveForm } from "./device-approve-form";
import { deviceApproveHref, withRedirectUrl } from "@/lib/auth-redirect";

const state = vi.hoisted(() => ({
  session: {
    clerkEnabled: true,
    isReady: true,
    isSignedIn: false,
    getAccessToken: async () => null as string | null,
    localToken: "",
    setLocalToken: () => {},
    cacheKey: "none",
  },
}));

vi.mock("@/components/providers", () => ({
  useSession: () => state.session,
}));

afterEach(cleanup);

beforeEach(() => {
  state.session = {
    clerkEnabled: true,
    isReady: true,
    isSignedIn: false,
    getAccessToken: async () => null,
    localToken: "",
    setLocalToken: () => {},
    cacheKey: "none",
  };
});

describe("DeviceApproveForm", () => {
  it("sends unsigned Clerk users through sign-in and back to the code", () => {
    render(<DeviceApproveForm initialUserCode="abcd2345" />);

    expect(screen.queryByRole("button", { name: "Approve device" })).toBeNull();
    expect(screen.getByText("ABCD-2345")).toBeTruthy();

    const returnTo = deviceApproveHref("abcd2345");
    expect(screen.getByRole("link", { name: "Sign in" }).getAttribute("href")).toBe(
      withRedirectUrl("/sign-in", returnTo),
    );
    expect(
      screen.getByRole("link", { name: "Create an account" }).getAttribute("href"),
    ).toBe(withRedirectUrl("/sign-up", returnTo));
  });

  it("does not show the approve form while Clerk is still loading", () => {
    state.session = { ...state.session, isReady: false };
    render(<DeviceApproveForm initialUserCode="abcd2345" />);

    expect(screen.queryByRole("button", { name: "Approve device" })).toBeNull();
    expect(screen.queryByRole("link", { name: "Sign in" })).toBeNull();
    expect(screen.getByText("Checking your session…")).toBeTruthy();
  });

  it("shows the approve form after Clerk sign-in", () => {
    state.session = {
      ...state.session,
      isSignedIn: true,
      getAccessToken: async () => "clerk-jwt",
    };
    render(<DeviceApproveForm initialUserCode="abcd2345" />);

    expect(screen.getByRole("button", { name: "Approve device" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Sign in" })).toBeNull();
  });

  it("keeps the local bearer-token path when Clerk is off", () => {
    state.session = {
      ...state.session,
      clerkEnabled: false,
      isSignedIn: false,
      getAccessToken: async () => "dev:local-dev",
    };
    render(<DeviceApproveForm initialUserCode="abcd2345" />);

    expect(screen.getByRole("button", { name: "Approve device" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Sign in" })).toBeNull();
  });
});
