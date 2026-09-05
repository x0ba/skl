"use client";

import { AlertDialog } from "@base-ui/react/alert-dialog";
import type { ReactNode } from "react";

/**
 * Confirmation for irreversible actions. Uses AlertDialog rather than Dialog
 * so it traps focus and cannot be dismissed by clicking away — revoking a
 * device should take a deliberate answer.
 */
export function ConfirmDialog({
  trigger,
  title,
  description,
  confirmLabel,
  onConfirm,
  pending,
}: {
  trigger: ReactNode;
  title: string;
  description: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  pending?: boolean;
}) {
  return (
    <AlertDialog.Root>
      <AlertDialog.Trigger render={trigger as React.ReactElement} />
      <AlertDialog.Portal>
        <AlertDialog.Backdrop className="fixed inset-0 bg-foreground/20 transition-opacity data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <AlertDialog.Popup className="fixed left-1/2 top-1/2 w-[calc(100vw-2rem)] max-w-md -translate-x-1/2 -translate-y-1/2 border border-foreground bg-background p-6 transition-opacity data-ending-style:opacity-0 data-starting-style:opacity-0">
          <AlertDialog.Title className="font-sans text-[19px] font-semibold tracking-[-0.025em] text-foreground">
            {title}
          </AlertDialog.Title>
          <AlertDialog.Description className="mt-2 text-[14px] leading-relaxed text-muted-foreground">
            {description}
          </AlertDialog.Description>
          <div className="mt-7 flex items-center justify-end gap-6">
            <AlertDialog.Close className="font-mono text-[13px] text-muted-foreground underline decoration-from-font underline-offset-2 hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring">
              Cancel
            </AlertDialog.Close>
            <AlertDialog.Close
              onClick={onConfirm}
              disabled={pending}
              className="font-mono text-[13px] text-destructive underline decoration-from-font underline-offset-2 hover:decoration-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50"
            >
              {pending ? "Working…" : confirmLabel}
            </AlertDialog.Close>
          </div>
        </AlertDialog.Popup>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
