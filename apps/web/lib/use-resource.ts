"use client";

import { useCallback, useEffect, useState } from "react";
import { useSession } from "@/components/providers";
import { describeApiError } from "./api";

export type Resource<T> = {
  data: T | null;
  error: string | null;
  /** True only while the very first load is in flight. */
  loading: boolean;
  /** True while any load is in flight, including manual refreshes. */
  refreshing: boolean;
  /** No credentials available yet — the caller should prompt rather than error. */
  unauthenticated: boolean;
  refresh: () => void;
};

/**
 * Loads a token-scoped API resource and re-loads it when the session changes.
 *
 * `fetcher` must be stable across renders, since it is part of the reload
 * dependency list — pass a module-level function, or wrap a closure that
 * captures props in `useCallback`.
 */
export function useResource<T>(fetcher: (token: string) => Promise<T>): Resource<T> {
  const session = useSession();
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [settled, setSettled] = useState(false);
  const [unauthenticated, setUnauthenticated] = useState(false);

  const load = useCallback(async () => {
    try {
      const token = await session.getAccessToken();
      setError(null);
      if (!token) {
        setData(null);
        setUnauthenticated(true);
        return;
      }
      setUnauthenticated(false);
      setData(await fetcher(token));
    } catch (caught) {
      setData(null);
      setError(describeApiError(caught));
    } finally {
      setRefreshing(false);
      setSettled(true);
    }
  }, [fetcher, session]);

  useEffect(() => {
    if (!session.isReady) return;
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void load();
  }, [load, session.isReady, session.isSignedIn, session.localToken]);

  const refresh = useCallback(() => {
    setRefreshing(true);
    void load();
  }, [load]);

  return {
    data,
    error,
    loading: !settled,
    refreshing,
    unauthenticated,
    refresh,
  };
}
