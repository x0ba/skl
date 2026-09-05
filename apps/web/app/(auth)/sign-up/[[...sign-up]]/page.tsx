import { SignUp } from "@clerk/nextjs";
import type { Metadata } from "next";
import { ClerkNotConfigured } from "@/components/clerk-not-configured";
import { clerkAppearance } from "@/lib/clerk-appearance";
import { isClerkEnabled } from "@/lib/config";

export const metadata: Metadata = {
  title: "Sign up",
};

export default function SignUpPage() {
  if (!isClerkEnabled()) {
    return <ClerkNotConfigured action="Sign up" />;
  }

  return <SignUp appearance={clerkAppearance} />;
}
