"use client";

import { useState } from "react";
import { AuthGate } from "@/components/auth-gate";
import { useSession } from "@/components/providers";
import { ActionButton, ActionLink, DangerButton } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { Lane, LaneEnd, LaneHead, LaneRow } from "@/components/ui/lane";
import { PageHeader } from "@/components/ui/page-header";
import { StatusDot } from "@/components/ui/status-dot";
import { describeApiError, listDevices, revokeDevice } from "@/lib/api";
import type { DeviceRecord } from "@/lib/contracts";
import { exactTime, relativeTime } from "@/lib/format";
import { useResource } from "@/lib/use-resource";

const COLS = "minmax(0,1fr) 7rem 7rem 5rem";

export function DevicesView() {
  const session = useSession();
  const { data, error, loading, refreshing, unauthenticated, refresh } =
    useResource(listDevices);

  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [revokeError, setRevokeError] = useState<string | null>(null);

  const devices = data?.devices ?? [];

  async function onRevoke(device: DeviceRecord) {
    setRevokeError(null);
    const token = await session.getAccessToken();
    if (!token) {
      setRevokeError("No credentials. Sign in or set a bearer token first.");
      return;
    }
    setRevokingId(device.id);
    try {
      await revokeDevice(token, device.id);
      refresh();
    } catch (caught) {
      setRevokeError(describeApiError(caught));
    } finally {
      setRevokingId(null);
    }
  }

  return (
    <>
      <PageHeader
        eyebrow="Access"
        title="Devices"
        description="Machines holding a device token for this account. Revoking one takes effect on its next request."
        action={
          <ActionButton onClick={refresh} disabled={refreshing}>
            {refreshing ? "Refreshing…" : "Refresh"}
          </ActionButton>
        }
      />

      {error ? (
        <Banner tone="danger" title="Could not load devices" className="mb-8">
          {error}
        </Banner>
      ) : null}

      {revokeError ? (
        <Banner tone="danger" title="Revoke failed" className="mb-8">
          {revokeError}
        </Banner>
      ) : null}

      {unauthenticated ? (
        <AuthGate />
      ) : loading ? (
        <p className="border-t border-border py-14 font-mono text-[13px] text-faint">
          Loading…
        </p>
      ) : devices.length === 0 ? (
        <EmptyState
          title="No devices"
          action={<ActionLink href="/device">Approve a device</ActionLink>}
        >
          Run <code className="font-mono text-foreground">skl login</code> on a
          machine to get a code, then approve it here.
        </EmptyState>
      ) : (
        <Lane cols={COLS}>
          <LaneHead>
            <div>Device</div>
            <div>Added</div>
            <div>Last used</div>
            <LaneEnd />
          </LaneHead>
          {devices.map((device) => {
            const revoked = Boolean(device.revoked_at);
            return (
              <LaneRow key={device.id}>
                <div className="min-w-0">
                  <p
                    className={
                      revoked
                        ? "truncate font-mono text-[13px] text-faint line-through"
                        : "truncate font-mono text-[13px] text-foreground"
                    }
                  >
                    {device.name}
                  </p>
                  <StatusDot
                    className="mt-1.5 text-[12px]"
                    status={revoked ? "revoked" : "synced"}
                    label={
                      revoked
                        ? `Revoked ${relativeTime(device.revoked_at)}`
                        : "Active"
                    }
                  />
                </div>
                <div
                  className="font-mono text-[12px] text-faint"
                  title={exactTime(device.created_at)}
                >
                  {relativeTime(device.created_at)}
                </div>
                <div
                  className="font-mono text-[12px] text-faint"
                  title={exactTime(device.last_used_at)}
                >
                  {relativeTime(device.last_used_at)}
                </div>
                <LaneEnd>
                  {revoked ? null : (
                    <ConfirmDialog
                      trigger={
                        <DangerButton disabled={revokingId === device.id}>
                          {revokingId === device.id ? "Revoking…" : "Revoke"}
                        </DangerButton>
                      }
                      title={`Revoke ${device.name}?`}
                      description="That machine loses access immediately and will need a new code from skl login. Skills already on its disk are left alone."
                      confirmLabel="Revoke device"
                      pending={revokingId === device.id}
                      onConfirm={() => void onRevoke(device)}
                    />
                  )}
                </LaneEnd>
              </LaneRow>
            );
          })}
        </Lane>
      )}
    </>
  );
}
