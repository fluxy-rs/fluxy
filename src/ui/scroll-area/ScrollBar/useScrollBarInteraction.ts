/**
 *
 */

import { type Accessor, onCleanup } from "solid-js";
import {
  applyScrollPos,
  isVertical,
  keyboardScrollDelta,
  maxScrollOf,
  type Orientation,
  scrollFromKeyboard,
  scrollFromTrackClick,
  scrollPosOf,
} from "./ScrollBar.utils";
import { createThumbDrag } from "./scroll-bar.thumb-drag";

interface Options {
  track: () => HTMLDivElement | undefined;
  viewport: () => HTMLDivElement | undefined;
  orientation: Accessor<Orientation>;
  onDragChange: (dragging: boolean) => void;
}

export interface ScrollBarInteraction {
  onThumbPointerDown: (event: PointerEvent) => void;
  onTrackPointerDown: (event: PointerEvent) => void;
  onKeyDown: (event: KeyboardEvent) => void;
}

export function useScrollBarInteraction({
  track,
  viewport,
  orientation,
  onDragChange,
}: Options): ScrollBarInteraction {
  const thumbDrag = createThumbDrag({
    track,
    viewport,
    orientation,
    onDragChange,
  });

  onCleanup(thumbDrag.dispose);

  const onThumbPointerDown = (event: PointerEvent) => thumbDrag.start(event);

  /**
   *
   */
  const onTrackPointerDown = (event: PointerEvent) => {
    const vp = viewport();
    const el = track();
    if (!vp || !el) return;

    const axis = orientation();
    const target = scrollFromTrackClick(
      axis,
      el.getBoundingClientRect(),
      event,
      maxScrollOf(axis, vp),
    );
    vp.scrollTo({
      [isVertical(axis) ? "top" : "left"]: target,
      behavior: "smooth",
    });
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const vp = viewport();
    if (!vp) return;

    const axis = orientation();
    const pageSize = isVertical(axis) ? vp.clientHeight : vp.clientWidth;
    const delta = keyboardScrollDelta(event.key, pageSize);
    if (delta === undefined) return;

    event.preventDefault();
    const scrollSize = isVertical(axis) ? vp.scrollHeight : vp.scrollWidth;
    const max = maxScrollOf(axis, vp);
    applyScrollPos(axis, vp, scrollFromKeyboard(scrollPosOf(axis, vp), delta, max, scrollSize));
  };

  return { onThumbPointerDown, onTrackPointerDown, onKeyDown };
}
