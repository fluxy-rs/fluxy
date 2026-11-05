import { createUniqueId, onMount, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useDialogContentContext } from "../../dialog";
import { classes } from "./SheetTitle.styles";
import type { SheetTitleProps } from "../sheet.types";

export function SheetTitle(props: SheetTitleProps) {
  const { setTitleID } = useDialogContentContext();
  const [local, others] = splitProps(props, ["class", "classList"]);
  const id = createUniqueId();

  onMount(() => setTitleID(id));

  return (
    <h2
      id={id}
      data-slot="sheet-title"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...others}
    />
  );
}
