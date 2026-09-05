import Link from "next/link";

/**
 * Chromeless, centered column for sign-in, sign-up, and device approval —
 * surfaces the user reaches mid-task, usually from the CLI.
 */
export default function AuthLayout({ children }: LayoutProps<"/">) {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="px-6 py-5">
        <Link
          href="/"
          className="font-mono text-[15px] font-medium tracking-tight text-foreground"
        >
          skl
        </Link>
      </header>
      <main className="flex flex-1 items-start justify-center px-6 pb-24 pt-10 sm:pt-20">
        <div className="w-full max-w-sm">{children}</div>
      </main>
    </div>
  );
}
