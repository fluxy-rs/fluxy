import { onCleanup, type ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Button } from "../../button";
import { mergeRefs } from "../../utils";
import { useDrawerContext } from "../Drawer/Drawer.context";
import type { DrawerTriggerProps } from "../Drawer/Drawer.types";

export const DrawerTrigger = <T extends ValidComponent = typeof Button<"button">>(
  props: DrawerTriggerProps<T>,
) => {
  const ctx = useDrawerContext("DrawerTrigger");

  const attach = (el: Element) => {
    const onClick = () => {
      if (!ctx.open()) ctx.setOpen(true, "trigger-press");
    };
    el.addEventListener("click", onClick);
    onCleanup(() => el.removeEventListener("click", onClick));
  };

  return (
    <Dynamic
      {...props}
      component={(props.component as ValidComponent) ?? Button<"button">}
      ref={mergeRefs((el: Element) => ctx.setTrigger(el as HTMLElement), props.ref, attach)}
      data-slot="drawer-trigger"
      aria-haspopup="dialog"
      aria-expanded={ctx.open()}
      aria-controls={ctx.open() ? ctx.contentId : undefined}
      data-state={ctx.open() ? "open" : "closed"}
    />
  );
};
