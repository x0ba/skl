"use client";

import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

/**
 * A single shell command with a copy affordance. The button label doubles as
 * the confirmation, so there is no toast and no icon.
 */
export function CopyCommand({
  command,
  className,
}: {
  command: string;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
    } catch {
      // Clipboard is unavailable (insecure origin or denied permission). The
      // command stays selectable, so fail quietly rather than alarming anyone.
      return;
    }
    setCopied(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 1600);
  }

  return (
    <div
      className={cn(
        "flex w-max items-center gap-3 border border-border bg-secondary py-2.5 pl-4 pr-3",
        className,
      )}
    >
      <code className="whitespace-nowrap font-mono text-[13px] leading-relaxed text-foreground">
        <span className="text-faint select-none">{"$ "}</span>
        {command}
      </code>
      <button
        type="button"
        onClick={copy}
        className="relative shrink-0 text-right font-mono text-[11px] tracking-label text-primary underline decoration-from-font underline-offset-2 hover:decoration-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      >
        <span className="invisible" aria-hidden="true">
          COPIED
        </span>
        <span className="absolute inset-0">
          {copied ? "COPIED" : "COPY"}
        </span>
      </button>
    </div>
  );
}
