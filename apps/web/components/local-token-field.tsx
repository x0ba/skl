"use client";

import { useSession } from "@/components/providers";
import { Field, Input } from "@/components/ui/field";

/**
 * Development-only credential entry. In production Clerk supplies the bearer
 * token, so this collapses to a one-line statement of which auth is in play.
 */
export function LocalTokenField() {
  const session = useSession();

  if (session.isSignedIn) {
    return (
      <p className="font-mono text-[12px] text-faint">
        Authorizing with the Clerk session JWT.
      </p>
    );
  }

  return (
    <div className="space-y-3 border border-border p-4">
      <Field
        label="Bearer token"
        htmlFor="local-token"
        hint={
          session.clerkEnabled
            ? "Not signed in. Paste a Clerk session JWT, or a dev:<user_id> token if the API is running without CLERK_SECRET_KEY."
            : "Clerk is not configured, so the API accepts dev:<user_id> tokens."
        }
      >
        <Input
          id="local-token"
          value={session.localToken}
          onChange={(event) => session.setLocalToken(event.target.value)}
          autoComplete="off"
          spellCheck={false}
          placeholder="dev:user_123"
        />
      </Field>
    </div>
  );
}
