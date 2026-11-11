import type { ParentProps } from "solid-js";
import type { ContextMenuChangeEventDetails, ContextMenuOrientation } from "../context-menu.types";

/**
 * ContextMenu 根组件 props,对齐 Base UI `ContextMenu.Root`。
 * 根组件不渲染任何 DOM,只负责状态与 context。
 */
export interface ContextMenuProps extends ParentProps {
  /** 受控打开状态;不传则内部自管理 */
  open?: boolean;
  /** 非受控模式下的初始打开状态 */
  defaultOpen?: boolean;
  /** 打开状态变化回调,第二个参数对齐 Base UI 的 ChangeEventDetails */
  onOpenChange?: (open: boolean, eventDetails: ContextMenuChangeEventDetails) => void;
  /** 是否忽略用户交互 */
  disabled?: boolean;
  /** 方向键到达两端时是否回到另一端,默认 true */
  loopFocus?: boolean;
  /** 鼠标悬停是否高亮菜单项,默认 true */
  highlightItemOnHover?: boolean;
  /** 菜单排列方向,默认 'vertical' */
  orientation?: ContextMenuOrientation;
  /**
   * 打开时是否进入模态状态,默认 true(对齐 Base UI):
   * 锁定页面滚动并拦截菜单之外的滚轮/触摸滚动,避免浮窗锚点随页面滚动而"飘移"。
   * 子菜单不受该 prop 影响,只有根菜单会加锁。
   */
  modal?: boolean;
}
