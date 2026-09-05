import type { ComponentProps, CSSProperties } from "react";
import { cn } from "@/lib/utils";

/**
 * A "lane" is this design system's table: a mono all-caps header, hairline
 * rules between rows, and no vertical borders or zebra striping. Columns are a
 * grid template shared by the header and every row, passed as `cols`.
 */
export function Lane({
  cols,
  className,
  style,
  ...props
}: ComponentProps<"div"> & { cols: string }) {
  return (
    <div
      className={cn("border-t border-border", className)}
      style={{ ...style, "--lane-cols": cols } as CSSProperties}
      {...props}
    />
  );
}

const rowLayout = "grid grid-cols-[var(--lane-cols)] items-center gap-x-6";

export function LaneHead({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      className={cn(
        rowLayout,
        "border-b border-border py-2.5",
        "font-mono text-[11px] font-medium tracking-label text-faint",
        className,
      )}
      {...props}
    />
  );
}

export function LaneRow({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      className={cn(
        rowLayout,
        "border-b border-rule-soft py-3.5 last:border-b-border",
        className,
      )}
      {...props}
    />
  );
}

/** A row that links somewhere. Hover tints the whole band, not just the text. */
export function LaneLinkRow({ className, ...props }: ComponentProps<"div">) {
  return (
    <LaneRow
      className={cn(
        "-mx-4 px-4 transition-colors hover:bg-secondary",
        "focus-within:bg-secondary",
        className,
      )}
      {...props}
    />
  );
}

/** Right-aligns a cell. Use for counts and trailing actions. */
export function LaneEnd({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("justify-self-end", className)} {...props} />;
}
