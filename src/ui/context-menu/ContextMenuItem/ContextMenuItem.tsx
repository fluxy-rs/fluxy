import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import { useContextMenuEntry } from "../context-menu.entry";
import { contextMenuItemClass } from "../context-menu.styles";
import type { ContextMenuItemProps } from "./ContextMenuItem.types";

/**
 * 一个可点击的菜单项。点击后默认关闭菜单;在 `onClick` 里调用
 * `event.preventDefault()` 可以阻止关闭(比如需要保持菜单打开的场景)。
 */
export function ContextMenuItem(props: ContextMenuItemProps) {
  const [local, rest] = splitProps(props, [
    "class",
    "children",
    "inset",
    "variant",
    "disabled",
    "label",
    "closeOnClick",
    "onClick",
  ]);

  let element: HTMLDivElement | undefined;

  const entry = useContextMenuEntry({
    component: "ContextMenuItem",
    element: () => element,
    disabled: () => !!local.disabled,
    label: () => local.label,
  });

  const handleClick = (e: MouseEvent) => {
    if (local.disabled) return;
    local.onClick?.(e);
    if (e.defaultPrevented) return;
    if (local.closeOnClick !== false) {
      entry.popup.closeAll("item-press", e);
    }
  };

  return (
    <div
      ref={(el) => {
        element = el;
      }}
      id={entry.id}
      role="menuitem"
      tabIndex={-1}
      data-slot="context-menu-item"
      data-inset={local.inset ? "" : undefined}
      data-variant={local.variant ?? "default"}
      data-highlighted={entry.isActive() ? "" : undefined}
      data-disabled={local.disabled ? "" : undefined}
      aria-disabled={local.disabled ? "true" : undefined}
      onClick={handleClick}
      onPointerEnter={() => entry.highlight()}
      onPointerMove={() => entry.highlight()}
      class={clsx(contextMenuItemClass, local.class)}
      {...rest}
    >
      {local.children}
    </div>
  );
}
