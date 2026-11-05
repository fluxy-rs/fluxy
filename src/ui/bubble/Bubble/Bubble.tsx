import { mergeProps, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { bubbleVariants } from "./Bubble.styles";
import type { BubbleProps } from "./Bubble.types";

export function Bubble(props: BubbleProps) {
  const merged = mergeProps({ variant: "default", align: "start" } as const, props);
  const [local, rest] = splitProps(merged, ["variant", "align", "class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="bubble"
      data-variant={local.variant}
      data-align={local.align}
      class={clsx(bubbleVariants({ variant: local.variant }), local.class)}
      classList={local.classList}
    />
  );
}
