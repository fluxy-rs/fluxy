import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { DrawerFooterProps } from "./DrawerFooter.types";

export function DrawerFooter(props: DrawerFooterProps) {
  const [local, rest] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...rest}
      data-slot="drawer-footer"
      class={clsx("mt-auto flex shrink-0 flex-col gap-2 p-4 pt-0", local.class)}
      classList={local.classList}
    />
  );
}
