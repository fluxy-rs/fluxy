import { For, createEffect, type Component } from "solid-js";
import { createVirtualElement } from "../ui/lib";
import { Popover, PopoverContent } from "../ui/popover";
import { usePopoverContext } from "../ui/popover/Popover/Popover.context";

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

/**
 * The long-press menu a phone expects.
 *
 * It floats above the pressed message, as the messengers that taught people
 * this gesture do: a dark panel of icon-and-label cells, four to a row, close
 * enough to the message that it is obviously about that message.
 *
 * The placement is the library popover's job - side flipping, collision with
 * the viewport edge, dismissal on an outside press - so what is left here is
 * the anchor and the look.
 */
export function MessageActions(props: {
  open: boolean;
  anchor: MenuAnchor | null;
  actions: MessageAction[];
  onClose: () => void;
  label: string;
}) {
  return (
    <Popover
      open={props.open}
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
      // A message menu should not freeze the conversation behind it.
      lockScroll={false}
    >
      <PointerAnchor anchor={props.anchor} />
      <PopoverContent
        side="top"
        align="center"
        sideOffset={10}
        class="w-auto max-w-[min(22rem,calc(100vw-1rem))] gap-0 rounded-xl bg-[#4c4c4c] p-1 text-white shadow-2xl ring-1 ring-white/10"
      >
        <div
          role="menu"
          aria-label={props.label}
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
                  class="flex w-20 flex-col items-center justify-center gap-1.5 rounded-lg px-1 py-2 transition-colors hover:bg-white/10 active:bg-white/15"
                  onClick={() => {
                    props.onClose();
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
      </PopoverContent>
    </Popover>
  );
}

/**
 * Turns a point into something the popover can measure.
 *
 * The trigger component anchors to a rendered element, but the thing that was
 * pressed is a message the popover does not own, so this registers a detached
 * element whose reported box is the press point. The positioner only ever asks
 * for that box, which keeps the menu - and its collision handling - in the
 * library rather than reimplemented against raw coordinates.
 */
function PointerAnchor(props: { anchor: MenuAnchor | null }) {
  const ctx = usePopoverContext("MessageActions");

  // A fresh element per anchor, not one element whose box changes: the
  // positioner watches the reference identity, so reusing it would leave the
  // menu where the first press put it. Mounting is not enough either - the
  // component is alive before the first long press, when there is no anchor.
  createEffect(() => {
    const anchor = props.anchor;
    if (!anchor) return;
    // The library builds these for exactly this: a press point standing in
    // for a trigger element. The cast is the popover context being narrower
    // than the positioner it feeds.
    const element = createVirtualElement({ x: anchor.x, y: anchor.y });
    // The popover asks its reference whether a press landed inside it. A
    // virtual element has no DOM, so answer no rather than not answering:
    // a missing method makes the outside-press handler throw and the menu
    // then never closes.
    const reference = Object.assign(element, { contains: () => false });
    ctx.setReference(reference as unknown as Element);
  });

  return null;
}
