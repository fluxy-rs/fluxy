import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useMessageScrollerContext } from "../message-scroller.context";
import { scrollableData } from "../message-scroller.utils";
import type { MessageScrollerProps } from "../message-scroller.types";

export function MessageScroller(props: MessageScrollerProps) {
  const ctx = useMessageScrollerContext("MessageScroller");
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="message-scroller"
      data-scrollable={scrollableData(ctx.scrollableStart, ctx.scrollableEnd)}
      data-autoscrolling={ctx.autoscrolling() ? "" : undefined}
      data-pending-scroll={ctx.pendingScroll() ? "" : undefined}
      class={clsx(
        "group/message-scroller relative flex size-full min-h-0 flex-col overflow-hidden",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
