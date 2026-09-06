"use client";

import { OTPField } from "@base-ui/react/otp-field";
import Link from "next/link";
import { Fragment, useState, type FormEvent, type ReactNode } from "react";
import { LocalTokenField } from "@/components/local-token-field";
import { useSession } from "@/components/providers";
import { ActionLink } from "@/components/ui/action-link";
import { Banner } from "@/components/ui/banner";
import { Field, Input } from "@/components/ui/field";
import { Label } from "@/components/ui/text";
import { ApiError, approveDevice, describeApproveError } from "@/lib/api";
import {
  DEVICE_USER_CODE_LENGTH,
  deviceApproveHref,
  formatDeviceUserCode,
  normalizeDeviceUserCode,
  withRedirectUrl,
} from "@/lib/auth-redirect";

const primaryButtonClassName =
  "inline-flex h-9 w-full items-center justify-center bg-primary px-4 font-mono text-[13px] text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-40";

export function DeviceApproveForm({
  initialUserCode,
}: {
  initialUserCode: string;
}) {
  const session = useSession();
  const [userCode, setUserCode] = useState(() =>
    normalizeDeviceUserCode(initialUserCode),
  );
  const [deviceName, setDeviceName] = useState("");
  const [pending, setPending] = useState(false);
  const [approvedId, setApprovedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const complete = userCode.length === DEVICE_USER_CODE_LENGTH;
  const canApprove = !session.clerkEnabled || session.isSignedIn;

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canApprove) {
      return;
    }
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

  if (session.clerkEnabled && !session.isReady) {
    return (
      <DeviceChrome>
        <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
          Checking your session…
        </p>
      </DeviceChrome>
    );
  }

  if (session.clerkEnabled && !session.isSignedIn) {
    return <SignInToApprove userCode={userCode} />;
  }

  return (
    <DeviceChrome>
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
            length={DEVICE_USER_CODE_LENGTH}
            validationType="alphanumeric"
            value={userCode}
            onValueChange={(value) => setUserCode(value.toUpperCase())}
            className="flex items-center gap-1.5"
          >
            {Array.from({ length: DEVICE_USER_CODE_LENGTH }, (_, index) => (
              <Fragment key={index}>
                {index === DEVICE_USER_CODE_LENGTH / 2 ? (
                  <span aria-hidden className="mx-1 h-px w-2 bg-border" />
                ) : null}
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
          className={primaryButtonClassName}
        >
          {pending ? "Approving…" : "Approve device"}
        </button>
      </form>

      <div className="mt-10 border-t border-border pt-6">
        <LocalTokenField />
      </div>
    </DeviceChrome>
  );
}

function DeviceChrome({ children }: { children: ReactNode }) {
  return (
    <div>
      <Label className="mb-4">Device authorization</Label>
      <h1 className="font-sans text-[27px] font-bold tracking-[-0.03em] text-foreground">
        Approve this device
      </h1>
      {children}
    </div>
  );
}

function SignInToApprove({ userCode }: { userCode: string }) {
  const returnTo = deviceApproveHref(userCode);
  const displayCode = formatDeviceUserCode(userCode);

  return (
    <DeviceChrome>
      <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
        The CLI is waiting. Sign in or create an account, then you can approve
        this device.
      </p>

      {displayCode ? (
        <p className="mt-6 font-mono text-[15px] tracking-[0.18em] text-foreground">
          {displayCode}
        </p>
      ) : null}

      <div className="mt-8 space-y-6">
        <Link href={withRedirectUrl("/sign-in", returnTo)} className={primaryButtonClassName}>
          Sign in
        </Link>
        <ActionLink href={withRedirectUrl("/sign-up", returnTo)}>
          Create an account
        </ActionLink>
      </div>
    </DeviceChrome>
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
