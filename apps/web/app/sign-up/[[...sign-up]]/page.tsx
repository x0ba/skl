import { SignUp } from "@clerk/nextjs";
import { isClerkEnabled } from "@/lib/config";

export default function SignUpPage() {
  if (!isClerkEnabled()) {
    return (
      <div className="space-y-2">
        <h1 className="text-xl font-medium tracking-tight">Sign up</h1>
        <p className="text-sm text-muted-foreground">
          Clerk is not configured. Set{" "}
          <code>NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY</code> and{" "}
          <code>CLERK_SECRET_KEY</code> to enable hosted sign-up.
        </p>
      </div>
    );
  }

  return <SignUp />;
}
