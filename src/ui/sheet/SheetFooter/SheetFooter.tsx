import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { SheetFooterProps } from "../sheet.types";

export function SheetFooter(props: SheetFooterProps) {
  const [local, others] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...others}
      data-slot="sheet-footer"
      class={clsx("mt-auto flex flex-col gap-2 p-4", local.class)}
      classList={local.classList}
    />
  );
}
