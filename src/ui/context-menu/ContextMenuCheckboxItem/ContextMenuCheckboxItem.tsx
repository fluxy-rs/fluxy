import { createSignal, Show, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { Check } from "lucide-solid";
import { useContextMenuEntry } from "../context-menu.entry";
import { contextMenuCheckableItemClass, contextMenuIndicatorClass } from "../context-menu.styles";
import { createChangeEventDetails } from "../context-menu.utils";
import type { ContextMenuCheckboxItemProps } from "./ContextMenuCheckboxItem.types";

/**
 * 可勾选的菜单项(toggle),角色为 `menuitemcheckbox`。
 * 与 Base UI 一致:点击后默认**不**关闭菜单,便于连续切换多个选项。
 */
export function ContextMenuCheckboxItem(props: ContextMenuCheckboxItemProps) {
  const [local, rest] = splitProps(props, [
    "class",
    "children",
    "checked",
    "defaultChecked",
    "onCheckedChange",
    "disabled",
    "inset",
    "label",
    "closeOnClick",
    "onClick",
  ]);

  const [internalChecked, setInternalChecked] = createSignal(props.defaultChecked ?? false);
  const checked = () => (props.checked !== undefined ? props.checked : internalChecked());

  let element: HTMLDivElement | undefined;

  const entry = useContextMenuEntry({
    component: "ContextMenuCheckboxItem",
    element: () => element,
    disabled: () => !!local.disabled,
    label: () => local.label,
  });

  const handleClick = (e: MouseEvent) => {
    if (local.disabled) return;
    local.onClick?.(e);
    if (e.defaultPrevented) return;

    const next = !checked();
    if (props.checked === undefined) setInternalChecked(next);
    props.onCheckedChange?.(next, createChangeEventDetails("item-press", e, element));

    if (local.closeOnClick) entry.popup.closeAll("item-press", e);
  };

  return (
    <div
      ref={(el) => {
        element = el;
      }}
      id={entry.id}
      role="menuitemcheckbox"
      tabIndex={-1}
      aria-checked={checked()}
      data-slot="context-menu-checkbox-item"
      data-inset={local.inset ? "" : undefined}
      data-checked={checked() ? "" : undefined}
      data-unchecked={checked() ? undefined : ""}
      data-highlighted={entry.isActive() ? "" : undefined}
      data-disabled={local.disabled ? "" : undefined}
      aria-disabled={local.disabled ? "true" : undefined}
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
