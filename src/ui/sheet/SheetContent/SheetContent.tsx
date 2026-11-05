import { Show, mergeProps, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { DialogSurface } from "../../dialog";
import { SheetClose } from "../SheetClose";
import { closeClasses, contentBase, sideClasses } from "./SheetContent.styles";
import type { SheetContentProps } from "../sheet.types";

/**
 */
export const SheetContent = (props: SheetContentProps) => {
  const merged = mergeProps({ side: "right" as const, showCloseButton: true }, props);

  const [local, others] = splitProps(merged, [
    "side",
    "showCloseButton",
    "class",
    "classList",
    "children",
  ]);

  return (
    <DialogSurface
      {...others}
      role="dialog"
      data-slot="sheet-content"
      data-side={local.side}
      class={clsx(contentBase, sideClasses(local.side), local.class)}
      classList={local.classList}
    >
      {local.children}
      <Show when={local.showCloseButton}>
        <SheetClose class={closeClasses(local.side)} />
      </Show>
    </DialogSurface>
  );
};
