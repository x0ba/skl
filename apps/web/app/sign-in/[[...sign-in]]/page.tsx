import { SignIn } from "@clerk/nextjs";
import { isClerkEnabled } from "@/lib/config";

export default function SignInPage() {
  if (!isClerkEnabled()) {
    return (
      <div className="space-y-2">
        <h1 className="text-xl font-medium tracking-tight">Sign in</h1>
        <p className="text-sm text-muted-foreground">
          Clerk is not configured. Set{" "}
          <code>NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY</code> and{" "}
          <code>CLERK_SECRET_KEY</code>, or use a local{" "}
          <code>dev:&lt;user_id&gt;</code> Bearer token on the dashboard.
        </p>
      </div>
    );
  }

  return <SignIn />;
}
