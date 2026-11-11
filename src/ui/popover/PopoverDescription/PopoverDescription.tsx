import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";

export function PopoverDescription(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <div
      data-slot="popover-description"
      class={clsx("text-muted-foreground", local.class)}
      {...rest}
    />
  );
}
