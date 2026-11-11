import { Show, splitProps } from "solid-js";
import { Check } from "lucide-solid";
import { clsx } from "../../utils";
import { useContextMenuRadioGroup } from "../context-menu.context";
import { useContextMenuEntry } from "../context-menu.entry";
import { contextMenuCheckableItemClass, contextMenuIndicatorClass } from "../context-menu.styles";
import type { ContextMenuRadioItemProps } from "./ContextMenuRadioItem.types";

/**
 * RadioGroup 中的单选项,角色为 `menuitemradio`。
 * 选中状态完全由所属 `<ContextMenuRadioGroup>` 的值决定。
 */
export function ContextMenuRadioItem(props: ContextMenuRadioItemProps) {
  const group = useContextMenuRadioGroup("ContextMenuRadioItem");
  const [local, rest] = splitProps(props, [
    "class",
    "children",
    "value",
    "disabled",
    "inset",
    "label",
    "closeOnClick",
    "onClick",
  ]);

  const disabled = () => !!local.disabled || group.disabled();
  const checked = () => group.value() === props.value;

  let element: HTMLDivElement | undefined;

  const entry = useContextMenuEntry({
    component: "ContextMenuRadioItem",
    element: () => element,
    disabled,
    label: () => local.label,
  });

  const handleClick = (e: MouseEvent) => {
    if (disabled()) return;
    local.onClick?.(e);
    if (e.defaultPrevented) return;

    group.setValue(props.value, e);

    if (local.closeOnClick) entry.popup.closeAll("item-press", e);
  };

  return (
    <div
      ref={(el) => {
        element = el;
      }}
      id={entry.id}
      role="menuitemradio"
      tabIndex={-1}
      aria-checked={checked()}
      data-slot="context-menu-radio-item"
      data-inset={local.inset ? "" : undefined}
      data-checked={checked() ? "" : undefined}
      data-unchecked={checked() ? undefined : ""}
      data-highlighted={entry.isActive() ? "" : undefined}
      data-disabled={disabled() ? "" : undefined}
      aria-disabled={disabled() ? "true" : undefined}
      onClick={handleClick}
      onPointerEnter={() => entry.highlight()}
      onPointerMove={() => entry.highlight()}
      class={clsx(contextMenuCheckableItemClass, local.class)}
      {...rest}
    >
      <span class={contextMenuIndicatorClass}>
        <Show when={checked()}>
          <Check />
        </Show>
      </span>
      {local.children}
    </div>
  );
}
