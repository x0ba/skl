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

// Marketing, install.sh, and /device stay public so an unsigned CLI login
// can show a sign-in gate that returns to `/device?user_code=`. When Clerk
// is configured, the dashboard requires a session — local `dev:<user_id>`
// tokens still work if keys are unset.
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
    "/skills/:path*", "/devices/:path*", "/activity/:path*", "/settings/:path*",
    "/sign-in/:path*", "/sign-up/:path*", "/device/:path*",
  ],
};
