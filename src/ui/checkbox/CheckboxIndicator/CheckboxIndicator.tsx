import { clsx } from "../../utils";
import { classes } from "./CheckboxIndicator.styles";
import { Check } from "lucide-solid";
import { Show } from "solid-js";

export const CheckboxIndicator = (props: { checked: boolean }) => {
  return (
    <Show when={props.checked}>
      <span data-slot="checkbox-indicator" data-checked={props.checked} class={clsx(classes)}>
        <Check />
      </span>
    </Show>
  );
};
