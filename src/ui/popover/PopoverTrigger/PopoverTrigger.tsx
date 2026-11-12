import type { ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Button } from "../../button";
import { mergeRefs } from "../../utils";
import { usePopoverTrigger } from "./usePopoverTrigger";
import type { PopoverTriggerProps } from "./PopoverTrigger.types";

export const PopoverTrigger = <T extends ValidComponent = typeof Button<"button">>(
  props: PopoverTriggerProps<T>,
) => {
  const { ctx, isDisabled, attachListeners } = usePopoverTrigger(() => props);

  return (
    <Dynamic
      {...props}
      component={(props.component as ValidComponent) ?? Button<"button">}
      ref={mergeRefs(ctx.setReference, props.ref, attachListeners)}
      disabled={isDisabled()}
      data-slot="popover-trigger"
      aria-haspopup="dialog"
      aria-expanded={ctx.open()}
      aria-controls={ctx.open() ? ctx.contentId : undefined}
      data-state={ctx.open() ? "open" : "closed"}
    />
  );
};
