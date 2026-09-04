"use client";

import { useCallback, useEffect, useState } from "react";
import { LocalTokenField } from "@/components/local-token-field";
import { useSession } from "@/components/providers";
import { Button } from "@/components/ui/button";
import { describeApiError, listDevices, listSkills, revokeDevice } from "@/lib/api";
import type { DeviceRecord } from "@/lib/contracts";

function formatWhen(value: string | null): string {
  if (!value) {
    return "—";
  }
  return value.replace("T", " ").replace(/\.\d+Z$/, "Z");
}

export function DashboardView() {
  const session = useSession();
  const [skillCount, setSkillCount] = useState<number | null>(null);
  const [devices, setDevices] = useState<DeviceRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [revokingId, setRevokingId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const token = await session.getAccessToken();
      if (!token) {
        setSkillCount(null);
        setDevices(null);
        setError("Sign in or set a local Bearer token to load the dashboard.");
        return;
      }

      const [skillsRes, devicesRes] = await Promise.all([
        listSkills(token),
        listDevices(token),
      ]);
      setSkillCount(skillsRes.skills.length);
      setDevices(devicesRes.devices);
    } catch (caught) {
      setSkillCount(null);
      setDevices(null);
      setError(describeApiError(caught));
    } finally {
      setLoading(false);
    }
  }, [session]);

  useEffect(() => {
    if (!session.isReady) {
      return;
    }
    const timer = window.setTimeout(() => {
      void refresh();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [refresh, session.isReady, session.isSignedIn, session.localToken]);

  async function onRevoke(id: string) {
    const token = await session.getAccessToken();
    if (!token) {
      setError("Sign in or set a local Bearer token before revoking.");
      return;
    }
    setRevokingId(id);
    setError(null);
    try {
      await revokeDevice(token, id);
      await refresh();
    } catch (caught) {
      setError(describeApiError(caught));
    } finally {
      setRevokingId(null);
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="space-y-1">
          <h1 className="text-xl font-medium tracking-tight">Dashboard</h1>
          <p className="text-sm text-muted-foreground">
            Skill count from <code>GET /v1/skills</code>. Devices from{" "}
            <code>GET /v1/devices</code>.
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          onClick={() => void refresh()}
          disabled={loading || !session.isReady}
        >
          {loading ? "Loading…" : "Refresh"}
        </Button>
      </div>

      <LocalTokenField />

      {error ? <p className="text-sm text-destructive">{error}</p> : null}

      <section className="border border-border p-4">
        <p className="text-xs text-muted-foreground">Skills</p>
        <p className="mt-1 text-3xl font-medium tracking-tight">
          {skillCount === null ? "—" : skillCount}
        </p>
      </section>

      <section className="space-y-3">
        <h2 className="text-sm font-medium">Devices</h2>
        {devices === null ? (
          <p className="text-sm text-muted-foreground">No devices loaded.</p>
        ) : devices.length === 0 ? (
          <p className="text-sm text-muted-foreground">No devices yet.</p>
        ) : (
          <div className="overflow-x-auto border border-border">
            <table className="w-full text-left text-xs">
              <thead className="border-b border-border bg-muted/40">
                <tr>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium">Created</th>
                  <th className="px-3 py-2 font-medium">Last used</th>
                  <th className="px-3 py-2 font-medium">Revoked</th>
                  <th className="px-3 py-2 font-medium" />
                </tr>
              </thead>
              <tbody>
                {devices.map((device) => (
                  <tr key={device.id} className="border-b border-border last:border-0">
                    <td className="px-3 py-2">{device.name}</td>
                    <td className="px-3 py-2 text-muted-foreground">
                      {formatWhen(device.created_at)}
                    </td>
                    <td className="px-3 py-2 text-muted-foreground">
                      {formatWhen(device.last_used_at)}
                    </td>
                    <td className="px-3 py-2 text-muted-foreground">
                      {device.revoked_at ? formatWhen(device.revoked_at) : "—"}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {device.revoked_at ? null : (
                        <Button
                          type="button"
                          variant="destructive"
                          size="xs"
                          disabled={revokingId === device.id}
                          onClick={() => void onRevoke(device.id)}
                        >
                          {revokingId === device.id ? "Revoking…" : "Revoke"}
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
