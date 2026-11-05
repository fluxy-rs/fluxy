import type { Boundary, Middleware, Placement, Side } from "../types";
import { getViewportBoundary } from "../utils/dom";
import { computeCoordsFromPlacement, getOppositePlacement, getSide } from "../core/placement";

export interface FlipOptions {
  padding?: number;
  boundary?: Boundary;
  fallbackPlacements?: Placement[];
}

interface SideOverflow {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

function getSideOverflow(
  x: number,
  y: number,
  width: number,
  height: number,
  boundary: Boundary,
): SideOverflow {
  return {
    top: boundary.y - y,
    left: boundary.x - x,
    right: x + width - (boundary.x + boundary.width),
    bottom: y + height - (boundary.y + boundary.height),
  };
}

function getMainAxisOverflow(side: Side, overflow: SideOverflow): number {
  switch (side) {
    case "top":
      return overflow.top;
    case "bottom":
      return overflow.bottom;
    case "left":
      return overflow.left;
    case "right":
      return overflow.right;
  }
}

export function flip(options: FlipOptions = {}): Middleware {
  return {
    name: "flip",
    fn(state) {
      const { x, y, rects, placement, strategy, middlewareData, initialPlacement } = state;

      if (middlewareData.flip?.flipped) return {};

      const { padding = 0 } = options;
      const boundary = options.boundary ?? getViewportBoundary(strategy, padding);

      const currentOverflow = getSideOverflow(
        x,
        y,
        rects.floating.width,
        rects.floating.height,
        boundary,
      );
      const currentMainOverflow = getMainAxisOverflow(getSide(placement), currentOverflow);

      if (currentMainOverflow <= 0) return {};

      const candidates = options.fallbackPlacements ?? [getOppositePlacement(initialPlacement)];

      let best: { placement: Placement; overflow: number } | null = null;

      for (const candidate of candidates) {
        if (candidate === placement) continue;

        const candidateCoords = computeCoordsFromPlacement(rects, candidate);
        const candidateOverflow = getSideOverflow(
          candidateCoords.x,
          candidateCoords.y,
          rects.floating.width,
          rects.floating.height,
          boundary,
        );
        const candidateMainOverflow = getMainAxisOverflow(getSide(candidate), candidateOverflow);

        if (candidateMainOverflow <= 0) {
          best = { placement: candidate, overflow: candidateMainOverflow };
          break;
        }
        if (!best || candidateMainOverflow < best.overflow) {
          best = { placement: candidate, overflow: candidateMainOverflow };
        }
      }

      if (!best || best.overflow >= currentMainOverflow) return {};

      return {
        reset: { placement: best.placement },
        data: { flipped: true },
      };
    },
  };
}
