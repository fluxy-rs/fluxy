import { For, Show, createSignal, onCleanup, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { cn } from "../cn";

/** One row of a context menu. */
export type ContextMenuItem = {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  /** Renders in the destructive colour; the caller still has to confirm. */
  destructive?: boolean;
  /** Draw a rule above this item. */
  separatorBefore?: boolean;
};

/**
 * A right-click menu anchored to the pointer.
 *
 * The vendored library had no context menu, and the card that needs one is a
 * plain div inside a scroller, so this wraps its children rather than asking
 * the caller to invent a trigger element. It is deliberately not a full
 * menu-bar primitive: one level, no submenus, no checkable items.
 *
 * Escape and a pointer press outside close it; arrows move between enabled
 * items; focus returns to the wrapper so keyboard users are not dropped at the
 * top of the document.
 */
export function ContextMenu(props: {
  items: ContextMenuItem[];
  children: JSX.Element;
  class?: string;
  /** Skip the menu entirely, e.g. for a card that offers no actions. */
  disabled?: boolean;
}) {
  const [at, setAt] = createSignal<{ x: number; y: number } | null>(null);
  let wrapper: HTMLDivElement | undefined;
  let menu: HTMLDivElement | undefined;

  const close = (restoreFocus = true) => {
    setAt(null);
    if (restoreFocus) wrapper?.focus();
  };

  const onPointerDown = (event: PointerEvent) => {
    if (!at()) return;
    if (menu && event.target instanceof Node && menu.contains(event.target)) return;
    close(false);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (!at()) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeyDown);
  onCleanup(() => {
    document.removeEventListener("pointerdown", onPointerDown, true);
    document.removeEventListener("keydown", onKeyDown);
  });

  const move = (delta: number) => {
    const buttons = [...(menu?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
    if (buttons.length === 0) return;
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = (current + delta + buttons.length) % buttons.length;
    buttons[next].focus();
  };

  return (
    <div
      ref={wrapper}
      class={props.class}
      tabindex="-1"
      onContextMenu={(event) => {
        if (props.disabled || props.items.length === 0) return;
        event.preventDefault();
        setAt({ x: event.clientX, y: event.clientY });
      }}
    >
      {props.children}
      <Show when={at()}>
        {(point) => (
          <Portal>
            <div
              ref={menu}
              role="menu"
              aria-orientation="vertical"
              class="fixed z-50 min-w-[13rem] overflow-hidden rounded-lg border border-border bg-card p-1 text-body shadow-lg"
              style={{
                left: `${Math.min(point().x, window.innerWidth - 220)}px`,
                top: `${Math.min(point().y, window.innerHeight - 220)}px`,
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown") {
                  event.preventDefault();
                  move(1);
                } else if (event.key === "ArrowUp") {
                  event.preventDefault();
                  move(-1);
                }
              }}
            >
              <For each={props.items}>
                {(item) => (
                  <>
                    <Show when={item.separatorBefore}>
                      <div class="my-1 h-px bg-border" role="separator" />
                    </Show>
                    <button
                      type="button"
                      role="menuitem"
                      disabled={item.disabled}
                      class={cn(
                        "flex w-full items-center rounded-md px-2.5 py-1.5 text-left transition-colors",
                        "hover:bg-muted focus:bg-muted focus:outline-none disabled:pointer-events-none disabled:opacity-50",
                        item.destructive ? "text-destructive" : "text-foreground",
                      )}
                      onClick={() => {
                        close();
                        item.onSelect();
                      }}
                    >
                      {item.label}
                    </button>
                  </>
                )}
              </For>
            </div>
          </Portal>
        )}
      </Show>
    </div>
  );
}
