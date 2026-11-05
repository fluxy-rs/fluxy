import { mergeProps, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { bubbleReactionsVariants } from "./BubbleReactions.styles";
import type { BubbleReactionsProps } from "./BubbleReactions.types";

/**
 */
export function BubbleReactions(props: BubbleReactionsProps) {
  const merged = mergeProps({ side: "bottom", align: "end" } as const, props);
  const [local, rest] = splitProps(merged, ["side", "align", "class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="bubble-reactions"
      data-align={local.align}
      data-side={local.side}
      class={clsx(bubbleReactionsVariants({ side: local.side, align: local.align }), local.class)}
      classList={local.classList}
    />
  );
}
