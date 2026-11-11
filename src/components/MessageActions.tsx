import { For, type Component } from "solid-js";
import { Sheet, SheetContent } from "../ui/sheet";

/** One row of the mobile action sheet. */
export type MessageAction = {
  label: string;
  icon: Component<{ size?: number; class?: string }>;
  destructive?: boolean;
  onSelect: () => void;
};

/**
 * The long-press menu a phone expects.
 *
 * A right-click menu has no place on a touch screen - the desktop one showed up
 * there by accident, anchored to a finger - so this is the platform idiom: a
 * sheet from the bottom with the actions laid out as a grid of icon and label,
 * the shape the messengers everyone already uses have trained people on.
 */
export function MessageActions(props: {
  open: boolean;
  onClose: () => void;
  actions: MessageAction[];
  label: string;
}) {
  return (
    <Sheet
      open={props.open}
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
    >
      <SheetContent
        side="bottom"
        aria-label={props.label}
        class="rounded-t-2xl pb-[max(1rem,env(safe-area-inset-bottom))]"
      >
        <div class="grid grid-cols-4 gap-1 p-2">
          <For each={props.actions}>
            {(action) => {
              const Icon = action.icon;
              return (
                <button
                  type="button"
                  class="flex flex-col items-center gap-1.5 rounded-lg px-2 py-3 text-label transition-colors hover:bg-muted active:bg-muted"
                  onClick={() => {
                    props.onClose();
                    action.onSelect();
                  }}
                >
                  <Icon
                    size={22}
                    class={action.destructive ? "text-destructive" : "text-foreground"}
                  />
                  <span class={action.destructive ? "text-destructive" : undefined}>
                    {action.label}
                  </span>
                </button>
              );
            }}
          </For>
        </div>
      </SheetContent>
    </Sheet>
  );
}
