import type { Boundary, Middleware } from "../types";
import { getViewportBoundary } from "../utils/dom";

export interface ShiftOptions {
  padding?: number;
  boundary?: Boundary;
  mainAxis?: boolean;
  crossAxis?: boolean;
}

export function shift(options: ShiftOptions = {}): Middleware {
  return {
    name: "shift",
    fn(state) {
      const { x, y, rects, strategy } = state;
      const { padding = 0, mainAxis = true, crossAxis = true } = options;
      const boundary = options.boundary ?? getViewportBoundary(strategy, padding);

      let nextX = x;
      let nextY = y;

      if (crossAxis) {
        const minX = boundary.x;
        const maxX = boundary.x + boundary.width - rects.floating.width;
        nextX = Math.min(Math.max(x, minX), Math.max(minX, maxX));
      }

      if (mainAxis) {
        const minY = boundary.y;
        const maxY = boundary.y + boundary.height - rects.floating.height;
        nextY = Math.min(Math.max(y, minY), Math.max(minY, maxY));
      }

      return {
        x: nextX,
        y: nextY,
        data: { x: nextX - x, y: nextY - y },
      };
    },
  };
}
