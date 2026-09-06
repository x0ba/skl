import { SignUp } from "@clerk/nextjs";
import type { Metadata } from "next";
import { ClerkNotConfigured } from "@/components/clerk-not-configured";
import {
  clerkSignUpRedirects,
  redirectFromSearchParams,
} from "@/lib/auth-redirect";
import { clerkAppearance } from "@/lib/clerk-appearance";
import { isClerkEnabled } from "@/lib/config";

export const metadata: Metadata = {
  title: "Sign up",
};

export default async function SignUpPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  if (!isClerkEnabled()) {
    return <ClerkNotConfigured action="Sign up" />;
  }

  const redirectUrl = redirectFromSearchParams(await searchParams);

  return (
    <SignUp appearance={clerkAppearance} {...clerkSignUpRedirects(redirectUrl)} />
  );
}
