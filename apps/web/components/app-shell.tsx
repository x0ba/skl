"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { ClerkAuthControls, useSession } from "./providers";

const NAV = [
  { href: "/skills", label: "Skills" },
  { href: "/devices", label: "Devices" },
  { href: "/activity", label: "Activity" },
  { href: "/settings", label: "Settings" },
] as const;

function NavLink({ href, label }: { href: string; label: string }) {
  const pathname = usePathname();
  const active = pathname === href || pathname.startsWith(`${href}/`);

  return (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      className={cn(
        "relative block py-1.5 font-mono text-[13px] transition-colors md:pl-4",
        // The vertical tick only reads as "current" against a stacked list. In
        // the mobile rail the nav runs horizontally, where a left tick looks
        // like a separator between items, so active is underlined there.
        active
          ? "text-foreground underline decoration-from-font underline-offset-4 md:no-underline md:before:absolute md:before:left-0 md:before:top-1/2 md:before:h-3 md:before:w-px md:before:-translate-y-1/2 md:before:bg-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
    </Link>
  );
}

/**
 * Two-column app chrome: a fixed hairline-ruled sidebar and a content column
 * capped at a comfortable reading measure. The sidebar collapses to a
 * horizontal rail on narrow viewports.
 */
export function AppShell({ children }: { children: ReactNode }) {
  const { clerkEnabled } = useSession();

  return (
    <div className="flex min-h-dvh flex-col md:flex-row">
      <aside className="flex shrink-0 flex-col gap-8 border-b border-border px-6 py-5 md:sticky md:top-0 md:h-dvh md:w-52 md:border-b-0 md:border-r md:py-8">
        <Link
          href="/"
          className="font-mono text-[15px] font-medium tracking-tight text-foreground"
        >
          skl
        </Link>

        <nav className="flex flex-row gap-x-5 md:flex-1 md:flex-col md:gap-x-0 md:gap-y-0.5">
          {NAV.map((item) => (
            <NavLink key={item.href} {...item} />
          ))}
        </nav>

        <div className="md:pl-4">
          {clerkEnabled ? <ClerkAuthControls /> : null}
        </div>
      </aside>

      <main className="min-w-0 flex-1 px-6 py-10 md:px-12 md:py-14">
        <div className="mx-auto w-full max-w-3xl">{children}</div>
      </main>
    </div>
  );
}
