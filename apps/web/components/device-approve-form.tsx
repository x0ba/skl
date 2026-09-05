"use client";

import { OTPField } from "@base-ui/react/otp-field";
import Link from "next/link";
import { Fragment, useState, type FormEvent } from "react";
import { LocalTokenField } from "@/components/local-token-field";
import { useSession } from "@/components/providers";
import { ActionLink } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { Field, Input } from "@/components/ui/field";
import { Label } from "@/components/ui/text";
import { ApiError, approveDevice, describeApproveError } from "@/lib/api";

/** `user_code` is 8 alphanumeric characters, shown to the user as ABCD-2345. */
const CODE_LENGTH = 8;

export function DeviceApproveForm({
  initialUserCode,
}: {
  initialUserCode: string;
}) {
  const session = useSession();
  const [userCode, setUserCode] = useState(() =>
    initialUserCode.replace(/[^a-zA-Z0-9]/g, "").toUpperCase().slice(0, CODE_LENGTH),
  );
  const [deviceName, setDeviceName] = useState("");
  const [pending, setPending] = useState(false);
  const [approvedId, setApprovedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const complete = userCode.length === CODE_LENGTH;

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setError(null);

    try {
      const token = await session.getAccessToken();
      if (!token) {
        setError("Sign in or set a bearer token before approving.");
        return;
      }

      const approved = await approveDevice(token, {
        user_code: userCode,
        ...(deviceName.trim() ? { device_name: deviceName.trim() } : {}),
      });
      setApprovedId(approved.device_id);
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

  if (approvedId) {
    return <Approved deviceId={approvedId} />;
  }

  return (
    <div>
      <Label className="mb-4">Device authorization</Label>
      <h1 className="font-sans text-[27px] font-bold tracking-[-0.03em] text-foreground">
        Approve this device
      </h1>
      <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
        Enter the code shown by{" "}
        <code className="font-mono text-foreground">skl login</code>.
      </p>

      {error ? (
        <Banner tone="danger" title="Could not approve" className="mt-6">
          {error}
        </Banner>
      ) : null}

      <form onSubmit={onSubmit} className="mt-8 space-y-8">
        <div>
          <Label className="mb-3">Code</Label>
          <OTPField.Root
            length={CODE_LENGTH}
            validationType="alphanumeric"
            value={userCode}
            onValueChange={(value) => setUserCode(value.toUpperCase())}
            className="flex items-center gap-1.5"
          >
            {Array.from({ length: CODE_LENGTH }, (_, index) => (
              <Fragment key={index}>
                {/* The dash users see in ABCD-2345, drawn rather than typed. */}
                {index === CODE_LENGTH / 2 ? (
                  <span aria-hidden className="mx-1 h-px w-2 bg-border" />
                ) : null}
                {/* Slot index comes from DOM order; the separator is not a slot. */}
                <OTPField.Input
                  className="size-9 border border-input bg-background text-center font-mono text-[15px] uppercase text-foreground caret-primary focus:border-primary focus:outline-none data-filled:border-foreground"
                />
              </Fragment>
            ))}
          </OTPField.Root>
        </div>

        <Field
          label="Device name (optional)"
          htmlFor="device-name"
          hint="Defaults to the name the CLI reported. Useful when you have several machines."
        >
          <Input
            id="device-name"
            value={deviceName}
            onChange={(event) => setDeviceName(event.target.value)}
            placeholder="mbp-16"
            autoComplete="off"
          />
        </Field>

        <button
          type="submit"
          disabled={pending || !complete || !session.isReady}
          className="h-9 w-full bg-primary px-4 font-mono text-[13px] text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-40"
        >
          {pending ? "Approving…" : "Approve device"}
        </button>
      </form>

      <div className="mt-10 border-t border-border pt-6">
        <LocalTokenField />
      </div>
    </div>
  );
}

function Approved({ deviceId }: { deviceId: string }) {
  return (
    <div>
      <Label className="mb-4">Device authorization</Label>
      <h1 className="font-sans text-[27px] font-bold tracking-[-0.03em] text-foreground">
        Device approved
      </h1>
      <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
        You can close this tab. The CLI has picked up its token and will finish
        on its own.
      </p>

      <dl className="mt-8 border-t border-border pt-4">
        <dt className="font-mono text-[11px] font-medium tracking-label text-faint">
          Device ID
        </dt>
        <dd className="mt-1.5 break-all font-mono text-[13px] text-foreground">
          {deviceId}
        </dd>
      </dl>

      <div className="mt-8 flex items-center gap-6">
        <ActionLink href="/devices">Manage devices</ActionLink>
        <Link
          href="/skills"
          className="font-mono text-[13px] text-muted-foreground underline decoration-from-font underline-offset-2 hover:text-foreground"
        >
          View skills
        </Link>
      </div>
    </div>
  );
}
