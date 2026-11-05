import type { Boundary, Middleware, MiddlewareState } from "../types";
import { getViewportBoundary } from "../utils/dom";

export interface SizeAvailableSpace {
  availableWidth: number;
  availableHeight: number;
}

export interface SizeOptions {
  padding?: number;
  boundary?: Boundary;
  apply?: (space: SizeAvailableSpace, state: MiddlewareState) => void;
}

export function size(options: SizeOptions = {}): Middleware {
  return {
    name: "size",
    fn(state) {
      const { x, y, strategy } = state;
      const { padding = 0 } = options;
      const boundary = options.boundary ?? getViewportBoundary(strategy, padding);

      const availableWidth = boundary.x + boundary.width - x;
      const availableHeight = boundary.y + boundary.height - y;

      options.apply?.({ availableWidth, availableHeight }, state);

      return { data: { availableWidth, availableHeight } };
    },
  };
}
