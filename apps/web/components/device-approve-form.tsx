"use client";

import { useState, type FormEvent } from "react";
import { LocalTokenField } from "@/components/local-token-field";
import { useSession } from "@/components/providers";
import { Button } from "@/components/ui/button";
import {
  ApiError,
  approveDevice,
  describeApproveError,
} from "@/lib/api";

export function DeviceApproveForm({
  initialUserCode,
}: {
  initialUserCode: string;
}) {
  const session = useSession();
  const [userCode, setUserCode] = useState(initialUserCode);
  const [deviceName, setDeviceName] = useState("");
  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setResult(null);
    setError(null);

    try {
      const token = await session.getAccessToken();
      if (!token) {
        setError("Sign in or set a local Bearer token before approving.");
        return;
      }

      const body = {
        user_code: userCode.trim(),
        ...(deviceName.trim() ? { device_name: deviceName.trim() } : {}),
      };
      const approved = await approveDevice(token, body);
      setResult(`Approved device ${approved.device_id}`);
    } catch (caught) {
      if (caught instanceof ApiError) {
        setError(describeApproveError(caught));
      } else if (caught instanceof Error) {
        setError(caught.message);
      } else {
        setError("Approve failed");
      }
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h1 className="text-xl font-medium tracking-tight">Approve a device</h1>
        <p className="text-sm text-muted-foreground">
          Paste the <code>user_code</code> from <code>skl login</code>. This
          calls <code>POST /v1/auth/device/approve</code>.
        </p>
      </div>

      <LocalTokenField />

      <form onSubmit={onSubmit} className="space-y-4">
        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">user_code</span>
          <input
            className="h-9 w-full border border-input bg-background px-2 font-mono text-sm tracking-wider uppercase outline-none focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/50"
            value={userCode}
            onChange={(event) => setUserCode(event.target.value)}
            placeholder="ABCD-2345"
            autoComplete="off"
            spellCheck={false}
            required
          />
        </label>
        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">
            device_name (optional)
          </span>
          <input
            className="h-9 w-full border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/50"
            value={deviceName}
            onChange={(event) => setDeviceName(event.target.value)}
            placeholder="cli"
            autoComplete="off"
          />
        </label>
        <Button type="submit" disabled={pending || !session.isReady}>
          {pending ? "Approving…" : "Approve device"}
        </Button>
      </form>

      {result ? <p className="text-sm">{result}</p> : null}
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
    </div>
  );
}
