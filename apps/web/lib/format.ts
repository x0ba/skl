/** Presentation helpers. Kept UI-agnostic so surfaces stay declarative. */

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * Coarse relative time ("4h ago"). Deliberately low-resolution: the exact
 * timestamp is always available in a `title` next to it.
 */
export function relativeTime(value: string | null | undefined): string {
  if (!value) return "—";
  const then = Date.parse(value);
  if (Number.isNaN(then)) return "—";

  const delta = Date.now() - then;
  if (delta < MINUTE) return "just now";
  if (delta < HOUR) return `${Math.floor(delta / MINUTE)}m ago`;
  if (delta < DAY) return `${Math.floor(delta / HOUR)}h ago`;
  if (delta < 30 * DAY) return `${Math.floor(delta / DAY)}d ago`;
  return new Date(then).toISOString().slice(0, 10);
}

/** Full timestamp for `title` attributes, without subsecond noise. */
export function exactTime(value: string | null | undefined): string {
  if (!value) return "—";
  const then = Date.parse(value);
  if (Number.isNaN(then)) return "—";
  return new Date(then).toISOString().replace("T", " ").replace(/\.\d+Z$/, "Z");
}

/** First 12 characters of a content hash — enough to eyeball, short enough to fit. */
export function shortHash(hash: string): string {
  return hash.slice(0, 12);
}

export function pluralize(count: number, singular: string, plural?: string): string {
  return `${count} ${count === 1 ? singular : (plural ?? `${singular}s`)}`;
}

/** Splits a skill-relative path into its directory and file name. */
export function splitPath(path: string): { dir: string; file: string } {
  const at = path.lastIndexOf("/");
  if (at === -1) return { dir: "", file: path };
  return { dir: path.slice(0, at + 1), file: path.slice(at + 1) };
}
