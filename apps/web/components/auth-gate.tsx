"use client";

import { LocalTokenField } from "@/components/local-token-field";

/**
 * Rendered by app surfaces when no bearer token is available. Distinct from an
 * error: nothing has failed yet, we just have no credentials to call with.
 */
export function AuthGate() {
  return (
    <div className="border-t border-border py-14">
      <p className="font-sans text-[17px] font-semibold text-foreground">
        No credentials
      </p>
      <p className="mt-2 max-w-prose text-[14px] leading-relaxed text-muted-foreground">
        Sign in, or set a bearer token to talk to the API directly.
      </p>
      <div className="mt-6 max-w-md">
        <LocalTokenField />
      </div>
    </div>
  );
}
