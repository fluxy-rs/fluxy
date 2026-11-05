import { createMemo, createSignal, mergeProps, splitProps } from "solid-js";
import type { SwitchProps } from "./Switch.types";
import { clsx } from "../cn";
import { classes } from "./Switch.styles";

export const Switch = (props: SwitchProps) => {
  const merged = mergeProps({ size: "md" } as const, props);

  const [local, others] = splitProps(merged, [
    "id",
    "checked",
    "defaultChecked",
    "onCheckedChange",
    "size",
    "disabled",
    "class",
    "classList",
  ]);

  const [internalChecked, setInternalChecked] = createSignal(local.defaultChecked ?? false);

  // Copied in from neut/ui; upstream wires clicks only.
  //
  // A span with role="switch" and tabindex is a real control as far as ARIA is
  // concerned, and ARIA says Space and Enter activate it. Without this, a
  // keyboard user can reach the switch and then do nothing with it.
  const handleKeyDown = (event: KeyboardEvent) => {
    if (local.disabled) return;
    if (event.key !== " " && event.key !== "Enter") return;
    // Space scrolls the page by default, and Enter may submit a form the
    // switch happens to sit in.
    event.preventDefault();
    handleClick();
  };

  const handleClick = () => {
    if (local.disabled) return;

    if (local.checked !== undefined) {
      setInternalChecked(local.checked);
      local.onCheckedChange?.(!local.checked);
      return;
    }

    const newChecked = !internalChecked();
    setInternalChecked(newChecked);
    local.onCheckedChange?.(newChecked);
  };

  const checked = createMemo(() =>
    local.checked !== undefined ? local.checked : internalChecked(),
  );

  return (
    <span
      data-slot="switch"
      data-size={local.size}
      data-checked={checked()}
      data-disabled={local.disabled}
      role="switch"
      tabIndex={0}
      aria-checked={checked()}
      aria-disabled={local.disabled}
      class={clsx(classes.switch, local.class)}
      classList={local.classList}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      {...others}
    >
      <span data-slot="switch-thumb" class={classes.thumb} data-checked={checked()} />
      <input
        id={local.id}
        tabIndex={-1}
        aria-hidden="true"
        type="checkbox"
        checked={checked()}
        class="sr-only"
        disabled={local.disabled}
      />
    </span>
  );
};
