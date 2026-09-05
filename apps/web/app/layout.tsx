import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { AppProviders } from "@/components/providers";
import { isClerkEnabled } from "@/lib/config";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: {
    default: "skl",
    template: "%s · skl",
  },
  description: "Personal sync for your AI agent skills.",
};

/**
 * Page chrome lives in the route-group layouts — (marketing) is full-bleed,
 * (app) gets the sidebar, (auth) is centered — so this layout only sets up
 * fonts and session context.
 */
export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} antialiased`}
    >
      <body className="min-h-dvh bg-background text-foreground">
        <AppProviders clerkEnabled={isClerkEnabled()}>{children}</AppProviders>
      </body>
    </html>
  );
}
