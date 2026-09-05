import { clerkMiddleware } from "@clerk/nextjs/server";
import { NextResponse } from "next/server";

const clerkEnabled = Boolean(
  process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim(),
);

// Pages stay reachable without a Clerk session so local `dev:<user_id>`
// Bearer tokens can call the API. clerkMiddleware still runs the handshake
// when keys are present.
export default clerkEnabled
  ? clerkMiddleware()
  : function proxy() {
      return NextResponse.next();
    };

export const config = {
  matcher: [
    "/((?!_next|[^?]*\\.(?:html?|css|js(?!on)|jpe?g|webp|png|gif|svg|ttf|woff2?|ico|csv|docx?|xlsx?|zip|webmanifest|sh)).*)",
    "/(api|trpc)(.*)",
  ],
};
