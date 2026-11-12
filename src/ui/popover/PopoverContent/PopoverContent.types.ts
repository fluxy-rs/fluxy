import type { ParentProps } from "solid-js";
import type { Alignment, Side } from "../../lib";
import type { BaseProps } from "../../types";

export type PopoverSide = Side | "inline-start" | "inline-end";

export interface PopoverContentProps extends BaseProps, ParentProps {
  /** 贴 trigger 的哪一侧，默认 'bottom'。额外支持 Base UI 的 logical side。 */
  side?: PopoverSide;
  /** 沿边对齐方式，默认 'center' */
  align?: Alignment | "center";
  /** 沿主轴（side 指向的方向）与 trigger 之间的间距（像素），默认 4 */
  sideOffset?: number;
  /** 沿交叉轴（对齐方向）的偏移（像素），默认 0 */
  alignOffset?: number;
  /** 距视口边缘的最小间距（像素），默认 8 */
  collisionPadding?: number;
}
