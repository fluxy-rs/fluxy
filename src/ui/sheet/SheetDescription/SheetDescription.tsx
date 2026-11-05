import { createUniqueId, onMount, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useDialogContentContext } from "../../dialog";
import { classes } from "./SheetDescription.styles";
import type { SheetDescriptionProps } from "../sheet.types";

export function SheetDescription(props: SheetDescriptionProps) {
  const { setDescriptionID } = useDialogContentContext();
  const [local, others] = splitProps(props, ["class", "classList"]);
  const id = createUniqueId();

  onMount(() => setDescriptionID(id));

  return (
    <p
      id={id}
      data-slot="sheet-description"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...others}
    />
  );
}
