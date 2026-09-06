import { clerkMiddleware, createRouteMatcher } from "@clerk/nextjs/server";
import { NextResponse } from "next/server";

const clerkEnabled = Boolean(
  process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim(),
);

const isDashboardRoute = createRouteMatcher([
  "/skills(.*)",
  "/devices(.*)",
  "/activity(.*)",
  "/settings(.*)",
]);

// Marketing, install.sh, and /device stay public. When Clerk is configured,
// the dashboard requires a session — local `dev:<user_id>` tokens still work
// if keys are unset.
export default clerkEnabled
  ? clerkMiddleware(async (auth, req) => {
      if (isDashboardRoute(req)) {
        await auth.protect();
      }
    })
  : function proxy() {
      return NextResponse.next();
    };

export const config = {
  matcher: [
    "/((?!_next|[^?]*\\.(?:html?|css|js(?!on)|jpe?g|webp|png|gif|svg|ttf|woff2?|ico|csv|docx?|xlsx?|zip|webmanifest|sh)).*)",
    "/(api|trpc)(.*)",
  ],
};
