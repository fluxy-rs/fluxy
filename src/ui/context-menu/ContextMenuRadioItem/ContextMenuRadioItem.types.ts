import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

/** ContextMenuRadioItem props,对齐 Base UI `ContextMenu.RadioItem` */
export interface ContextMenuRadioItemProps extends BaseProps, ParentProps {
  /** 该项的值;选中时写入所属 RadioGroup */
  value: any;
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
