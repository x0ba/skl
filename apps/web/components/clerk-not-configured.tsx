import { ActionLink } from "@/components/ui/action-link";
import { Label } from "@/components/ui/text";

/**
 * Local-development fallback for the Clerk-backed auth routes. Names the exact
 * env vars, since that is the only thing standing between here and a real
 * sign-in form.
 */
export function ClerkNotConfigured({ action }: { action: string }) {
  return (
    <div>
      <Label className="mb-4">Auth</Label>
      <h1 className="font-sans text-[27px] font-bold tracking-[-0.03em] text-foreground">
        {action}
      </h1>
      <p className="mt-3 text-[14px] leading-relaxed text-muted-foreground">
        Clerk is not configured in this environment.
      </p>

      <dl className="mt-8 space-y-3 border-t border-border pt-6 font-mono text-[12px]">
        <div>
          <dt className="text-faint">Set</dt>
          <dd className="mt-1 text-foreground">
            NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY
          </dd>
          <dd className="text-foreground">CLERK_SECRET_KEY</dd>
        </div>
        <div>
          <dt className="text-faint">Or authorize directly with</dt>
          <dd className="mt-1 text-foreground">
            Authorization: Bearer dev:&lt;user_id&gt;
          </dd>
        </div>
      </dl>

      <div className="mt-8">
        <ActionLink href="/skills">Continue to the dashboard</ActionLink>
      </div>
    </div>
  );
}
