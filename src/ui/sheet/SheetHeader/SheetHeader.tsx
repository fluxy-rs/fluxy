import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { SheetHeaderProps } from "../sheet.types";

export function SheetHeader(props: SheetHeaderProps) {
  const [local, others] = splitProps(props, ["class", "classList"]);
  return (
    <div
      {...others}
      data-slot="sheet-header"
      class={clsx("flex flex-col gap-1.5 p-4", local.class)}
      classList={local.classList}
    />
  );
}
