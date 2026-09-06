"use client";

import useSWR from "swr";
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

/** Session-scoped stale-while-revalidate data shared across dashboard routes. */
export function useResource<T>(
  fetcher: (token: string) => Promise<T>,
  key: string,
): Resource<T> {
  const session = useSession();
  const { data, error, isLoading, isValidating, mutate } = useSWR(
    session.isReady ? [session.cacheKey, key] : null,
    async () => {
      const token = await session.getAccessToken();
      return token
        ? { value: await fetcher(token), unauthenticated: false }
        : { value: null, unauthenticated: true };
    },
    { shouldRetryOnError: false, dedupingInterval: 5000 },
  );
  return {
    data: data?.value ?? null,
    error: error ? describeApiError(error) : null,
    loading: !session.isReady || isLoading,
    refreshing: isValidating,
    unauthenticated: data?.unauthenticated ?? false,
    refresh: () => { void mutate(); },
  };
}
