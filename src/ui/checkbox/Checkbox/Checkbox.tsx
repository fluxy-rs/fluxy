import { createMemo, createSignal, createUniqueId, mergeProps, splitProps } from "solid-js";
import type { CheckboxProps } from "./Checkbox.types";
import { clsx } from "../../utils";
import { classes } from "./Checkbox.styles";
import { CheckboxIndicator } from "../CheckboxIndicator";

export const Checkbox = (props: CheckboxProps) => {
  const defaultID = createUniqueId();
  const merged = mergeProps(
    { id: defaultID, name: defaultID, defaultChecked: false } as const,
    props,
  );

  const [local, others] = splitProps(merged, [
    "name",
    "id",
    "checked",
    "defaultChecked",
    "onChange",
    "disabled",
    "class",
    "classList",
  ]);

  const [internalChecked, setInternalChecked] = createSignal(local.defaultChecked);

  const checked = createMemo(() => local.checked ?? internalChecked());

  let inputRef: HTMLInputElement | undefined;

  /**
   */
  const handleClick = (event: MouseEvent) => {
    if (local.disabled) return;
    const target = event.target as Element | null;
    const label = target?.closest("label") as HTMLLabelElement | null;
    if (label?.control === inputRef) {
      event.preventDefault();
    }
    inputRef?.click();
  };

  return (
    <>
      <span
        data-slot="checkbox"
        role="checkbox"
        tabIndex={0}
        aria-checked={checked()}
        aria-disabled={local.disabled}
        data-disabled={local.disabled}
        data-checked={checked()}
        aria-labelledby={local.id}
        class={clsx(classes, local.class)}
        classList={local.classList}
        onClick={(event) => handleClick(event)}
        {...others}
      >
        <CheckboxIndicator checked={checked()} />
      </span>
      <input
        ref={(el) => {
          inputRef = el;
        }}
        id={local.id}
        type="checkbox"
        checked={checked()}
        tabIndex={-1}
        aria-hidden="true"
        name={local.name}
        class="sr-only"
        disabled={local.disabled}
        onChange={(event) => {
          if (local.disabled) return;
          const next = event.currentTarget.checked;
          setInternalChecked(next);
          local.onChange?.(next);
        }}
      />
    </>
  );
};
