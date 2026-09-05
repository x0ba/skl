import Link from "next/link";
import { ActionLink } from "@/components/ui/action-link";
import { Label } from "@/components/ui/text";

export default function NotFound() {
  return (
    <main className="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-6 py-20">
      <Label className="mb-4">404</Label>
      <h1 className="font-sans text-[27px] font-bold tracking-[-0.03em] text-foreground">
        No such page
      </h1>
      <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
        The link may be stale, or the skill it pointed at was renamed.
      </p>
      <div className="mt-8 flex items-center gap-6">
        <ActionLink href="/skills">Go to skills</ActionLink>
        <Link
          href="/"
          className="font-mono text-[13px] text-muted-foreground underline decoration-from-font underline-offset-2 hover:text-foreground"
        >
          Home
        </Link>
      </div>
    </main>
  );
}
