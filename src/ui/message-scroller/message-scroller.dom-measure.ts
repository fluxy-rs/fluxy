import type { Accessor } from "solid-js";
import { paddingBox } from "./message-scroller.measure";

/**
 *
 */

export interface DomMeasureOptions {
  viewport: Accessor<HTMLElement | undefined>;
  content: Accessor<HTMLElement | undefined>;
  spacer: Accessor<HTMLElement | undefined>;
}

export interface DomMeasure {
  items: () => HTMLElement[];
  itemOffsetTop: (element: HTMLElement) => number;
  itemTopInViewport: (element: HTMLElement) => number;
  contentBottom: () => number;
  maxScrollTop: () => number;
}

export function createDomMeasure(options: DomMeasureOptions): DomMeasure {
  const items = (): HTMLElement[] => {
    const root = options.content();
    const spacer = options.spacer();
    if (!root) return [];
    return Array.from(root.children).filter(
      (child): child is HTMLElement => child instanceof HTMLElement && child !== spacer,
    );
  };

  const itemOffsetTop = (element: HTMLElement) => {
    const viewport = options.viewport();
    if (!viewport) return 0;
    return (
      element.getBoundingClientRect().top -
      viewport.getBoundingClientRect().top +
      viewport.scrollTop
    );
  };

  const itemTopInViewport = (element: HTMLElement) => {
    const viewport = options.viewport();
    if (!viewport) return 0;
    return element.getBoundingClientRect().top - viewport.getBoundingClientRect().top;
  };

  const contentBottom = () => {
    const viewport = options.viewport();
    const root = options.content();
    if (!viewport || !root) return 0;

    const pad = paddingBox(root);
    const viewportRect = viewport.getBoundingClientRect();
    let bottom = pad.start + pad.end;
    for (const item of items()) {
      const rect = item.getBoundingClientRect();
      bottom = Math.max(bottom, rect.bottom - viewportRect.top + viewport.scrollTop + pad.end);
    }
    return bottom;
  };

  const maxScrollTop = () => {
    const viewport = options.viewport();
    return viewport ? Math.max(0, viewport.scrollHeight - viewport.clientHeight) : 0;
  };

  return {
    items,
    itemOffsetTop,
    itemTopInViewport,
    contentBottom,
    maxScrollTop,
  };
}
