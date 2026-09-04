"use client";

import { useSession } from "@/components/providers";

export function LocalTokenField() {
  const session = useSession();

  if (session.isSignedIn) {
    return (
      <p className="text-xs text-muted-foreground">
        Using Clerk session JWT for <code>Authorization: Bearer</code>.
      </p>
    );
  }

  return (
    <div className="space-y-2 border border-border p-3">
      <p className="text-xs text-muted-foreground">
        {session.clerkEnabled
          ? "Not signed in. Paste a Clerk session JWT, or use local API auth:"
          : "Clerk is not configured. Local API accepts"}{" "}
        <code className="text-foreground">Authorization: Bearer dev:&lt;user_id&gt;</code>
        {" "}when <code className="text-foreground">CLERK_SECRET_KEY</code> is unset.
      </p>
      <label className="block space-y-1">
        <span className="text-xs text-muted-foreground">Bearer token</span>
        <input
          className="h-8 w-full border border-input bg-background px-2 text-xs outline-none focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/50"
          value={session.localToken}
          onChange={(event) => session.setLocalToken(event.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
      </label>
    </div>
  );
}
