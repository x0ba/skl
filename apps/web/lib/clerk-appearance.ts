/**
 * Bends Clerk's default card into the Hypertext system: square corners, no
 * shadow, mono for anything machine-shaped, and the one accent blue.
 *
 * Colors are literals rather than `var(--…)` because Clerk parses some of them
 * to derive hover and alpha shades, which it cannot do with a CSS variable.
 * Left untyped on purpose — `@clerk/types` is not a direct dependency here.
 */
export const clerkAppearance = {
  variables: {
    colorPrimary: "#0b24fb",
    colorText: "#0a0a0a",
    colorTextSecondary: "#5c5c5c",
    colorBackground: "#ffffff",
    colorInputBackground: "#ffffff",
    colorInputText: "#0a0a0a",
    colorDanger: "#cc1100",
    borderRadius: "0",
    fontFamily: "var(--font-geist-sans), sans-serif",
    fontSize: "14px",
  },
  elements: {
    rootBox: "w-full",
    cardBox: "w-full border border-border shadow-none",
    card: "shadow-none border-0 bg-background px-6 py-7",
    headerTitle: "font-sans text-[21px] font-bold tracking-[-0.03em]",
    headerSubtitle: "text-[14px] text-muted-foreground",
    socialButtonsBlockButton: "border border-border shadow-none hover:bg-secondary",
    dividerLine: "bg-border",
    dividerText: "font-mono text-[11px] tracking-label text-faint",
    formFieldLabel: "font-mono text-[11px] font-medium tracking-label text-faint",
    formFieldInput:
      "border-0 border-b border-input bg-transparent shadow-none focus:border-primary focus:ring-0",
    formButtonPrimary:
      "bg-primary text-primary-foreground shadow-none font-mono text-[13px] normal-case hover:opacity-90",
    footer: "bg-background",
    footerActionText: "text-[13px] text-muted-foreground",
    footerActionLink:
      "font-mono text-[13px] text-primary underline decoration-from-font underline-offset-2",
    identityPreview: "border border-border",
    formFieldInputShowPasswordButton: "text-faint hover:text-foreground",
  },
};
