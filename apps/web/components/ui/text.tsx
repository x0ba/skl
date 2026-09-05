import { cn } from "@/lib/utils";
import type { ComponentProps } from "react";

/**
 * Small mono all-caps label. Used for column headers, metadata keys, and
 * section eyebrows.
 */
export function Label({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      className={cn(
        "font-mono text-[11px] font-medium tracking-label text-faint",
        className,
      )}
      {...props}
    />
  );
}

/** Section heading, used inside a page. Page titles come from PageHeader. */
export function Heading({ className, ...props }: ComponentProps<"h2">) {
  return (
    <h2
      className={cn(
        "font-sans text-[19px] font-semibold tracking-[-0.025em] text-foreground",
        className,
      )}
      {...props}
    />
  );
}

/** Monospaced data: hashes, paths, counts, timestamps, commands. */
export function Mono({ className, ...props }: ComponentProps<"span">) {
  return <span className={cn("font-mono text-[13px]", className)} {...props} />;
}
