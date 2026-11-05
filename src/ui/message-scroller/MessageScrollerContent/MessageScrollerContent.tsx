import { onCleanup, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useMessageScrollerContext } from "../message-scroller.context";
import type { MessageScrollerContentProps } from "../message-scroller.types";

/**
 */
export function MessageScrollerContent(props: MessageScrollerContentProps) {
  const ctx = useMessageScrollerContext("MessageScrollerContent");
  const [local, rest] = splitProps(props, [
    "class",
    "classList",
    "aria-busy",
    "spacerClassName",
    "role",
    "aria-relevant",
    "children",
  ]);

  return (
    <div
      {...rest}
      ref={(el) => {
        ctx.setContent(el);
        onCleanup(() => ctx.setContent(undefined));
      }}
      data-slot="message-scroller-content"
      role={local.role ?? "log"}
      aria-relevant={local["aria-relevant"] ?? "additions"}
      aria-busy={local["aria-busy"]}
      class={clsx("flex h-max min-h-full flex-col gap-6", local.class)}
      classList={local.classList}
    >
      {local.children}
      <div
        ref={(el) => {
          ctx.setSpacer(el);
          onCleanup(() => ctx.setSpacer(undefined));
        }}
        aria-hidden="true"
        data-message-scroller-spacer=""
        hidden
        class={local.spacerClassName}
      />
    </div>
  );
}
