import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { MessageFooterProps } from "./MessageFooter.types";

export function MessageFooter(props: MessageFooterProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="message-footer"
      class={clsx(
        "flex max-w-full min-w-0 items-center px-3 text-xs font-medium text-muted-foreground group-has-data-[variant=ghost]/message:px-0 group-data-[align=end]/message:justify-end",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
