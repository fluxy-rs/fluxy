import type { ParentProps } from "solid-js";
import type { ContextMenuChangeEventDetails, ContextMenuOrientation } from "../context-menu.types";

/**
 * ContextMenuSub props,对齐 Base UI `ContextMenu.SubmenuRoot`。
 * 不渲染自己的 DOM,只作为一级子菜单的状态容器。
 */
export interface ContextMenuSubProps extends ParentProps {
  /** 受控打开状态;不传则内部自管理 */
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean, eventDetails: ContextMenuChangeEventDetails) => void;
  disabled?: boolean;
  /** 子菜单内按 Escape 时,是否连同父菜单一起关闭,默认 false */
  closeParentOnEsc?: boolean;
  loopFocus?: boolean;
  orientation?: ContextMenuOrientation;
  highlightItemOnHover?: boolean;
}
