"use client";

import { useSyncExternalStore } from "react";
import { CopyCommand } from "@/components/ui/copy-command";

function installCommand(origin: string): string {
  return `curl -fsSL ${origin}/install.sh | bash`;
}

/**
 * Advertises `curl …/install.sh` against the origin the visitor is on,
 * so localhost, preview, and production all copy a working URL.
 */
const subscribe = () => () => {};
const getOrigin = () => window.location.origin;
const getServerOrigin = () => process.env.NEXT_PUBLIC_WEB_ORIGIN || "https://www.tryskl.fyi";

export function InstallCommand() {
  const origin = useSyncExternalStore(subscribe, getOrigin, getServerOrigin);
  return <CopyCommand command={installCommand(origin)} />;
}
