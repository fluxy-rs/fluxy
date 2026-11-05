import {
  Dialog as NeutDialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  type DialogProps,
} from "../dialog";
import { X } from "lucide-solid";
import { useI18n } from "../../i18n";
import { Show, createEffect, onCleanup, splitProps, type JSX } from "solid-js";

export type AppDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children?: JSX.Element;
  footer?: JSX.Element;
  trigger?: JSX.Element;
  width?: "sm" | "md";
  returnFocus?: HTMLElement | null;
  /**
   * When false the dialog can only be closed by its own buttons: Escape, a
   * click on the backdrop and the corner X are all ignored. A prompt whose
   * answer unlocks something has to stay answerable.
   */
  dismissible?: boolean;
} & Omit<DialogProps, "open" | "onOpenChange" | "children">;

function focusable(surface: HTMLElement | undefined): HTMLElement[] {
  if (!surface) return [];
  return Array.from(
    surface.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => element.offsetParent !== null);
}

export function Dialog(props: AppDialogProps) {
  const { t } = useI18n();
  const [local, rest] = splitProps(props, [
    "open",
    "onOpenChange",
    "title",
    "description",
    "children",
    "footer",
    "trigger",
    "width",
    "returnFocus",
    "dismissible",
  ]);

  let surface: HTMLDivElement | undefined;
  let previousFocus: HTMLElement | null = null;
  let focusFrame = 0;
  let restoreTimer = 0;

  const onKeyDown = (event: KeyboardEvent) => {
    if (!local.open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      // A prompt that must be answered ignores Escape; its buttons decide.
      if (local.dismissible !== false) local.onOpenChange(false);
      return;
    }
    if (event.key !== "Tab") return;

    const items = focusable(surface);
    if (items.length === 0) {
      event.preventDefault();
      surface?.focus();
      return;
    }

    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;

    if (event.shiftKey && (active === first || !surface?.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !surface?.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  };

  // A trigger can disappear while the dialog is open (a peer goes offline), so
  // the element is only refocused when it is still part of the document.
  const restoreFocus = () => {
    const target = previousFocus;
    previousFocus = null;
    if (target && target !== document.body && target.isConnected) {
      target.focus();
    }
  };

  createEffect(() => {
    if (local.open) {
      previousFocus = local.returnFocus ?? (document.activeElement as HTMLElement | null);
      document.addEventListener("keydown", onKeyDown, true);
      focusFrame = window.requestAnimationFrame(() => {
        const items = focusable(surface);
        (items[0] ?? surface)?.focus();
      });
    } else {
      document.removeEventListener("keydown", onKeyDown, true);
      if (focusFrame) {
        window.cancelAnimationFrame(focusFrame);
        focusFrame = 0;
      }
      if (previousFocus && previousFocus !== document.body) {
        restoreTimer = window.setTimeout(() => {
          if (!local.open) restoreFocus();
        }, 0);
      }
    }
  });

  onCleanup(() => {
    document.removeEventListener("keydown", onKeyDown, true);
    if (focusFrame) {
      window.cancelAnimationFrame(focusFrame);
    }
    if (restoreTimer) {
      window.clearTimeout(restoreTimer);
    }
    // A parent may unmount the dialog instead of closing it - the pairing
    // dialog disappears when the challenge is withdrawn - and the focus it
    // captured still has to come back to the trigger.
    restoreFocus();
  });

  return (
    <NeutDialog
      {...rest}
      open={local.open}
      onOpenChange={(open) => {
        // A prompt that unlocks a blocked transfer must be answered, not
        // dismissed; its own buttons are the only way out.
        if (!open && local.dismissible === false) return;
        local.onOpenChange(open);
      }}
    >
      {local.trigger}
      <DialogContent
        ref={(element) => {
          surface = element;
        }}
        class="max-w-lg gap-4 p-5"
        classList={{ "max-w-sm": local.width === "sm" }}
        aria-modal="true"
        tabindex="-1"
        showCloseButton={false}
      >
        <DialogHeader>
          <DialogTitle>{local.title}</DialogTitle>
          <Show when={local.description}>
            <DialogDescription>{local.description}</DialogDescription>
          </Show>
        </DialogHeader>
        <Show when={local.children}>
          <div class="flex flex-col gap-3 text-sm">{local.children}</div>
        </Show>
        <Show when={local.footer}>
          <DialogFooter>{local.footer}</DialogFooter>
        </Show>
        <Show when={local.dismissible !== false}>
          <DialogClose
            class="absolute top-3 right-3 grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            aria-label={t("common.close")}
          >
            <X size={15} aria-hidden="true" />
          </DialogClose>
        </Show>
      </DialogContent>
    </NeutDialog>
  );
}
