import { cn } from "@/lib/utils";

export type Status = "synced" | "pending" | "conflict" | "revoked";

/**
 * Revoked is drawn hollow rather than as a pale fill: "switched off" reads more
 * clearly as an outline, and a #e4e4e4 square is invisible on white.
 */
const TONE: Record<Status, string> = {
  synced: "bg-foreground",
  pending: "bg-faint",
  conflict: "bg-destructive",
  revoked: "border border-faint",
};

const LABEL: Record<Status, string> = {
  synced: "Synced",
  pending: "Pending",
  conflict: "Conflict",
  revoked: "Revoked",
};

/**
 * A 5px square (not a circle — nothing in this system has a radius) plus a
 * text label, so status never depends on color alone.
 */
export function StatusDot({
  status,
  label,
  className,
}: {
  status: Status;
  label?: string;
  className?: string;
}) {
  return (
    <span
      className={cn("inline-flex items-center gap-2 font-mono text-[13px]", className)}
    >
      <span
        aria-hidden
        className={cn("size-[5px] shrink-0", TONE[status])}
      />
      <span className={status === "conflict" ? "text-destructive" : undefined}>
        {label ?? LABEL[status]}
      </span>
    </span>
  );
}
