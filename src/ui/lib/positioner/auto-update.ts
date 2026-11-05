import type { ReferenceElement } from "./types";
import { getOverflowAncestors } from "./utils/dom";

export interface AutoUpdateOptions {
  ancestorScroll?: boolean;
  ancestorResize?: boolean;
  elementResize?: boolean;
}

/**
 *
 */
export function autoUpdate(
  reference: ReferenceElement,
  floating: HTMLElement,
  update: () => void,
  options: AutoUpdateOptions = {},
): () => void {
  const { ancestorScroll = true, ancestorResize = true, elementResize = true } = options;

  const isRealElement = reference instanceof Element;
  const ancestorAnchor: Element | null = isRealElement
    ? reference
    : ((reference as { contextElement?: Element }).contextElement ?? null);

  const ancestors =
    (ancestorScroll || ancestorResize) && ancestorAnchor
      ? getOverflowAncestors(ancestorAnchor)
      : [];

  ancestors.forEach((ancestor) => {
    if (ancestorScroll) {
      ancestor.addEventListener("scroll", update, { passive: true });
    }
  });

  let resizeObserver: ResizeObserver | null = null;
  if ((ancestorResize || elementResize) && typeof ResizeObserver !== "undefined") {
    resizeObserver = new ResizeObserver(() => update());
    if (elementResize) {
      if (isRealElement) resizeObserver.observe(reference);
      resizeObserver.observe(floating);
    }
    if (ancestorResize) {
      ancestors.forEach((ancestor) => {
        if (ancestor instanceof Element) resizeObserver!.observe(ancestor);
      });
    }
  }

  window.addEventListener("resize", update);

  const visualViewport = window.visualViewport;
  visualViewport?.addEventListener("resize", update);
  visualViewport?.addEventListener("scroll", update);

  return () => {
    ancestors.forEach((ancestor) => {
      ancestor.removeEventListener("scroll", update);
    });
    resizeObserver?.disconnect();
    window.removeEventListener("resize", update);
    visualViewport?.removeEventListener("resize", update);
    visualViewport?.removeEventListener("scroll", update);
  };
}
