"use client";

import { useSyncExternalStore } from "react";
import { CopyCommand } from "@/components/ui/copy-command";

function withoutWww(origin: string): string {
  return origin.replace(/^(https?:\/\/)www\./, "$1");
}

function installCommand(origin: string): string {
  return `curl -fsSL ${origin}/install.sh | sh`;
}

/**
 * Advertises `curl …/install.sh` against the origin the visitor is on,
 * so localhost, preview, and production all copy a working URL.
 * Apex is canonical; strip `www.` so the copied command stays apex.
 */
const subscribe = () => () => {};
const getOrigin = () => withoutWww(window.location.origin);
const getServerOrigin = () =>
  withoutWww(process.env.NEXT_PUBLIC_WEB_ORIGIN || "https://tryskl.fyi");

export function InstallCommand() {
  const origin = useSyncExternalStore(subscribe, getOrigin, getServerOrigin);
  return <CopyCommand command={installCommand(origin)} />;
}
