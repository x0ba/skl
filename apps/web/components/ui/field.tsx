import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

/** A bottom-ruled text input. No box, no radius — just a baseline to type on. */
export function Input({ className, ...props }: ComponentProps<"input">) {
  return (
    <input
      className={cn(
        "w-full border-0 border-b border-input bg-transparent py-2 font-mono text-[14px] text-foreground",
        "placeholder:text-faint",
        "focus:border-primary focus:outline-none",
        "disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

/** Label + input + optional hint or error, stacked. */
export function Field({
  label,
  htmlFor,
  hint,
  error,
  children,
  className,
}: {
  label: string;
  htmlFor?: string;
  hint?: ReactNode;
  error?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("space-y-1.5", className)}>
      <label
        htmlFor={htmlFor}
        className="block font-mono text-[11px] font-medium tracking-label text-faint"
      >
        {label}
      </label>
      {children}
      {error ? (
        <p className="font-mono text-[12px] text-destructive">{error}</p>
      ) : hint ? (
        <p className="text-[12px] leading-relaxed text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  );
}

/** A read-only key/value pair, used on detail surfaces. */
export function Meta({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="font-mono text-[11px] font-medium tracking-label text-faint">
        {label}
      </dt>
      <dd className="mt-1.5 font-mono text-[13px] text-foreground">{children}</dd>
    </div>
  );
}
