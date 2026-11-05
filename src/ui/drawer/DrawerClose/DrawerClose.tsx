import { onCleanup, type ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Button } from "../../button";
import { mergeRefs } from "../../utils";
import { useDrawerContext } from "../Drawer/Drawer.context";
import type { DrawerCloseProps } from "../Drawer/Drawer.types";

export const DrawerClose = <T extends ValidComponent = typeof Button<"button">>(
  props: DrawerCloseProps<T>,
) => {
  const ctx = useDrawerContext("DrawerClose");

  const attach = (el: Element) => {
    const onClick = () => ctx.setOpen(false, "close-press");
    el.addEventListener("click", onClick);
    onCleanup(() => el.removeEventListener("click", onClick));
  };

  return (
    <Dynamic
      {...props}
      component={(props.component as ValidComponent) ?? Button<"button">}
      ref={mergeRefs(props.ref, attach)}
      data-slot="drawer-close"
    />
  );
};
