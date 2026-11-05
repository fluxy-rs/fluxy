import { type Accessor, createSignal, onCleanup, onMount } from "solid-js";
import type { ScrollMetrics } from "./ScrollArea.types";
import { computeMetrics } from "./ScrollArea.utils";

const EMPTY_METRICS: ScrollMetrics = {
  thumbRatio: 1,
  thumbOffset: 0,
  scrollable: false,
};

export interface ScrollAreaMetrics {
  vertical: Accessor<ScrollMetrics>;
  horizontal: Accessor<ScrollMetrics>;
  viewportRef: () => HTMLDivElement | undefined;
  setup: (el: HTMLDivElement) => void;
}

/**
 *
 *
 */
export function useScrollAreaMetrics(): ScrollAreaMetrics {
  const [vertical, setVertical] = createSignal<ScrollMetrics>(EMPTY_METRICS);
  const [horizontal, setHorizontal] = createSignal<ScrollMetrics>(EMPTY_METRICS);

  let viewport: HTMLDivElement | undefined;
  let content: Element | null = null;
  let observer: ResizeObserver | null = null;
  let mutations: MutationObserver | null = null;

  const measure = () => {
    if (!viewport) return;
    setVertical(computeMetrics(viewport.clientHeight, viewport.scrollTop, viewport.scrollHeight));
    setHorizontal(computeMetrics(viewport.clientWidth, viewport.scrollLeft, viewport.scrollWidth));
  };

  const observeContent = () => {
    const next = viewport?.firstElementChild ?? null;
    if (!observer || next === content) return;
    if (content) observer.unobserve(content);
    content = next;
    if (content) observer.observe(content);
  };

  const setup = (el: HTMLDivElement) => {
    viewport = el;
    measure();

    observer?.disconnect();
    observer = new ResizeObserver(() => measure());
    observer.observe(el);
    content = null;
    observeContent();

    el.addEventListener("scroll", measure, { passive: true });

    if (typeof MutationObserver !== "undefined") {
      mutations = new MutationObserver(() => {
        observeContent();
        measure();
      });
      mutations.observe(el, { childList: true });
    }
  };

  onCleanup(() => {
    observer?.disconnect();
    observer = null;
    mutations?.disconnect();
    mutations = null;
    content = null;
    viewport?.removeEventListener("scroll", measure);
    viewport = undefined;
  });

  onMount(() => {
    observeContent();
    measure();
  });

  return { vertical, horizontal, viewportRef: () => viewport, setup };
}
