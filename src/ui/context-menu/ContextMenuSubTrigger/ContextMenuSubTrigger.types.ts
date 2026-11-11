import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

/**
 * ContextMenuSubTrigger props,对齐 Base UI `ContextMenu.SubmenuTrigger`。
 * 渲染一个带右侧箭头图标的菜单项,悬停/点击/右方向键都会打开子菜单。
 */
export interface ContextMenuSubTriggerProps extends BaseProps, ParentProps {
  /** 左侧对齐到带图标项的文字位置 */
  inset?: boolean;
  disabled?: boolean;
  /** 键盘字符导航匹配用的文本 */
  label?: string;
  /** 悬停是否打开子菜单,默认 true */
  openOnHover?: boolean;
  /** 悬停多久后打开(毫秒),默认 100 */
  delay?: number;
  /** 移出多久后关闭(毫秒),默认 0;实现里会保留 100ms 的穿越容忍 */
  closeDelay?: number;
  onClick?: (event: MouseEvent) => void;
  [key: string]: any;
}
