import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";

export function PopoverTitle(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return <div data-slot="popover-title" class={clsx("font-medium", local.class)} {...rest} />;
}
