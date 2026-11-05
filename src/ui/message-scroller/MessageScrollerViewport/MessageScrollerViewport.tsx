import { createEffect, onCleanup, splitProps } from "solid-js";
import { clsx, mergeRefs } from "../../utils";
import { useMessageScrollerContext } from "../message-scroller.context";
import { scrollableData } from "../message-scroller.utils";
import type { MessageScrollerViewportProps } from "../message-scroller.types";

/**
 */
export function MessageScrollerViewport(props: MessageScrollerViewportProps) {
  const ctx = useMessageScrollerContext("MessageScrollerViewport");
  const [local, rest] = splitProps(props, [
    "class",
    "classList",
    "aria-label",
    "preserveScrollOnPrepend",
    "ref",
    "role",
    "tabIndex",
  ]);

  createEffect(() => ctx.setPreserveScrollOnPrepend(local.preserveScrollOnPrepend ?? true));

  return (
    <div
      {...rest}
      ref={mergeRefs(
        (el) => {
          ctx.setViewport(el as HTMLElement);
          onCleanup(() => ctx.setViewport(undefined));
        },
        local.ref as ((el: Element) => void) | undefined,
      )}
      data-slot="message-scroller-viewport"
      role={local.role ?? "region"}
      aria-label={local["aria-label"] ?? "Messages"}
      tabindex={local.tabIndex ?? 0}
      data-scrollable={scrollableData(ctx.scrollableStart, ctx.scrollableEnd)}
      data-pending-scroll={ctx.pendingScroll() ? "" : undefined}
      data-autoscrolling={ctx.autoscrolling() ? "" : undefined}
      class={clsx(
        "size-full min-h-0 min-w-0 scroll-fade-b scrollbar-thin scrollbar-gutter-stable overflow-y-auto overscroll-contain contain-content outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
        "data-autoscrolling:scrollbar-thumb-transparent data-autoscrolling:scrollbar-track-transparent",
        "data-pending-scroll:invisible",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
