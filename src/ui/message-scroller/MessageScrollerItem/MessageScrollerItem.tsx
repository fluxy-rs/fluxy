import { onCleanup, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useMessageScrollerContext } from "../message-scroller.context";
import type { MessageScrollerItemProps } from "../message-scroller.types";

/**
 */
export function MessageScrollerItem(props: MessageScrollerItemProps) {
  const ctx = useMessageScrollerContext("MessageScrollerItem");
  const [local, rest] = splitProps(props, ["class", "classList", "messageId", "scrollAnchor"]);

  return (
    <div
      {...rest}
      ref={(el) => {
        const unregister = ctx.registerItem({
          id: local.messageId,
          element: el,
        });
        onCleanup(unregister);
      }}
      data-slot="message-scroller-item"
      data-message-id={local.messageId}
      data-scroll-anchor={local.scrollAnchor === true ? "true" : "false"}
      class={clsx(
        "min-w-0 shrink-0 [contain-intrinsic-size:auto_10rem] [content-visibility:auto]",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
