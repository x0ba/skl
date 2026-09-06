import { AppProviders } from "@/components/providers";
import { isClerkEnabled } from "@/lib/config";
import { AppShell } from "@/components/app-shell";

export default function AppLayout({ children }: LayoutProps<"/">) {
  return (
    <AppProviders clerkEnabled={isClerkEnabled()}>
      <AppShell>{children}</AppShell>
    </AppProviders>
  );
}
