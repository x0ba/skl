"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { ClerkAuthControls, useSession } from "@/components/providers";
import { API_BASE } from "@/lib/config";
import { cn } from "@/lib/utils";

const LINKS = [
  { href: "/", label: "Dashboard" },
  { href: "/device", label: "Approve device" },
] as const;

export function AppHeader() {
  const pathname = usePathname();
  const session = useSession();

  return (
    <header className="border-b border-border">
      <div className="mx-auto flex w-full max-w-3xl flex-wrap items-center justify-between gap-3 px-4 py-3">
        <div className="flex items-center gap-6">
          <Link href="/" className="text-sm font-medium tracking-tight">
            skl
          </Link>
          <nav className="flex items-center gap-3 text-xs">
            {LINKS.map((link) => {
              const active =
                link.href === "/"
                  ? pathname === "/"
                  : pathname.startsWith(link.href);
              return (
                <Link
                  key={link.href}
                  href={link.href}
                  className={cn(
                    "hover:text-foreground",
                    active ? "text-foreground" : "text-muted-foreground",
                  )}
                >
                  {link.label}
                </Link>
              );
            })}
          </nav>
        </div>
        <div className="flex items-center gap-3">
          {session.clerkEnabled ? <ClerkAuthControls /> : null}
          <span className="text-[10px] text-muted-foreground">{API_BASE}</span>
        </div>
      </div>
    </header>
  );
}
