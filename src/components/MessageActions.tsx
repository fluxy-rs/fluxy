import { For, Show, createUniqueId, type Component } from "solid-js";
import { Portal } from "solid-js/web";

/** One row of the message menu. */
export type MessageAction = {
  label: string;
  icon: Component<{ size?: number; class?: string }>;
  destructive?: boolean;
  onSelect: () => void;
};

/** Where the finger pressed, so the menu can sit next to that message. */
export type MenuAnchor = { x: number; y: number };

const COLUMNS = 4;
const CELL_WIDTH = 84;
const CELL_HEIGHT = 74;
const GAP = 4;
const MARGIN = 12;

/**
 * The long-press menu a phone expects.
 *
 * It floats above the pressed message, as the messengers that taught people
 * this gesture do: a dark panel of icon-and-label cells, four to a row, close
 * enough to the message that it is obviously about that message. A sheet from
 * the bottom would have been a different idiom, and further from the thing
 * that was pressed.
 *
 * The panel is placed from the anchor the caller passes - the top edge and
 * horizontal centre of the pressed bubble - and flips below it when there is
 * no room above.
 */
export function MessageActions(props: {
  open: boolean;
  anchor: MenuAnchor | null;
  actions: MessageAction[];
  onClose: () => void;
  label: string;
}) {
  const id = `menu-${createUniqueId()}`;

  const geometry = () => {
    const rows = Math.ceil(props.actions.length / COLUMNS);
    const width = Math.min(
      COLUMNS * CELL_WIDTH + (COLUMNS + 1) * GAP,
      window.innerWidth - MARGIN * 2,
    );
    const height = rows * CELL_HEIGHT + (rows + 1) * GAP;
    const anchor = props.anchor ?? { x: window.innerWidth / 2, y: window.innerHeight / 2 };
    const left = Math.min(
      Math.max(MARGIN, anchor.x - width / 2),
      window.innerWidth - width - MARGIN,
    );
    const above = anchor.y - height - MARGIN;
    const top =
      above >= MARGIN
        ? above
        : Math.min(anchor.y + MARGIN * 3, window.innerHeight - height - MARGIN);
    return { left, top, width };
  };

  const close = () => props.onClose();

  return (
    <Show when={props.open && props.actions.length > 0}>
      <Portal>
        {/* Any press beside the menu dismisses it, as it does in the apps this
            imitates. The pressed message stays visible behind it. */}
        <div class="fixed inset-0 z-40" aria-hidden="true" onPointerDown={close} onClick={close} />
        <div
          id={id}
          role="menu"
          aria-label={props.label}
          class="fixed z-50 rounded-xl bg-[#4c4c4c] p-1 text-white shadow-2xl"
          style={{
            left: `${geometry().left}px`,
            top: `${geometry().top}px`,
            width: `${geometry().width}px`,
          }}
        >
          <div
            class="grid gap-1"
            style={{ "grid-template-columns": `repeat(${COLUMNS}, minmax(0, 1fr))` }}
          >
            <For each={props.actions}>
              {(action) => {
                const Icon = action.icon;
                return (
                  <button
                    type="button"
                    role="menuitem"
                    class="flex flex-col items-center justify-center gap-1.5 rounded-lg px-1 py-2 transition-colors hover:bg-white/10 active:bg-white/15"
                    style={{ height: `${CELL_HEIGHT - GAP * 2}px` }}
                    onClick={() => {
                      close();
                      action.onSelect();
                    }}
                  >
                    <Icon size={22} class={action.destructive ? "text-[#ff8a80]" : "text-white"} />
                    <span
                      class="text-xs leading-tight"
                      classList={{ "text-[#ff8a80]": action.destructive }}
                    >
                      {action.label}
                    </span>
                  </button>
                );
              }}
            </For>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
