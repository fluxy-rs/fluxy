import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

export type ContextMenuItemVariant = "default" | "destructive";

/**
 * ContextMenuItem props,对齐 Base UI `ContextMenu.Item`:
 * `label` / `closeOnClick` / `nativeButton` / `disabled` / `onClick`。
 * `inset` 与 `variant` 是 shadcn 在包装层额外加的样式属性。
 */
export interface ContextMenuItemProps extends BaseProps, ParentProps {
  /** 左侧对齐到带图标项的文字位置(shadcn 的 inset 样式) */
  inset?: boolean;
  /** 视觉风格,`destructive` 用于危险操作 */
  variant?: ContextMenuItemVariant;
  disabled?: boolean;
  /** 键盘字符导航匹配用的文本,默认取子节点文本 */
  label?: string;
  /** 点击后是否关闭菜单,默认 true */
  closeOnClick?: boolean;
  /** 在 onClick 里调用 event.preventDefault() 可阻止本次点击关闭菜单 */
  onClick?: (event: MouseEvent) => void;
  [key: string]: any;
}
