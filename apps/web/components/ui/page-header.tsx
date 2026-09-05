import type { ReactNode } from "react";
import { Label } from "./text";

/**
 * The standard top of every app surface: an eyebrow label, the page title, and
 * an optional trailing action aligned to the title's baseline.
 *
 * Deliberately carries no bottom rule. Everything that follows a page header —
 * lanes, metadata lists, empty states — already opens with a top rule, and two
 * hairlines a gap apart read as a mistake.
 */
export function PageHeader({
  eyebrow,
  title,
  description,
  action,
}: {
  eyebrow?: string;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <header className="mb-10">
      {eyebrow ? <Label className="mb-3">{eyebrow}</Label> : null}
      <div className="flex flex-wrap items-baseline justify-between gap-4">
        <h1 className="font-sans text-[31px] font-bold tracking-[-0.035em] text-foreground">
          {title}
        </h1>
        {action}
      </div>
      {description ? (
        <p className="mt-3 max-w-prose text-[15px] leading-relaxed text-muted-foreground">
          {description}
        </p>
      ) : null}
    </header>
  );
}
