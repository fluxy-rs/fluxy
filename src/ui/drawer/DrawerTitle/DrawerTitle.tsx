import { createUniqueId, onCleanup, onMount, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useDrawerContext } from "../Drawer/Drawer.context";
import type { DrawerTitleProps } from "./DrawerTitle.types";

export function DrawerTitle(props: DrawerTitleProps) {
  const ctx = useDrawerContext("DrawerTitle");
  const [local, rest] = splitProps(props, ["class", "classList"]);
  const id = `drawer-title-${createUniqueId()}`;

  onMount(() => {
    ctx.setTitleId(id);
    onCleanup(() => ctx.setTitleId(undefined));
  });

  return (
    <h2
      {...rest}
      id={id}
      data-slot="drawer-title"
      class={clsx("cn-font-heading text-base font-medium text-foreground", local.class)}
      classList={local.classList}
    />
  );
}
