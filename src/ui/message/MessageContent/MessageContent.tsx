import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { MessageContentProps } from "./MessageContent.types";

export function MessageContent(props: MessageContentProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="message-content"
      class={clsx(
        "flex w-full min-w-0 flex-col gap-2.5 break-words group-data-[align=end]/message:*:data-slot:self-end",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
