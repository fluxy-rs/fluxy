import { createEffect, onCleanup, type Accessor } from "solid-js";
import { isOverflowElement } from "../lib";

export interface UseScrollLockOptions {
  allowedSelector?: string;
}

let lockCount = 0;
let release: (() => void) | undefined;
const allowedSelectors = new Set<string>();

/**
 *
 */
function getViewportScroller(): HTMLElement {
  const html = document.documentElement;
  return isOverflowElement(html) ? html : document.body;
}

function supportsStableScrollbarGutter(): boolean {
  return (
    typeof CSS !== "undefined" &&
    typeof CSS.supports === "function" &&
    CSS.supports("scrollbar-gutter", "stable")
  );
}

function isWithinAllowedScroll(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  for (const selector of allowedSelectors) {
    if (target.closest(selector)) return true;
  }
  return false;
}

function lockViewport(): () => void {
  const html = document.documentElement;
  const body = document.body;
  const scroller = getViewportScroller();

  const previous = {
    htmlScrollbarGutter: html.style.scrollbarGutter,
    scrollerOverflowX: scroller.style.overflowX,
    scrollerOverflowY: scroller.style.overflowY,
    bodyPaddingRight: body.style.paddingRight,
    scrollTop: scroller.scrollTop,
    scrollLeft: scroller.scrollLeft,
  };

  const scrollbarWidth = Math.max(0, window.innerWidth - html.clientWidth);
  const gutterSupported = supportsStableScrollbarGutter();
  if (scrollbarWidth > 0) {
    if (gutterSupported) {
      html.style.scrollbarGutter = "stable";
    } else {
      const padding = Number.parseFloat(getComputedStyle(body).paddingRight) || 0;
      body.style.paddingRight = `${padding + scrollbarWidth}px`;
    }
  }

  scroller.style.overflowX = "hidden";
  scroller.style.overflowY = "hidden";

  const preventScroll = (event: Event) => {
    if (isWithinAllowedScroll(event.target)) return;
    event.preventDefault();
  };
  document.addEventListener("wheel", preventScroll, { passive: false });
  document.addEventListener("touchmove", preventScroll, { passive: false });

  return () => {
    html.style.scrollbarGutter = previous.htmlScrollbarGutter;
    if (!gutterSupported) {
      body.style.paddingRight = previous.bodyPaddingRight;
    }
    scroller.style.overflowX = previous.scrollerOverflowX;
    scroller.style.overflowY = previous.scrollerOverflowY;
    document.removeEventListener("wheel", preventScroll);
    document.removeEventListener("touchmove", preventScroll);
    scroller.scrollTop = previous.scrollTop;
    scroller.scrollLeft = previous.scrollLeft;
  };
}

function acquireLock(allowedSelector?: string): () => void {
  lockCount += 1;
  if (allowedSelector) allowedSelectors.add(allowedSelector);
  if (lockCount === 1) release = lockViewport();

  let released = false;
  return () => {
    if (released) return;
    released = true;
    lockCount -= 1;
    if (lockCount === 0) {
      release?.();
      release = undefined;
      allowedSelectors.clear();
    }
  };
}

/**
 *
 */
export function useScrollLock(locked: Accessor<boolean>, options?: UseScrollLockOptions) {
  createEffect(() => {
    if (!locked()) return;
    const unlock = acquireLock(options?.allowedSelector);
    onCleanup(unlock);
  });
}
