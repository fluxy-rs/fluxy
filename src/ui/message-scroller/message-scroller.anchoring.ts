import { createSignal, type Accessor } from "solid-js";
import {
  firstAnchorFrom,
  firstUnhandledAnchor,
  hasMultipleAnchorsFrom,
} from "./message-scroller.anchors";
import type { DomMeasure } from "./message-scroller.dom-measure";
import type { ScrollCommands } from "./message-scroller.commands";
import type { ScrollState } from "./message-scroller.scroll-state";
import type {
  MessageScrollerProviderProps,
  MessageScrollerScrollOptions,
} from "./message-scroller.types";
import { AT_EDGE_TOLERANCE } from "./message-scroller.utils";

type EngineOptions = Required<Omit<MessageScrollerProviderProps, "children">>;

/**
 *
 *
 */
export interface AnchoringOptions {
  viewport: Accessor<HTMLElement | undefined>;
  content: Accessor<HTMLElement | undefined>;
  options: () => EngineOptions;
  measure: DomMeasure;
  commands: ScrollCommands;
  state: ScrollState;
  scheduleState: () => void;
  commitState: () => void;
  scheduleVisibility: () => void;
  preserveScrollOnPrepend: Accessor<boolean>;
  getMessageElement: (id: string) => HTMLElement | undefined;
}

export interface Anchoring {
  pendingScroll: Accessor<boolean>;
  scrollToStart: (command?: MessageScrollerScrollOptions) => boolean;
  scrollToEnd: (command?: MessageScrollerScrollOptions) => boolean;
  scrollToElement: (
    element: HTMLElement,
    command?: MessageScrollerScrollOptions,
    options?: { keepPreviousPeek?: boolean },
  ) => boolean;
  scrollToMessage: (messageId: string, command?: MessageScrollerScrollOptions) => boolean;
  reanchorToAnchoredMessage: () => boolean;
  handleContentChange: () => void;
  handleResize: () => void;
  syncAfterScroll: () => void;
  capturePrependAnchor: () => void;
  notifyMessageRegistered: (id: string) => void;
  dispose: () => void;
}

