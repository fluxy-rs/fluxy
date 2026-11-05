import type { Middleware } from "../types";
import { getSide, isVerticalSide } from "../core/placement";

export interface ArrowOptions {
  element: Element | undefined | (() => Element | undefined);
  padding?: number;
}

export interface ArrowData {
  x?: number;
  y?: number;
  side: ReturnType<typeof getSide>;
}

export function arrow(options: ArrowOptions): Middleware {
  return {
    name: "arrow",
    fn(state) {
      const el = typeof options.element === "function" ? options.element() : options.element;
      const side = getSide(state.placement);
      if (!el) return { data: { side } };

      const { rects, x, y } = state;
      const padding = options.padding ?? 4;
      const vertical = isVerticalSide(side);

      const arrowRect = el.getBoundingClientRect();
      const arrowWidth = arrowRect.width || 0;
      const arrowHeight = arrowRect.height || 0;

      const freshFloatingRect = state.elements.floating.getBoundingClientRect();
      const axisLen = vertical ? freshFloatingRect.width : freshFloatingRect.height;
      const arrowLen = vertical ? arrowWidth : arrowHeight;

      const refCenter = vertical
        ? rects.reference.x + rects.reference.width / 2 - x
        : rects.reference.y + rects.reference.height / 2 - y;

      let pos = refCenter - arrowLen / 2;
      const min = padding;
      const max = axisLen - arrowLen - padding;
      pos = Math.min(Math.max(pos, min), Math.max(min, max));

      const data: ArrowData = vertical ? { x: pos, side } : { y: pos, side };
      return { data };
    },
  };
}
