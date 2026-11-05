/**
 *
 */

import type { MessageScrollerScrollOptions } from "./message-scroller.types";
import { type PaddingBox, paddingBox } from "./message-scroller.measure";

export interface TargetTopInput {
  itemTop: number;
  itemHeight: number;
  viewportHeight: number;
  scrollTop: number;
  padding: PaddingBox;
  margin: number;
}

/**
 */
export function computeTargetTop(
  align: MessageScrollerScrollOptions["align"],
  { itemTop, itemHeight, viewportHeight, scrollTop, padding, margin }: TargetTopInput,
): number {
  switch (align ?? "start") {
    case "center": {
      const visible = Math.max(0, viewportHeight - padding.start - padding.end);
      return itemTop - padding.start - (visible - itemHeight) / 2 - margin;
    }
    case "end":
      return itemTop - viewportHeight + itemHeight + padding.end + margin;
    case "nearest": {
      const bottom = itemTop + itemHeight;
      const viewTop = scrollTop + padding.start;
      const viewBottom = scrollTop + viewportHeight - padding.end;
      if (itemTop >= viewTop && bottom <= viewBottom) return scrollTop;
      if (itemTop < viewTop) return itemTop - padding.start - margin;
      return bottom - viewportHeight + padding.end + margin;
    }
    default:
      return itemTop - padding.start - margin;
  }
}

export function targetTopFor(
  element: HTMLElement,
  command: MessageScrollerScrollOptions | undefined,
  margin: number,
  viewport: HTMLElement,
  content: HTMLElement | undefined,
  itemOffsetTop: (el: HTMLElement) => number,
): number {
  return computeTargetTop(command?.align, {
    itemTop: itemOffsetTop(element),
    itemHeight: element.getBoundingClientRect().height,
    viewportHeight: viewport.clientHeight,
    scrollTop: viewport.scrollTop,
    padding: content ? paddingBox(content) : { start: 0, end: 0 },
    margin,
  });
}
