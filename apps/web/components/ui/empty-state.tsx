import type { ReactNode } from "react";

/**
 * Shown when a lane has nothing in it. Always explains the next action, since
 * an empty skl account is usually one CLI command away from being full.
 */
export function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="border-t border-border py-14">
      <p className="font-sans text-[17px] font-semibold text-foreground">{title}</p>
      {children ? (
        <p className="mt-2 max-w-prose text-[14px] leading-relaxed text-muted-foreground">
          {children}
        </p>
      ) : null}
      {action ? <div className="mt-5">{action}</div> : null}
    </div>
  );
}
