import { createEffect, createSignal, onCleanup } from "solid-js";
import type {
  MessageScrollerContextValue,
  MessageScrollerItemEntry,
  MessageScrollerProviderProps,
} from "./message-scroller.types";
import { createDomMeasure } from "./message-scroller.dom-measure";
import { createAnchoring } from "./message-scroller.anchoring";
import { createScrollCommands } from "./message-scroller.commands";
import { createScrollState } from "./message-scroller.scroll-state";
import { createVisibility } from "./message-scroller.visibility";

const NAV_KEYS = new Set(["ArrowDown", "ArrowUp", "End", "Home", "PageDown", "PageUp", " "]);

/**
 */
export function useMessageScrollerEngine(
  options: () => Required<Omit<MessageScrollerProviderProps, "children">>,
): MessageScrollerContextValue {
  const [viewport, setViewport] = createSignal<HTMLElement>();
  const [content, setContent] = createSignal<HTMLElement>();
  const [spacer, setSpacerSignal] = createSignal<HTMLElement>();
  const [preserveScrollOnPrepend, setPreserveScrollOnPrepend] = createSignal(true);

  const messageElements = new Map<string, HTMLElement>();

  const measure = createDomMeasure({ viewport, content, spacer });
  const { items, contentBottom } = measure;

  const visibility = createVisibility({
    viewport,
    content,
    items,
    options,
    registeredElements: () => messageElements.values(),
  });
  const { currentAnchorId, visibleMessageIds, schedule: scheduleVisibilitySync } = visibility;

  const scrollState = createScrollState({ viewport, options, contentBottom });
  const {
    scrollableStart,
    scrollableEnd,
    autoscrolling,
    commit: commitScrollState,
    schedule: scheduleStateCommit,
    releaseFollow,
  } = scrollState;

  const commands = createScrollCommands({
    viewport,
    content,
    spacer,
    options,
    measure,
    state: scrollState,
    scheduleVisibility: scheduleVisibilitySync,
  });

  const anchoring = createAnchoring({
    viewport,
    content,
    options,
    measure,
    commands,
    state: scrollState,
    scheduleState: scheduleStateCommit,
    commitState: commitScrollState,
    scheduleVisibility: scheduleVisibilitySync,
    preserveScrollOnPrepend,
    getMessageElement: (id) => messageElements.get(id),
  });
  const {
    pendingScroll,
    scrollToStart,
    scrollToEnd,
    scrollToMessage,
    handleContentChange,
    handleResize,
    syncAfterScroll,
  } = anchoring;

  const registerMessage = (
    id: string,
    element: HTMLElement | undefined,
    previous?: HTMLElement,
  ) => {
    if (element) {
      messageElements.set(id, element);
      visibility.observe(element);
      scheduleVisibilitySync();
      anchoring.notifyMessageRegistered(id);
      return;
    }
    if (previous && messageElements.get(id) === previous) {
      messageElements.delete(id);
      visibility.unobserve(id, previous);
      scheduleVisibilitySync();
    }
  };

  const registerItem = (entry: MessageScrollerItemEntry) => {
    const id = entry.id;
    if (id) registerMessage(id, entry.element);
    return () => {
      if (id) registerMessage(id, undefined, entry.element);
    };
  };

  const attachSpacer = (element: HTMLElement | undefined) => {
    setSpacerSignal(element);
  };

  createEffect(() => {
    const root = content();
    if (!root) return;
    handleContentChange();

    const mutation =
      typeof MutationObserver !== "undefined"
        ? new MutationObserver(() => handleContentChange())
        : null;
    mutation?.observe(root, { childList: true });

    const anchorMutation =
      typeof MutationObserver !== "undefined"
        ? new MutationObserver(() => handleContentChange())
        : null;
    anchorMutation?.observe(root, {
      attributes: true,
      attributeFilter: ["data-scroll-anchor"],
      subtree: true,
    });

    let frame = 0;
    const resize =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => {
            window.cancelAnimationFrame(frame);
            frame = window.requestAnimationFrame(handleResize);
          })
        : null;
    resize?.observe(root);

    onCleanup(() => {
      mutation?.disconnect();
      anchorMutation?.disconnect();
      resize?.disconnect();
      window.cancelAnimationFrame(frame);
    });
  });

  createEffect(() => {
    const vp = viewport();
    if (!vp) return;
    visibility.ensureObserver();

    const onScroll = () => syncAfterScroll();
    const onIntent = () => releaseFollow();
    const onKeyDown = (event: KeyboardEvent) => {
      if (NAV_KEYS.has(event.key)) releaseFollow();
    };
    vp.addEventListener("scroll", onScroll, { passive: true });
    vp.addEventListener("wheel", onIntent, { passive: true });
    vp.addEventListener("touchmove", onIntent, { passive: true });
    vp.addEventListener("keydown", onKeyDown);

    let frame = 0;
    const resize =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => {
            window.cancelAnimationFrame(frame);
            frame = window.requestAnimationFrame(handleResize);
          })
        : null;
    resize?.observe(vp);

    onCleanup(() => {
      vp.removeEventListener("scroll", onScroll);
      vp.removeEventListener("wheel", onIntent);
      vp.removeEventListener("touchmove", onIntent);
      vp.removeEventListener("keydown", onKeyDown);
      resize?.disconnect();
      window.cancelAnimationFrame(frame);
    });
  });

  onCleanup(() => {
    anchoring.dispose();
    scrollState.dispose();
    visibility.dispose();
  });

  return {
    viewport,
    setViewport,
    content,
    setContent,
    setSpacer: attachSpacer,
    registerItem,
    preserveScrollOnPrepend,
    setPreserveScrollOnPrepend,
    scrollableStart,
    scrollableEnd,
    autoscrolling,
    pendingScroll,
    scrollToStart,
    scrollToEnd,
    scrollToMessage,
    currentAnchorId,
    visibleMessageIds,
    subscribeVisibility: visibility.subscribe,
    options,
  };
}
