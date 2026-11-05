import { splitProps, type ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { clsx } from "../../utils";
import type { AttachmentTriggerProps } from "./AttachmentTrigger.types";

/**
 */
export const AttachmentTrigger = <T extends ValidComponent = "button">(
  props: AttachmentTriggerProps<T>,
) => {
  const [local, others] = splitProps(props as AttachmentTriggerProps, [
    "component",
    "class",
    "classList",
  ]);
  const component = (local.component as ValidComponent) ?? "button";
  const type =
    component === "button"
      ? ((props as { type?: "button" | "submit" | "reset" }).type ?? "button")
      : undefined;

  return (
    <Dynamic
      {...others}
      component={component}
      type={type}
      data-slot="attachment-trigger"
      class={clsx("absolute inset-0 z-10 outline-none", local.class)}
      classList={local.classList}
    />
  );
};
