import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { MessageAvatarProps } from "./MessageAvatar.types";

/**
 */
export function MessageAvatar(props: MessageAvatarProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="message-avatar"
      class={clsx(
        "flex w-fit min-w-8 shrink-0 items-center justify-center self-end overflow-hidden rounded-full bg-muted group-has-data-[slot=message-footer]/message:-translate-y-8",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
