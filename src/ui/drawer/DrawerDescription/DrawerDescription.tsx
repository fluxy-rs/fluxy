import { createUniqueId, onCleanup, onMount, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useDrawerContext } from "../Drawer/Drawer.context";
import type { DrawerDescriptionProps } from "./DrawerDescription.types";

export function DrawerDescription(props: DrawerDescriptionProps) {
  const ctx = useDrawerContext("DrawerDescription");
  const [local, rest] = splitProps(props, ["class", "classList"]);
  const id = `drawer-description-${createUniqueId()}`;

  onMount(() => {
    ctx.setDescriptionId(id);
    onCleanup(() => ctx.setDescriptionId(undefined));
  });

  return (
    <p
      {...rest}
      id={id}
      data-slot="drawer-description"
      class={clsx("text-sm text-balance text-muted-foreground", local.class)}
      classList={local.classList}
    />
  );
}
