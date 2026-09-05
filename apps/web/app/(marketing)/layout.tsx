import Link from "next/link";

const REPO = "https://github.com/x0ba/skl";

export default function MarketingLayout({ children }: LayoutProps<"/">) {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="border-b border-border">
        <div className="mx-auto flex h-14 w-full max-w-content items-center justify-between px-6">
          <Link
            href="/"
            className="font-mono text-[15px] font-medium tracking-tight text-foreground"
          >
            skl
          </Link>
          <nav className="flex items-center gap-6 font-mono text-[13px]">
            <a
              href={REPO}
              className="text-muted-foreground underline decoration-from-font underline-offset-2 hover:text-foreground"
            >
              Source
            </a>
            <Link
              href="/skills"
              className="text-primary underline decoration-from-font underline-offset-2 hover:decoration-2"
            >
              Open dashboard
            </Link>
          </nav>
        </div>
      </header>

      <div className="flex-1">{children}</div>

      <footer className="border-t border-border">
        <div className="mx-auto flex w-full max-w-content flex-wrap items-center justify-between gap-4 px-6 py-6 font-mono text-[12px] text-faint">
          <span>skl — personal agent skill sync</span>
          <div className="flex items-center gap-6">
            <a href={REPO} className="hover:text-foreground">
              GitHub
            </a>
            <Link href="/design" className="hover:text-foreground">
              Design system
            </Link>
          </div>
        </div>
      </footer>
    </div>
  );
}
