import { createSignal, type Accessor } from "solid-js";
import type { MessageScrollerProviderProps } from "./message-scroller.types";
import { AT_EDGE_TOLERANCE } from "./message-scroller.utils";

/**
 *
 *
 */

type EngineOptions = Required<Omit<MessageScrollerProviderProps, "children">>;

export interface VisibilityOptions {
  viewport: Accessor<HTMLElement | undefined>;
  content: Accessor<HTMLElement | undefined>;
  items: () => HTMLElement[];
  options: () => EngineOptions;
  registeredElements: () => Iterable<HTMLElement>;
}

export interface Visibility {
  currentAnchorId: Accessor<string | null>;
  visibleMessageIds: Accessor<string[]>;
  observe: (element: HTMLElement) => void;
  unobserve: (id: string, element: HTMLElement) => void;
  schedule: () => void;
  ensureObserver: () => void;
  subscribe: () => () => void;
  dispose: () => void;
}

export function createVisibility(options: VisibilityOptions): Visibility {
  const [currentAnchorId, setCurrentAnchorId] = createSignal<string | null>(null);
  const [visibleMessageIds, setVisibleMessageIds] = createSignal<string[]>([]);

  const visibleIds = new Set<string>();
  let observer: IntersectionObserver | null = null;
  let visibilityFrame: number | null = null;
  let visibilitySubscribers = 0;

  const schedule = () => {
    if (visibilityFrame !== null) return;
    visibilityFrame = window.requestAnimationFrame(() => {
      visibilityFrame = null;
      if (visibilitySubscribers > 0) computeVisibility();
    });
  };

  const computeVisibility = () => {
    const viewport = options.viewport();
    const content = options.content();
    if (!viewport || !content) {
      setVisibleMessageIds([]);
      setCurrentAnchorId(null);
      return;
    }

    const viewportRect = viewport.getBoundingClientRect();
    const readingLine =
      viewportRect.top + options.options().scrollMargin + options.options().scrollPreviousItemPeek;
    const withoutObserver = typeof IntersectionObserver === "undefined";
    const visible: string[] = [];
    let anchorId: string | null = null;

    for (const item of options.items()) {
      const id = item.dataset.messageId;
      if (!id) continue;

      const isAnchor = item.dataset.scrollAnchor === "true";
      const rect = isAnchor || withoutObserver ? item.getBoundingClientRect() : null;
      const isVisible =
        withoutObserver && rect
          ? rect.bottom > readingLine && rect.top < viewportRect.bottom
          : visibleIds.has(id);

      if (isVisible) visible.push(id);
      if (isAnchor && rect && rect.top <= readingLine + AT_EDGE_TOLERANCE) {
        anchorId = id;
      }
    }

    if (visible.length === 0 && anchorId === null) {
      setVisibleMessageIds([]);
      setCurrentAnchorId(null);
      return;
    }
    setVisibleMessageIds(visible);
    setCurrentAnchorId(anchorId);
  };

  const startObserver = () => {
    if (typeof IntersectionObserver === "undefined") {
      schedule();
      return;
    }
    const viewport = options.viewport();
    if (!observer && viewport) {
      observer = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            const id = (entry.target as HTMLElement).dataset.messageId;
            if (!id) continue;
            if (entry.isIntersecting) visibleIds.add(id);
            else visibleIds.delete(id);
          }
          schedule();
        },
        {
          root: viewport,
          rootMargin: `${-(options.options().scrollMargin + options.options().scrollPreviousItemPeek)}px 0px 0px 0px`,
          threshold: [0, 0.01, 0.5, 1],
        },
      );
    }
    for (const element of options.registeredElements()) {
      observer?.observe(element);
    }
    schedule();
  };

  const stopObserver = () => {
    if (visibilityFrame !== null) {
      window.cancelAnimationFrame(visibilityFrame);
      visibilityFrame = null;
    }
    observer?.disconnect();
    observer = null;
    visibleIds.clear();
    setVisibleMessageIds([]);
    setCurrentAnchorId(null);
  };

  return {
    currentAnchorId,
    visibleMessageIds,
    schedule,
    ensureObserver() {
      if (visibilitySubscribers > 0 && !observer) startObserver();
    },
    observe(element) {
      observer?.observe(element);
    },
    unobserve(id, element) {
      visibleIds.delete(id);
      observer?.unobserve(element);
    },
    subscribe() {
      visibilitySubscribers += 1;
      if (visibilitySubscribers === 1) startObserver();
      schedule();

      let released = false;
      return () => {
        if (released) return;
        released = true;
        visibilitySubscribers -= 1;
        if (visibilitySubscribers === 0) stopObserver();
      };
    },
    dispose() {
      if (visibilityFrame !== null) {
        window.cancelAnimationFrame(visibilityFrame);
        visibilityFrame = null;
      }
      observer?.disconnect();
      observer = null;
    },
  };
}
