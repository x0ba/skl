import Link from "next/link";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/**
 * The accent-blue underlined link. This is the load-bearing affordance of the
 * design system: blue means actionable, so keep it to roughly two per screen.
 */
export function ActionLink({ className, ...props }: ComponentProps<typeof Link>) {
  return (
    <Link
      className={cn(
        "font-mono text-[13px] text-primary underline decoration-from-font underline-offset-2",
        "hover:decoration-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
        className,
      )}
      {...props}
    />
  );
}

/** Same treatment as ActionLink, for in-place actions that are not navigations. */
export function ActionButton({
  className,
  type = "button",
  ...props
}: ComponentProps<"button">) {
  return (
    <button
      type={type}
      className={cn(
        "font-mono text-[13px] text-primary underline decoration-from-font underline-offset-2",
        "hover:decoration-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
        "disabled:pointer-events-none disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

/**
 * Destructive counterpart. Deliberately not blue — revoke and delete should
 * not read as the happy path.
 */
export function DangerButton({
  className,
  type = "button",
  ...props
}: ComponentProps<"button">) {
  return (
    <button
      type={type}
      className={cn(
        "font-mono text-[13px] text-destructive underline decoration-from-font underline-offset-2",
        "hover:decoration-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
        "disabled:pointer-events-none disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}
