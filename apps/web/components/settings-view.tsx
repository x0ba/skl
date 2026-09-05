"use client";

import { useState } from "react";
import { AuthGate } from "@/components/auth-gate";
import { LocalTokenField } from "@/components/local-token-field";
import { useSession } from "@/components/providers";
import { ActionLink, DangerButton } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Meta } from "@/components/ui/field";
import { Lane, LaneHead, LaneRow } from "@/components/ui/lane";
import { PageHeader } from "@/components/ui/page-header";
import { Label } from "@/components/ui/text";
import { describeApiError, listDevices, revokeDevice } from "@/lib/api";
import { API_BASE } from "@/lib/config";
import { pluralize } from "@/lib/format";
import { useResource } from "@/lib/use-resource";

/** Project dests `skl use` writes. Canonical is always on; extras are opt-in. */
const TARGETS = [
  { tool: "Universal", root: ".agents/skills" },
  { tool: "Claude Code", root: ".claude/skills" },
];

export function SettingsView() {
  const session = useSession();
  const { data, unauthenticated, refresh } = useResource(listDevices);

  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const active = (data?.devices ?? []).filter((device) => !device.revoked_at);

  async function revokeAll() {
    setError(null);
    setNotice(null);
    const token = await session.getAccessToken();
    if (!token) {
      setError("No credentials. Sign in or set a bearer token first.");
      return;
    }

    setPending(true);
    try {
      // No bulk endpoint exists, so this fans out over DELETE /v1/devices/:id.
      const results = await Promise.allSettled(
        active.map((device) => revokeDevice(token, device.id)),
      );
      const rejected = results.filter(
        (result): result is PromiseRejectedResult => result.status === "rejected",
      );
      if (rejected.length > 0) {
        setError(
          `${pluralize(rejected.length, "device")} could not be revoked: ${describeApiError(rejected[0].reason)}`,
        );
      } else {
        setNotice(`Revoked ${pluralize(active.length, "device")}.`);
      }
      refresh();
    } finally {
      setPending(false);
    }
  }

  return (
    <>
      <PageHeader
        eyebrow="Account"
        title="Settings"
        description="Where this dashboard points, where skills land on disk, and how to cut off access."
      />

      {error ? (
        <Banner tone="danger" title="Something went wrong" className="mb-8">
          {error}
        </Banner>
      ) : null}

      {notice ? (
        <Banner title={notice} className="mb-8">
          Every machine will need a fresh code from{" "}
          <code className="font-mono">skl login</code>.
        </Banner>
      ) : null}

      <div className="space-y-14">
        <section>
          <Label className="mb-4">Connection</Label>
          <dl className="grid gap-x-8 gap-y-6 border-t border-border pt-6 sm:grid-cols-2">
            <Meta label="API base">
              <span className="break-all">{API_BASE}</span>
            </Meta>
            <Meta label="Auth mode">
              {session.clerkEnabled ? "Clerk" : "Local bearer token"}
            </Meta>
          </dl>
          <div className="mt-6 max-w-md">
            <LocalTokenField />
          </div>
        </section>

        <section>
          <Label className="mb-4">Targets</Label>
          <Lane cols="7rem minmax(0,1fr)">
            <LaneHead>
              <div>Agent</div>
              <div>Skills root</div>
            </LaneHead>
            {TARGETS.map((target) => (
              <LaneRow key={target.tool}>
                <div className="text-[14px] text-foreground">{target.tool}</div>
                <div className="truncate font-mono text-[13px] text-muted-foreground">
                  {target.root}
                </div>
              </LaneRow>
            ))}
          </Lane>
          <p className="mt-4 max-w-prose text-[13px] leading-relaxed text-muted-foreground">
            Canonical dest is always{" "}
            <code className="font-mono">.agents/skills</code>. Extra dirs like
            Claude Code are opt-in via{" "}
            <code className="font-mono">skl targets</code> or{" "}
            <code className="font-mono">skl use -a</code>. Resolved on each
            machine, not stored on the server.
          </p>
        </section>

        <section>
          <Label className="mb-4">Danger zone</Label>
          {unauthenticated ? (
            <AuthGate />
          ) : (
            <div className="border-t border-border pt-6">
              <h3 className="font-sans text-[15px] font-semibold text-foreground">
                Revoke every device
              </h3>
              <p className="mt-2 max-w-prose text-[13px] leading-relaxed text-muted-foreground">
                {active.length === 0
                  ? "No machine currently holds a device token."
                  : active.length === 1
                    ? "Invalidates the one device token currently in use."
                    : `Invalidates all ${active.length} device tokens currently in use.`}{" "}
                Skills already written to disk are untouched, and your synced
                skills are not deleted.
              </p>
              <div className="mt-5 flex items-center gap-6">
                <ConfirmDialog
                  trigger={
                    <DangerButton disabled={pending || active.length === 0}>
                      {pending ? "Revoking…" : "Revoke all devices"}
                    </DangerButton>
                  }
                  title="Revoke every device?"
                  description="All machines lose access immediately and will each need a new code from skl login. This cannot be undone."
                  confirmLabel="Revoke all"
                  pending={pending}
                  onConfirm={() => void revokeAll()}
                />
                <ActionLink href="/devices">Revoke one at a time</ActionLink>
              </div>
            </div>
          )}
        </section>
      </div>
    </>
  );
}
