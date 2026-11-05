import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { BubbleGroupProps } from "./BubbleGroup.types";

export function BubbleGroup(props: BubbleGroupProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="bubble-group"
      class={clsx("flex min-w-0 flex-col gap-2", local.class)}
      classList={local.classList}
    />
  );
}
