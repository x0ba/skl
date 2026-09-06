/** `user_code` is 8 alphanumeric characters, shown as ABCD-2345. */
export const DEVICE_USER_CODE_LENGTH = 8;

export function firstSearchParam(
  value: string | string[] | undefined,
): string {
  if (Array.isArray(value)) {
    return value[0] ?? "";
  }
  return value ?? "";
}

export function normalizeDeviceUserCode(input: string): string {
  return input
    .replace(/[^a-zA-Z0-9]/g, "")
    .toUpperCase()
    .slice(0, DEVICE_USER_CODE_LENGTH);
}

export function formatDeviceUserCode(input: string): string {
  const normalized = normalizeDeviceUserCode(input);
  if (normalized.length <= 4) {
    return normalized;
  }
  return `${normalized.slice(0, 4)}-${normalized.slice(4)}`;
}

/** Path the CLI's `verification_uri_complete` should return to after sign-in. */
export function deviceApproveHref(userCode = ""): string {
  const formatted = formatDeviceUserCode(userCode);
  if (!formatted) {
    return "/device";
  }
  return `/device?user_code=${encodeURIComponent(formatted)}`;
}

export function withRedirectUrl(
  path: "/sign-in" | "/sign-up",
  returnTo: string,
): string {
  return `${path}?redirect_url=${encodeURIComponent(returnTo)}`;
}

/**
 * Same-origin path only. Clerk may pass a relative `/device?user_code=…` or
 * an absolute URL; both collapse to path + search + hash on this origin.
 */
export function safeRedirectPath(value: string | undefined): string | undefined {
  const raw = value?.trim();
  if (!raw || raw.startsWith("//")) {
    return undefined;
  }

  try {
    const url = raw.startsWith("/")
      ? new URL(raw, "https://skl.invalid")
      : new URL(raw);
    if (raw.startsWith("/") && url.origin !== "https://skl.invalid") {
      return undefined;
    }
    const path = `${url.pathname}${url.search}${url.hash}`;
    if (!path.startsWith("/") || path.startsWith("//")) {
      return undefined;
    }
    return path;
  } catch {
    return undefined;
  }
}

export function redirectFromSearchParams(
  params: Record<string, string | string[] | undefined>,
): string | undefined {
  return safeRedirectPath(
    firstSearchParam(params.redirect_url) || firstSearchParam(params.redirectUrl),
  );
}

export function clerkSignInRedirects(redirectUrl: string | undefined) {
  if (!redirectUrl) {
    return {};
  }
  return {
    forceRedirectUrl: redirectUrl,
    fallbackRedirectUrl: redirectUrl,
    signUpForceRedirectUrl: redirectUrl,
    signUpFallbackRedirectUrl: redirectUrl,
    signUpUrl: withRedirectUrl("/sign-up", redirectUrl),
  };
}

export function clerkSignUpRedirects(redirectUrl: string | undefined) {
  if (!redirectUrl) {
    return {};
  }
  return {
    forceRedirectUrl: redirectUrl,
    fallbackRedirectUrl: redirectUrl,
    signInForceRedirectUrl: redirectUrl,
    signInFallbackRedirectUrl: redirectUrl,
    signInUrl: withRedirectUrl("/sign-in", redirectUrl),
  };
}
