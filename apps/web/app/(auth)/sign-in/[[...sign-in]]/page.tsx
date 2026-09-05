import { SignIn } from "@clerk/nextjs";
import type { Metadata } from "next";
import { ClerkNotConfigured } from "@/components/clerk-not-configured";
import { clerkAppearance } from "@/lib/clerk-appearance";
import { isClerkEnabled } from "@/lib/config";

export const metadata: Metadata = {
  title: "Sign in",
};

export default function SignInPage() {
  if (!isClerkEnabled()) {
    return <ClerkNotConfigured action="Sign in" />;
  }

  return <SignIn appearance={clerkAppearance} />;
}
