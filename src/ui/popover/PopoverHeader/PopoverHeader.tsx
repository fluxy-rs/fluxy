import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";

export function PopoverHeader(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <div
      data-slot="popover-header"
      class={clsx("flex flex-col gap-0.5 text-sm", local.class)}
      {...rest}
    />
  );
}
