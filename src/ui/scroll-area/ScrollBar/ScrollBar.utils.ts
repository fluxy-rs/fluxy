/**
 *
 */

import type { ScrollMetrics } from "../ScrollArea/ScrollArea.types";

export const MIN_THUMB_PX = 20;
export const TRACK_PADDING_PX = 2;
export const KEYBOARD_STEP_PX = 40;

export type Orientation = "horizontal" | "vertical";

export function isVertical(orientation: Orientation): boolean {
  return orientation === "vertical";
}

/**
 */
export function computeThumbStyle(
  orientation: Orientation,
  metrics: ScrollMetrics,
  trackSize: number,
): { height?: string; width?: string; transform: string } {
  const trackInnerSize = trackSize - TRACK_PADDING_PX;
  const thumbSize = Math.max(trackInnerSize * metrics.thumbRatio, MIN_THUMB_PX);
  const maxOffset = Math.max(0, trackInnerSize - thumbSize);
  const offset = maxOffset * metrics.thumbOffset;

  return isVertical(orientation)
    ? { height: `${thumbSize}px`, transform: `translateY(${offset}px)` }
    : { width: `${thumbSize}px`, transform: `translateX(${offset}px)` };
}

export function computeAriaValueNow(metrics: ScrollMetrics): number {
  return Math.round(metrics.thumbOffset * 100);
}

export function pointerCoord(orientation: Orientation, event: PointerEvent): number {
  return isVertical(orientation) ? event.clientY : event.clientX;
}

export function maxScrollOf(orientation: Orientation, viewport: HTMLElement): number {
  return isVertical(orientation)
    ? viewport.scrollHeight - viewport.clientHeight
    : viewport.scrollWidth - viewport.clientWidth;
}

export function scrollPosOf(orientation: Orientation, viewport: HTMLElement): number {
  return isVertical(orientation) ? viewport.scrollTop : viewport.scrollLeft;
}

export function applyScrollPos(
  orientation: Orientation,
  viewport: HTMLElement,
  value: number,
): void {
  if (isVertical(orientation)) {
    viewport.scrollTop = value;
  } else {
    viewport.scrollLeft = value;
  }
}

/**
 */
export function thumbDragRatio(maxScroll: number, trackSize: number, thumbSize: number): number {
  const denominator = trackSize - TRACK_PADDING_PX - thumbSize;
  return denominator === 0 ? 1 : maxScroll / denominator;
}

export function scrollFromDrag(
  startScroll: number,
  deltaPx: number,
  ratio: number,
  maxScroll: number,
): number {
  return Math.max(0, Math.min(maxScroll, startScroll + deltaPx * ratio));
}

/**
 */
export function scrollFromTrackClick(
  orientation: Orientation,
  rect: { top: number; left: number; height: number; width: number },
  event: PointerEvent,
  maxScroll: number,
): number {
  const size = isVertical(orientation) ? rect.height : rect.width;
  if (size === 0) return 0;
  const offset = isVertical(orientation) ? event.clientY - rect.top : event.clientX - rect.left;
  return (offset / size) * maxScroll;
}

export function keyboardScrollDelta(key: string, pageSize: number): number | undefined {
  const keyMap: Record<string, number> = {
    ArrowDown: KEYBOARD_STEP_PX,
    ArrowRight: KEYBOARD_STEP_PX,
    ArrowUp: -KEYBOARD_STEP_PX,
    ArrowLeft: -KEYBOARD_STEP_PX,
    PageDown: pageSize,
    PageUp: -pageSize,
    Home: -Infinity,
    End: Infinity,
  };
  return keyMap[key];
}

/**
 */
export function scrollFromKeyboard(
  current: number,
  delta: number,
  max: number,
  scrollSize: number,
): number {
  if (delta === Infinity) return scrollSize;
  if (delta === -Infinity) return 0;
  return Math.max(0, Math.min(max, current + delta));
}

export function isScrollBarVisible(
  hovering: boolean,
  dragging: "horizontal" | "vertical" | null,
  orientation: Orientation,
  active: boolean,
): boolean {
  return hovering || dragging === orientation || active;
}
