import { StrictMode, type ReactNode } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { SWRConfig } from "swr";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useResource } from "./use-resource";

const state = vi.hoisted(() => ({
  session: { isReady: true, cacheKey: "alice", getAccessToken: async () => "alice-token" as string | null },
}));
vi.mock("@/components/providers", () => ({ useSession: () => state.session }));

function wrapper() {
  const cache = new Map();
  return function Wrapper({ children }: { children: ReactNode }) {
    return <StrictMode><SWRConfig value={{ provider: () => cache }}>{children}</SWRConfig></StrictMode>;
  };
}

afterEach(cleanup);
beforeEach(() => {
  state.session = { isReady: true, cacheKey: "alice", getAccessToken: async () => "alice-token" };
});

describe("dashboard resources", () => {
  it("deduplicates concurrent consumers and shares manual refreshes", async () => {
    const fetcher = vi.fn().mockResolvedValueOnce("first").mockResolvedValue("updated");
    const { result } = renderHook(() => [useResource(fetcher, "skills"), useResource(fetcher, "skills")], { wrapper: wrapper() });
    await waitFor(() => expect(result.current[0].data).toBe("first"));
    expect(result.current[1].data).toBe("first");
    expect(fetcher).toHaveBeenCalledTimes(1);
    act(() => result.current[0].refresh());
    await waitFor(() => expect(result.current[1].data).toBe("updated"));
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("reuses cached data when returning to a route", async () => {
    const fetcher = vi.fn(async () => "cached");
    const { result, rerender } = renderHook(({ key }) => useResource(fetcher, key), { initialProps: { key: "skills" }, wrapper: wrapper() });
    await waitFor(() => expect(result.current.data).toBe("cached"));
    rerender({ key: "devices" });
    await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(2));
    rerender({ key: "skills" });
    expect(result.current.data).toBe("cached");
    expect(result.current.loading).toBe(false);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("never displays an old account's late response after switching accounts", async () => {
    let finishAlice!: (value: string) => void;
    const alice = new Promise<string>(resolve => { finishAlice = resolve; });
    const fetcher = vi.fn((token: string) => token === "alice-token" ? alice : Promise.resolve("bob-data"));
    const { result, rerender } = renderHook(() => useResource(fetcher, "skills"), { wrapper: wrapper() });
    await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
    state.session = { isReady: true, cacheKey: "bob", getAccessToken: async () => "bob-token" };
    rerender();
    expect(result.current.data).toBeNull();
    await waitFor(() => expect(result.current.data).toBe("bob-data"));
    await act(async () => { finishAlice("alice-data"); await alice; });
    expect(result.current.data).toBe("bob-data");
  });

  it("waits for authentication and avoids requests without credentials", async () => {
    state.session = { isReady: false, cacheKey: "none", getAccessToken: async () => null };
    const fetcher = vi.fn();
    const { result, rerender } = renderHook(() => useResource(fetcher, "skills"), { wrapper: wrapper() });
    expect(result.current.loading).toBe(true);
    expect(fetcher).not.toHaveBeenCalled();
    state.session = { ...state.session, isReady: true };
    rerender();
    await waitFor(() => expect(result.current.unauthenticated).toBe(true));
    expect(fetcher).not.toHaveBeenCalled();
  });
});
