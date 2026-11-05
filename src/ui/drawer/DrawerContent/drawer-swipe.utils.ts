import type { DrawerSwipeDirection } from "../Drawer/Drawer.types";

export const DISMISS_RATIO = 0.3;
export const DISMISS_MIN_PX = 80;

/**
 *
 */

export function isInteractive(target: Element): boolean {
  return !!target.closest(
    "button, a, input, textarea, select, [role=button], [contenteditable], [data-drawer-no-swipe]",
  );
}

export function findScroller(
  target: Element,
  popup: HTMLElement,
  horizontal: boolean,
): HTMLElement | null {
  let node: Element | null = target;
  while (node && node !== popup) {
    const style = getComputedStyle(node);
    const overflow = horizontal ? style.overflowX : style.overflowY;
    const scrollable =
      /auto|scroll/.test(overflow) &&
      (horizontal ? node.scrollWidth > node.clientWidth : node.scrollHeight > node.clientHeight);
    if (scrollable) return node as HTMLElement;
    node = node.parentElement;
  }
  return null;
}

export function canDragFromScroller(
  scroller: HTMLElement,
  horizontal: boolean,
  dir: DrawerSwipeDirection,
): boolean {
  if (horizontal) {
    if (dir === "left") {
      return scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - 1;
    }
    return scroller.scrollLeft <= 0;
  }
  if (dir === "down") return scroller.scrollTop <= 0;
  return scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1;
}

/**
 */
export function canSwipeFrom(
  target: Element,
  popup: HTMLElement,
  horizontal: boolean,
  dir: DrawerSwipeDirection,
): boolean {
  if (isInteractive(target)) return false;
  if (target.closest('[data-slot="drawer-swipe-handle"]')) return true;

  const scroller = findScroller(target, popup, horizontal);
  if (!scroller) return true;
  return canDragFromScroller(scroller, horizontal, dir);
}

export function resolveDismissThreshold(panelSize: number): number {
  return Math.max(DISMISS_MIN_PX, panelSize * DISMISS_RATIO);
}

export function clampSwipeOffset(dir: DrawerSwipeDirection, delta: number): number {
  return dir === "left" || dir === "up" ? Math.min(0, delta) : Math.max(0, delta);
}

export function isClosingSwipe(
  dir: DrawerSwipeDirection,
  delta: number,
  threshold: number,
): boolean {
  return (
    (dir === "down" && delta > threshold) ||
    (dir === "up" && -delta > threshold) ||
    (dir === "left" && -delta > threshold) ||
    (dir === "right" && delta > threshold)
  );
}
