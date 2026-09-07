"use client";

import { useState } from "react";
import { cn } from "@/lib/utils";

export type TuiSkill = {
  name: string;
  used: boolean;
  preview: string;
};

/**
 * A reconstruction of the two-pane TUI. Hover, focus, or click a skill
 * to show its preview. No motion: the highlight and body just swap.
 */
export function TuiFrame({
  lastSync,
  project,
  skills,
  initialSelected,
  keys,
  caption,
  className,
}: {
  lastSync: string;
  project: string;
  skills: readonly TuiSkill[];
  initialSelected?: string;
  keys: string;
  caption?: string;
  className?: string;
}) {
  const [selectedName, setSelectedName] = useState(
    initialSelected ?? skills[0]?.name ?? "",
  );
  const selected =
    skills.find((skill) => skill.name === selectedName) ?? skills[0];
  const n = skills.length;
  const skillWord = n === 1 ? "skill" : "skills";

  return (
    <figure className={cn("border border-border bg-secondary", className)}>
      <div className="overflow-x-auto">
        <div className="min-w-[28rem]">
          <div className="border-b border-border px-3 py-2 font-mono text-[13px] text-muted-foreground">
            library: {n} {skillWord}
            {"  ·  "}
            last sync: {lastSync}
            {"  ·  "}
            project: <span className="text-foreground">{project}</span>
          </div>
          <div className="grid h-64 grid-cols-[minmax(10.5rem,38%)_minmax(14rem,1fr)]">
            <div className="border-r border-border">
              <div className="border-b border-border px-3 py-1.5 font-mono text-[11px] tracking-label text-faint">
                skills
              </div>
              <ul>
                {skills.map((skill) => {
                  const on = skill.name === selected?.name;
                  return (
                    <li key={skill.name}>
                      <button
                        type="button"
                        aria-current={on ? "true" : undefined}
                        onPointerEnter={() => setSelectedName(skill.name)}
                        onFocus={() => setSelectedName(skill.name)}
                        onClick={() => setSelectedName(skill.name)}
                        className={cn(
                          "flex w-full cursor-default gap-2 px-3 py-0.5 text-left font-mono text-[13px] leading-relaxed",
                          "focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring",
                          on
                            ? "bg-foreground text-background"
                            : "text-muted-foreground",
                        )}
                      >
                        <span
                          className={cn(
                            "w-3 shrink-0",
                            on ? "text-background" : "text-faint",
                          )}
                        >
                          {skill.used ? "✓" : "·"}
                        </span>
                        {skill.name}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
            <div className="min-h-0">
              <div className="border-b border-border px-3 py-1.5 font-mono text-[11px] tracking-label text-faint">
                {selected?.name}
              </div>
              <pre className="overflow-hidden px-3 py-2.5 font-mono text-[13px] leading-relaxed text-muted-foreground whitespace-pre-wrap">
                {selected?.preview}
              </pre>
            </div>
          </div>
          <div className="overflow-x-auto border-t border-border px-3 py-1.5 font-mono text-[11px] text-faint whitespace-nowrap">
            {keys}
          </div>
        </div>
      </div>
      {caption ? (
        <figcaption className="border-t border-border px-4 py-2 font-mono text-[11px] tracking-label text-faint">
          {caption}
        </figcaption>
      ) : null}
    </figure>
  );
}
