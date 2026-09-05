import { cn } from "@/lib/utils";

export type TranscriptLine =
  | { kind: "command"; text: string }
  | { kind: "output"; text: string }
  | { kind: "note"; text: string };

/**
 * A static terminal transcript. Not interactive and not animated — it is
 * documentation of what the CLI prints, so it is rendered as plain text with
 * the prompt, output, and commentary held apart by weight and color alone.
 */
export function Transcript({
  lines,
  caption,
  className,
}: {
  lines: TranscriptLine[];
  caption?: string;
  className?: string;
}) {
  return (
    <figure className={cn("border border-border bg-secondary", className)}>
      <pre className="overflow-x-auto px-4 py-3.5 font-mono text-[13px] leading-[1.7]">
        <code>
          {lines.map((line, i) => (
            <span key={i} className="block whitespace-pre">
              {/* A blank line is a real separator here, so keep its height. */}
              {line.text === "" ? (
                "\u00A0"
              ) : line.kind === "command" ? (
                <>
                  <span className="text-faint select-none">{"$ "}</span>
                  <span className="text-foreground">{line.text}</span>
                </>
              ) : line.kind === "note" ? (
                <span className="text-faint">{line.text}</span>
              ) : (
                <span className="text-muted-foreground">{line.text}</span>
              )}
            </span>
          ))}
        </code>
      </pre>
      {caption ? (
        <figcaption className="border-t border-border px-4 py-2 font-mono text-[11px] tracking-label text-faint">
          {caption}
        </figcaption>
      ) : null}
    </figure>
  );
}
