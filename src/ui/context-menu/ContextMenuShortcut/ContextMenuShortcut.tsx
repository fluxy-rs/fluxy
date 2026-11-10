import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import { contextMenuShortcutClass } from "../context-menu.styles";
import type { ContextMenuShortcutProps } from "./ContextMenuShortcut.types";

/**
 * 菜单项右侧的快捷键提示文本,例如 `<ContextMenuShortcut>⌘K</ContextMenuShortcut>`。
 * 本身不参与交互,只是在菜单项被高亮时跟随变色(靠 group-data 选择器)。
 */
export function ContextMenuShortcut(props: ContextMenuShortcutProps) {
  const [local, rest] = splitProps(props, ["class", "children"]);

  return (
    <span
      data-slot="context-menu-shortcut"
      class={clsx(contextMenuShortcutClass, local.class)}
      {...rest}
    >
      {local.children}
    </span>
  );
}
