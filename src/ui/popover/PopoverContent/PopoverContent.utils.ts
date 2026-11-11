import {
  createFloatingMiddleware,
  getTransformOrigin,
  toPlacement,
  type Middleware,
} from "../../lib";
import type { PopoverSide } from "./PopoverContent.types";

// 定位相关的纯函数已抽到 ~/lib/positioner/utils/floating,这里保留原导出名。

/** side + align 转成核心库的 Placement(逻辑方向按 LTR 映射) */
export function toPopoverPlacement(side: PopoverSide, align: "start" | "end" | "center") {
  return toPlacement(side, align);
}

/** 根据最终 placement 计算缩放动画的 transform-origin */
export const getPopoverTransformOrigin = getTransformOrigin;

export interface BuildPopoverMiddlewareOptions {
  sideOffset: number;
  alignOffset: number;
  collisionPadding: number;
}

/** Popover 定位管线:offset → flip → shift → hide → containingBlockOffset */
export function createPopoverMiddleware(options: BuildPopoverMiddlewareOptions): Middleware[] {
  return createFloatingMiddleware(options);
}
