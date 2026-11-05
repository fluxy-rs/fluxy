import { getAlignment, getSide } from "../core/placement";
import { arrow, containingBlockOffset, flip, hide, offset, shift } from "../middleware";
import type { Alignment, Middleware, Placement, Side } from "../types";

export type LogicalSide = Side | "inline-start" | "inline-end";

export function toPhysicalSide(side: LogicalSide, dir?: string): Side {
  if (side === "inline-start") return dir === "rtl" ? "right" : "left";
  if (side === "inline-end") return dir === "rtl" ? "left" : "right";
  return side;
}

export function toPlacement(
  side: LogicalSide,
  align: Alignment | "center",
  dir?: string,
): Placement {
  const physical = toPhysicalSide(side, dir);
  return align === "center" ? physical : (`${physical}-${align}` as Placement);
}

export function getTransformOrigin(placement: Placement): string {
  const side = getSide(placement);
  const align = getAlignment(placement);
  const cross = align === "start" ? "0%" : align === "end" ? "100%" : "50%";
  switch (side) {
    case "top":
      return `${cross} 100%`;
    case "bottom":
      return `${cross} 0%`;
    case "left":
      return `100% ${cross}`;
    case "right":
      return `0% ${cross}`;
  }
}

export interface FloatingMiddlewareOptions {
  sideOffset: number;
  alignOffset?: number;
  collisionPadding?: number;
  arrowElement?: () => Element | undefined;
  arrowPadding?: number;
}

/**
 * offset → flip → shift → [arrow] → hide → containingBlockOffset。
 */
export function createFloatingMiddleware(options: FloatingMiddlewareOptions): Middleware[] {
  const middleware: Middleware[] = [
    offset({
      mainAxis: options.sideOffset,
      crossAxis: options.alignOffset ?? 0,
    }),
    flip(),
    shift({ padding: options.collisionPadding ?? 8 }),
  ];
  if (options.arrowElement) {
    middleware.push(
      arrow({
        element: options.arrowElement,
        padding: options.arrowPadding ?? 6,
      }),
    );
  }
  middleware.push(hide(), containingBlockOffset());
  return middleware;
}
