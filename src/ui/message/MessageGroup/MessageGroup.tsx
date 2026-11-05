import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { MessageGroupProps } from "./MessageGroup.types";

export function MessageGroup(props: MessageGroupProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="message-group"
      class={clsx("flex min-w-0 flex-col gap-2", local.class)}
      classList={local.classList}
    />
  );
}
