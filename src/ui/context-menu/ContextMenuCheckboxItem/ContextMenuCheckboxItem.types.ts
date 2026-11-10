import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";
import type { ContextMenuChangeEventDetails } from "../context-menu.types";

/** ContextMenuCheckboxItem props,对齐 Base UI `ContextMenu.CheckboxItem` */
export interface ContextMenuCheckboxItemProps extends BaseProps, ParentProps {
  /** 受控勾选状态;不传则内部自管理 */
  checked?: boolean;
  /** 非受控模式下的初始勾选状态 */
  defaultChecked?: boolean;
  onCheckedChange?: (checked: boolean, eventDetails: ContextMenuChangeEventDetails) => void;
  disabled?: boolean;
  /** 左侧对齐到带图标项的文字位置 */
  inset?: boolean;
  /** 键盘字符导航匹配用的文本 */
  label?: string;
  /** 点击后是否关闭菜单,默认 false(与 Base UI 一致) */
  closeOnClick?: boolean;
  onClick?: (event: MouseEvent) => void;
  [key: string]: any;
}
