import type { Boundary, Middleware } from "../types";
import { getViewportBoundary, getRectRelativeTo } from "../utils/dom";

export interface HideOptions {
  padding?: number;
  boundary?: Boundary;

  strategy?: "referenceHidden" | "escaped";
}

export interface HideData {
  referenceHidden?: boolean;
  escaped?: boolean;
}

export function hide(options: HideOptions = {}): Middleware {
  return {
    name: "hide",
    fn(state) {
      const { x, y, rects, strategy: positionStrategy, elements } = state;
      const { padding = 0, strategy = "referenceHidden" } = options;
      const boundary = options.boundary ?? getViewportBoundary(positionStrategy, padding);

      const data: HideData = {};

      if (strategy === "referenceHidden") {
        const refRect = getRectRelativeTo(elements.reference, positionStrategy);
        const referenceHidden =
          refRect.x + refRect.width <= boundary.x ||
          refRect.y + refRect.height <= boundary.y ||
          refRect.x >= boundary.x + boundary.width ||
          refRect.y >= boundary.y + boundary.height;
        data.referenceHidden = referenceHidden;
      }

      if (strategy === "escaped") {
        const escaped =
          x + rects.floating.width <= boundary.x ||
          y + rects.floating.height <= boundary.y ||
          x >= boundary.x + boundary.width ||
          y >= boundary.y + boundary.height;
        data.escaped = escaped;
      }

      return { data };
    },
  };
}