export function createAnchoring(options: AnchoringOptions): Anchoring {
  const [pendingScroll, setPendingScroll] = createSignal(
    options.options().defaultScrollPosition !== "start",
  );

  const handledScrollAnchors = new WeakSet<HTMLElement>();
  let itemCount = 0;
  let firstItem: HTMLElement | null = null;
  let defaultApplied = false;
  let streamingTurn: HTMLElement | null = null;
  let pendingMessage: {
    id: string;
    options?: MessageScrollerScrollOptions;
  } | null = null;
  let prependAnchor: { element: HTMLElement; viewportTop: number } | null = null;
  let pendingFrame: number | null = null;

  const capturePrependAnchor = () => {
    const viewport = options.viewport();
    if (!viewport) {
      prependAnchor = null;
      return;
    }
    const viewportRect = viewport.getBoundingClientRect();
    const firstVisible =
      options.measure.items().find((element) => {
        if (!element.dataset.messageId) return false;
        const rect = element.getBoundingClientRect();
        return rect.bottom > viewportRect.top && rect.top < viewportRect.bottom;
      }) ?? null;
    prependAnchor = firstVisible
      ? {
          element: firstVisible,
          viewportTop: options.measure.itemTopInViewport(firstVisible),
        }
      : null;
  };

  const restorePrependAnchor = () => {
    const anchor = prependAnchor;
    const viewport = options.viewport();
    if (!anchor || !viewport || !anchor.element.isConnected) return false;

    const delta = options.measure.itemTopInViewport(anchor.element) - anchor.viewportTop;
    if (Math.abs(delta) <= AT_EDGE_TOLERANCE) return false;

    viewport.scrollTop += delta;
    anchor.viewportTop = options.measure.itemTopInViewport(anchor.element);
    options.scheduleState();
    options.scheduleVisibility();
    return true;
  };

  const scrollToStart = (command?: MessageScrollerScrollOptions) => {
    streamingTurn = null;
    return options.commands.scrollToStart(command);
  };

  const scrollToEnd = (command?: MessageScrollerScrollOptions) => {
    streamingTurn = null;
    return options.commands.scrollToEnd(command);
  };

  const scrollToElement: Anchoring["scrollToElement"] = (
    element,
    command,
    { keepPreviousPeek = false } = {},
  ) => {
    const ok = options.commands.scrollToElement(element, command, {
      keepPreviousPeek,
    });
    if (!ok) return false;

    prependAnchor = {
      element,
      viewportTop: options.measure.itemTopInViewport(element),
    };
    streamingTurn = keepPreviousPeek ? element : null;
    return true;
  };

  const reanchorToAnchoredMessage = () => {
    const element = streamingTurn;
    if (!element?.isConnected || !options.state.isAnchored()) return false;
    return scrollToElement(element, { align: "start" }, { keepPreviousPeek: true });
  };

  const applyDefaultScrollPosition = () => {
    const viewport = options.viewport();
    if (!viewport || defaultApplied || itemCount === 0) return false;

    const position = options.options().defaultScrollPosition;
    let applied = false;
    if (position === "last-anchor") {
      const list = options.measure.items();
      const lastAnchor =
        [...list].reverse().find((el) => el.dataset.scrollAnchor === "true") ?? null;
      if (!lastAnchor) {
        applied = scrollToEnd({ behavior: "auto" });
      } else if (
        options.measure.contentBottom() - options.measure.itemOffsetTop(lastAnchor) <=
        viewport.clientHeight
      ) {
        applied = scrollToEnd({ behavior: "auto" });
      } else {
        applied = scrollToElement(lastAnchor, { align: "start" }, { keepPreviousPeek: true });
      }
    } else if (position === "end") {
      applied = scrollToEnd({ behavior: "auto" });
    } else {
      applied = scrollToStart({ behavior: "auto" });
    }

    if (applied) {
      defaultApplied = true;
      setPendingScroll(false);
    }
    return applied;
  };

  const flushPendingScrollToMessage = () => {
    const pending = pendingMessage;
    if (!pending) return false;

    const element = options.getMessageElement(pending.id);
    if (!element || !scrollToElement(element, pending.options)) return false;

    pendingMessage = null;
    defaultApplied = true;
    setPendingScroll(false);
    return true;
  };

  const schedulePendingFlush = () => {
    if (pendingFrame !== null) return;
    pendingFrame = window.requestAnimationFrame(() => {
      pendingFrame = null;
      if (flushPendingScrollToMessage()) capturePrependAnchor();
    });
  };

  const scrollToMessage: Anchoring["scrollToMessage"] = (messageId, command) => {
    const element = options.getMessageElement(messageId);
    if (element) {
      if (scrollToElement(element, command)) {
        pendingMessage = null;
        return true;
      }
      pendingMessage = { id: messageId, options: command };
      return true;
    }
    if (itemCount === 0) {
      pendingMessage = { id: messageId, options: command };
      setPendingScroll(false);
      return true;
    }
    return false;
  };

  const handleContentChange = () => {
    const content = options.content();
    if (!content) return;

    const list = options.measure.items();
    const previousCount = itemCount;
    const previousFirst = firstItem;
    itemCount = list.length;
    firstItem = list[0] ?? null;

    if (list.length === 0) setPendingScroll(false);

    if (flushPendingScrollToMessage()) {
      capturePrependAnchor();
      return;
    }

    if (previousCount === 0) {
      for (const element of list) {
        if (element.dataset.scrollAnchor === "true") {
          handledScrollAnchors.add(element);
        }
      }
      if (applyDefaultScrollPosition()) {
        capturePrependAnchor();
        return;
      }
      if (list.length > 0 && options.options().autoScroll && scrollToEnd({ behavior: "auto" })) {
        capturePrependAnchor();
        return;
      }
      options.commitState();
      options.scheduleVisibility();
      capturePrependAnchor();
      return;
    }

    const previousFirstIndex = list.indexOf(previousFirst as HTMLElement);
    if (options.preserveScrollOnPrepend() && previousFirstIndex > 0) {
      restorePrependAnchor();
      capturePrependAnchor();
      return;
    }

    if (list.length > previousCount) {
      const firstNewAnchor = firstAnchorFrom(list, previousCount);
      if (firstNewAnchor) {
        if (
          options.options().autoScroll &&
          options.state.isFollowing() &&
          hasMultipleAnchorsFrom(list, previousCount)
        ) {
          scrollToEnd({ behavior: "auto" });
          capturePrependAnchor();
          return;
        }
        scrollToElement(firstNewAnchor, { align: "start" }, { keepPreviousPeek: true });
        handledScrollAnchors.add(firstNewAnchor);
        capturePrependAnchor();
        return;
      }
    }

    if (list.length === previousCount) {
      const unhandled = firstUnhandledAnchor(list, handledScrollAnchors);
      if (unhandled) {
        scrollToElement(unhandled, { align: "start" }, { keepPreviousPeek: true });
        handledScrollAnchors.add(unhandled);
        capturePrependAnchor();
        return;
      }
    }

    if (options.state.isFollowing() && options.options().autoScroll) {
      scrollToEnd({ behavior: "auto" });
    } else {
      options.commitState();
      options.scheduleVisibility();
    }
    capturePrependAnchor();
  };

  const handleResize = () => {
    if (options.state.isFollowing() && options.options().autoScroll) {
      scrollToEnd({ behavior: "auto" });
      return;
    }
    const before = options.commands.spacerHeight();
    if (reanchorToAnchoredMessage()) {
      if (options.options().autoScroll && before > 0 && options.commands.spacerHeight() === 0) {
        scrollToEnd({ behavior: "auto" });
      }
      return;
    }
    options.scheduleState();
    options.scheduleVisibility();
  };

  const syncAfterScroll = () => {
    options.commitState();
    options.scheduleVisibility();
    capturePrependAnchor();
  };

  return {
    pendingScroll,
    scrollToStart,
    scrollToEnd,
    scrollToElement,
    scrollToMessage,
    reanchorToAnchoredMessage,
    handleContentChange,
    handleResize,
    syncAfterScroll,
    capturePrependAnchor,
    notifyMessageRegistered(id) {
      if (pendingMessage?.id === id) schedulePendingFlush();
    },
    dispose() {
      if (pendingFrame !== null) {
        window.cancelAnimationFrame(pendingFrame);
        pendingFrame = null;
      }
    },
  };
}
