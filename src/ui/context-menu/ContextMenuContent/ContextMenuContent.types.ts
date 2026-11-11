import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";
import type { ContextMenuAlign, ContextMenuSide } from "../context-menu.types";

/**
 * ContextMenuContent props:对齐 shadcn 的 `ContextMenuContent`
 * (Base UI 的 `Popup` 属性 + `Positioner` 的 align/alignOffset/side/sideOffset)。
 *
 * 位置相关 prop 的默认值与 shadcn 保持一致:side="right"、align="start"、
 * alignOffset=4、sideOffset=0;`collisionPadding` 对齐 Base UI 默认值 5。
 */
export interface ContextMenuContentProps extends BaseProps, ParentProps {
  /** 贴锚点的哪一侧,默认 'right';支持 Base UI 的逻辑方向 inline-start/inline-end */
  side?: ContextMenuSide;
  /** 沿边对齐方式,默认 'start' */
  align?: ContextMenuAlign;
  /** 与锚点之间的间距(像素),默认 0 */
  sideOffset?: number;
  /** 沿对齐轴的偏移(像素),默认 4 */
  alignOffset?: number;
  /** 距视口边缘的最小间距(像素),默认 5 */
  collisionPadding?: number;
  /** 关闭后是否把焦点还给触发器,默认 true */
  finalFocus?: boolean;
  /** 其余原生 div 属性原样透传到 popup 元素 */
  [key: string]: any;
}
