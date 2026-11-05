import type { Accessor } from "solid-js";
import {
  applyScrollPos,
  isVertical,
  maxScrollOf,
  type Orientation,
  pointerCoord,
  scrollFromDrag,
  scrollPosOf,
  thumbDragRatio,
} from "./ScrollBar.utils";

export interface ThumbDragContext {
  track: () => HTMLDivElement | undefined;
  viewport: () => HTMLDivElement | undefined;
  orientation: Accessor<Orientation>;
  onDragChange: (dragging: boolean) => void;
}

export interface ThumbDragSession {
  start: (event: PointerEvent) => void;
  dispose: () => void;
}

/**
 *
 */
export function createThumbDrag(ctx: ThumbDragContext): ThumbDragSession {
  let detach: (() => void) | null = null;

  const start = (event: PointerEvent) => {
    event.preventDefault();
    event.stopPropagation();

    const vp = ctx.viewport();
    const track = ctx.track();
    const thumb = event.currentTarget as HTMLDivElement | null;
    if (!vp || !track || !thumb) return;

    const axis = ctx.orientation();
    const vertical = isVertical(axis);
    const startPointer = pointerCoord(axis, event);
    const startScroll = scrollPosOf(axis, vp);
    const maxScroll = maxScrollOf(axis, vp);
    const trackSize = vertical ? track.clientHeight : track.clientWidth;
    const thumbSize = vertical ? thumb.clientHeight : thumb.clientWidth;
    const ratio = thumbDragRatio(maxScroll, trackSize, thumbSize);

    const onMove = (moveEvent: PointerEvent) => {
      const delta = pointerCoord(axis, moveEvent) - startPointer;
      applyScrollPos(axis, vp, scrollFromDrag(startScroll, delta, ratio, maxScroll));
    };

    const onUp = () => detach?.();

    detach = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      detach = null;
      ctx.onDragChange(false);
    };

    ctx.onDragChange(true);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return {
    start,
    dispose() {
      detach?.();
    },
  };
}
