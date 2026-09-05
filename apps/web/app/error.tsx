"use client";

import { ActionButton, ActionLink } from "@/components/ui/action-link";
import { Label } from "@/components/ui/text";

export default function ErrorBoundary({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <main className="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-6 py-20">
      <Label className="mb-4">Error</Label>
      <h1 className="font-sans text-[27px] font-bold tracking-[-0.03em] text-foreground">
        Something broke
      </h1>
      <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
        This page failed to render. Retrying is usually enough; if it is not,
        the digest below identifies the failure in the server logs.
      </p>

      <div className="mt-6 border border-border bg-secondary px-4 py-3 font-mono text-[12px]">
        <p className="break-words text-foreground">{error.message}</p>
        {error.digest ? (
          <p className="mt-2 text-faint">digest: {error.digest}</p>
        ) : null}
      </div>

      <div className="mt-8 flex items-center gap-6">
        <ActionButton onClick={reset}>Try again</ActionButton>
        <ActionLink
          href="/skills"
          className="text-muted-foreground hover:text-foreground"
        >
          Back to skills
        </ActionLink>
      </div>
    </main>
  );
}
