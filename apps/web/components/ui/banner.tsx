import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

type Tone = "info" | "danger";

/**
 * A full-width notice. Carries a left rule rather than a filled panel so it
 * sits inside the page's rhythm instead of interrupting it.
 */
export function Banner({
  tone = "info",
  title,
  children,
  action,
  className,
  ...props
}: ComponentProps<"div"> & {
  tone?: Tone;
  title: string;
  action?: ReactNode;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className={cn(
        "flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2 border-l-2 bg-secondary py-3 pl-4 pr-4",
        tone === "danger" ? "border-l-destructive" : "border-l-primary",
        className,
      )}
      {...props}
    >
      <div className="min-w-0">
        <p
          className={cn(
            "font-mono text-[13px] font-medium",
            tone === "danger" ? "text-destructive" : "text-foreground",
          )}
        >
          {title}
        </p>
        {children ? (
          <p className="mt-1 text-[13px] leading-relaxed text-muted-foreground">
            {children}
          </p>
        ) : null}
      </div>
      {action}
    </div>
  );
}
