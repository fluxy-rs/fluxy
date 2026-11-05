import type { ReferenceElement, Rect, Strategy } from "../types";

export function getViewportRect(el: ReferenceElement): Rect {
  const rect = el.getBoundingClientRect();
  return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
}

export function getRectRelativeTo(el: ReferenceElement, strategy: Strategy): Rect {
  const rect = getViewportRect(el);
  if (strategy === "fixed") return rect;
  const scrollX = window.scrollX ?? window.pageXOffset;
  const scrollY = window.scrollY ?? window.pageYOffset;
  return {
    x: rect.x + scrollX,
    y: rect.y + scrollY,
    width: rect.width,
    height: rect.height,
  };
}

export function getViewportBoundary(strategy: Strategy, padding = 0): Rect {
  const viewport = window.visualViewport;
  const width = viewport?.width ?? document.documentElement.clientWidth;
  const height = viewport?.height ?? document.documentElement.clientHeight;
  const offsetX = viewport?.offsetLeft ?? 0;
  const offsetY = viewport?.offsetTop ?? 0;
  const scrollX = strategy === "fixed" ? 0 : (window.scrollX ?? window.pageXOffset);
  const scrollY = strategy === "fixed" ? 0 : (window.scrollY ?? window.pageYOffset);
  return {
    x: scrollX + offsetX + padding,
    y: scrollY + offsetY + padding,
    width: width - padding * 2,
    height: height - padding * 2,
  };
}

/**
 *
 */
export function isOverflowElement(el: Element): boolean {
  const { overflow, overflowX, overflowY } = getComputedStyle(el);
  return /auto|scroll|overlay|hidden/.test(overflow + overflowX + overflowY);
}

export function getOverflowAncestors(node: Element): Array<Element | Window> {
  const result: Array<Element | Window> = [];
  let el: Element | null = node.parentElement;

  while (el) {
    if (isOverflowElement(el)) {
      result.push(el);
    }
    el = el.parentElement;
  }

  result.push(window);
  return result;
}
