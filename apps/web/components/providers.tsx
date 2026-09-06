"use client";

import {
  ClerkProvider,
  SignInButton,
  SignUpButton,
  UserButton,
  useAuth,
} from "@clerk/nextjs";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { SWRConfig } from "swr";
import { DEFAULT_DEV_TOKEN } from "@/lib/config";
import {
  readLocalToken,
  subscribeLocalToken,
  writeLocalToken,
} from "@/lib/local-token";

export type SessionApi = {
  cacheKey: string;
  clerkEnabled: boolean;
  isReady: boolean;
  isSignedIn: boolean;
  getAccessToken: () => Promise<string | null>;
  localToken: string;
  setLocalToken: (token: string) => void;
};

const SessionContext = createContext<SessionApi | null>(null);

export function useSession(): SessionApi {
  const ctx = useContext(SessionContext);
  if (!ctx) {
    throw new Error("useSession must be used within AppProviders");
  }
  return ctx;
}

function usePersistedLocalToken(defaultDevToken: string): [string, (token: string) => void] {
  const stored = useSyncExternalStore(
    subscribeLocalToken,
    readLocalToken,
    () => "",
  );
  const setLocalToken = useCallback((token: string) => {
    writeLocalToken(token);
  }, []);
  return [stored || defaultDevToken, setLocalToken];
}

function ClerkSessionBridge({
  children,
  defaultDevToken,
}: {
  children: ReactNode;
  defaultDevToken: string;
}) {
  const { isLoaded, isSignedIn, getToken, sessionId, orgId } = useAuth();
  const [localToken, setLocalToken] = usePersistedLocalToken(defaultDevToken);

  const getAccessToken = useCallback(async () => {
    if (isSignedIn) {
      return getToken();
    }
    const token = localToken.trim();
    return token.length > 0 ? token : null;
  }, [getToken, isSignedIn, localToken]);

  const value = useMemo<SessionApi>(
    () => ({
      cacheKey: isSignedIn ? `clerk:${sessionId}:${orgId ?? ""}` : `local:${localToken}`,
      clerkEnabled: true,
      isReady: isLoaded,
      isSignedIn: Boolean(isSignedIn),
      getAccessToken,
      localToken,
      setLocalToken,
    }),
    [getAccessToken, isLoaded, isSignedIn, sessionId, orgId, localToken, setLocalToken],
  );

  return (
    <SessionContext.Provider value={value}>
      <SessionResources>{children}</SessionResources>
    </SessionContext.Provider>
  );
}

function LocalSessionProvider({
  children,
  defaultDevToken,
}: {
  children: ReactNode;
  defaultDevToken: string;
}) {
  const [localToken, setLocalToken] = usePersistedLocalToken(defaultDevToken);

  const getAccessToken = useCallback(async () => {
    const token = localToken.trim();
    return token.length > 0 ? token : null;
  }, [localToken]);

  const value = useMemo<SessionApi>(
    () => ({
      cacheKey: `local:${localToken}`,
      clerkEnabled: false,
      isReady: true,
      isSignedIn: false,
      getAccessToken,
      localToken,
      setLocalToken,
    }),
    [getAccessToken, localToken, setLocalToken],
  );

  return (
    <SessionContext.Provider value={value}>
      <SessionResources>{children}</SessionResources>
    </SessionContext.Provider>
  );
}

// Keep the cache local to this mounted app. useResource partitions every entry
// by session/organization/token, so late responses cannot cross accounts.
// Preserve the subtree while editing a local token so inputs retain focus.
function SessionResources({ children }: { children: ReactNode }) {
  const value = useMemo(() => ({ provider: () => new Map() }), []);
  return <SWRConfig value={value}>{children}</SWRConfig>;
}

export function AppProviders({
  children,
  clerkEnabled,
}: {
  children: ReactNode;
  clerkEnabled: boolean;
}) {
  if (clerkEnabled) {
    return (
      <ClerkProvider>
        <ClerkSessionBridge defaultDevToken={DEFAULT_DEV_TOKEN}>
          {children}
        </ClerkSessionBridge>
      </ClerkProvider>
    );
  }

  return (
    <LocalSessionProvider defaultDevToken={DEFAULT_DEV_TOKEN}>
      {children}
    </LocalSessionProvider>
  );
}

export function ClerkAuthControls() {
  const { isLoaded, isSignedIn } = useAuth();

  if (!isLoaded) {
    return null;
  }

  if (isSignedIn) {
    return <UserButton />;
  }

  return (
    <div className="flex items-center gap-4">
      <SignInButton>
        <button
          type="button"
          className="font-mono text-[13px] text-muted-foreground underline decoration-from-font underline-offset-2 hover:text-foreground"
        >
          Sign in
        </button>
      </SignInButton>
      <SignUpButton>
        <button
          type="button"
          className="font-mono text-[13px] text-primary underline decoration-from-font underline-offset-2 hover:decoration-2"
        >
          Sign up
        </button>
      </SignUpButton>
    </div>
  );
}
