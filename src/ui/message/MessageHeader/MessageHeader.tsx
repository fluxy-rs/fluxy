import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { MessageHeaderProps } from "./MessageHeader.types";

export function MessageHeader(props: MessageHeaderProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="message-header"
      class={clsx(
        "flex max-w-full min-w-0 items-center px-3 text-xs font-medium text-muted-foreground group-has-data-[variant=ghost]/message:px-0",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
