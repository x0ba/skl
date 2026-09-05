"use client";

import { useEffect, useState } from "react";
import { CopyCommand } from "@/components/ui/copy-command";

function installCommand(origin: string): string {
  return `curl -fsSL ${origin}/install.sh | bash`;
}

/**
 * Advertises `curl …/install.sh` against the origin the visitor is on,
 * so localhost, preview, and production all copy a working URL.
 */
export function InstallCommand({ initialOrigin }: { initialOrigin: string }) {
  const [origin, setOrigin] = useState(initialOrigin);

  useEffect(() => {
    setOrigin(window.location.origin);
  }, []);

  return <CopyCommand command={installCommand(origin)} />;
}
