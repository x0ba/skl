import { SignIn } from "@clerk/nextjs";
import type { Metadata } from "next";
import { ClerkNotConfigured } from "@/components/clerk-not-configured";
import {
  clerkSignInRedirects,
  redirectFromSearchParams,
} from "@/lib/auth-redirect";
import { clerkAppearance } from "@/lib/clerk-appearance";
import { isClerkEnabled } from "@/lib/config";

export const metadata: Metadata = {
  title: "Sign in",
};

export default async function SignInPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  if (!isClerkEnabled()) {
    return <ClerkNotConfigured action="Sign in" />;
  }

  const redirectUrl = redirectFromSearchParams(await searchParams);

  return (
    <SignIn appearance={clerkAppearance} {...clerkSignInRedirects(redirectUrl)} />
  );
}
