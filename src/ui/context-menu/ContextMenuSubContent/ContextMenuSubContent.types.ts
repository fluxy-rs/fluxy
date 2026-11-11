import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";
import type { ContextMenuAlign, ContextMenuSide } from "../context-menu.types";

/**
 * ContextMenuSubContent props:与 `ContextMenuContent` 相同
 * (shadcn 里 SubContent 就是复用 Content,只是 data-slot / 样式略有差异)。
 */
export interface ContextMenuSubContentProps extends BaseProps, ParentProps {
  side?: ContextMenuSide;
  align?: ContextMenuAlign;
  sideOffset?: number;
  alignOffset?: number;
  collisionPadding?: number;
  /** 与 Content 保持 API 一致;子菜单的焦点回收统一由根菜单的 finalFocus 决定 */
  finalFocus?: boolean;
  [key: string]: any;
}
