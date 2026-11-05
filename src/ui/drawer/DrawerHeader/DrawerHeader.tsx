import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { DrawerHeaderProps } from "./DrawerHeader.types";

export function DrawerHeader(props: DrawerHeaderProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="drawer-header"
      class={clsx(
        "flex shrink-0 flex-col gap-0.5 p-4 pb-0 group-data-[swipe-axis=y]/drawer-popup:text-center md:text-left",
        local.class,
      )}
      classList={local.classList}
    />
  );
}
