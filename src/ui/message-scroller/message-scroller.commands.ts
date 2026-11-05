import type { Accessor } from "solid-js";
import type { DomMeasure } from "./message-scroller.dom-measure";
import { rowGap } from "./message-scroller.measure";
import type { ScrollState } from "./message-scroller.scroll-state";
import { targetTopFor as computeTargetTopFor } from "./message-scroller.scroll-target";
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
export interface ScrollCommandsOptions {
  viewport: Accessor<HTMLElement | undefined>;
  content: Accessor<HTMLElement | undefined>;
  spacer: Accessor<HTMLElement | undefined>;
  options: () => EngineOptions;
  measure: DomMeasure;
  state: ScrollState;
  scheduleVisibility: () => void;
}

export interface ScrollCommands {
  spacerHeight: () => number;
  setScrollTop: (top: number, options?: { behavior?: ScrollBehavior; auto?: boolean }) => void;
  targetTopFor: (
    element: HTMLElement,
    command: MessageScrollerScrollOptions | undefined,
    margin: number,
  ) => number;
  scrollToStart: (command?: MessageScrollerScrollOptions) => boolean;
  scrollToEnd: (command?: MessageScrollerScrollOptions) => boolean;
  scrollToElement: (
    element: HTMLElement,
    command?: MessageScrollerScrollOptions,
    options?: { keepPreviousPeek?: boolean },
  ) => boolean;
}

export function createScrollCommands(options: ScrollCommandsOptions): ScrollCommands {
  let spacerHeightValue = 0;

  /**
   */
  const spacerGap = () => rowGap(options.spacer()?.parentElement ?? null);

  const setSpacerHeight = (height: number) => {
    const spacer = options.spacer();
    if (!spacer) return;

    const next = Math.max(0, Math.ceil(height));
    if (spacerHeightValue === next) return;

    spacerHeightValue = next;
    spacer.hidden = next === 0;
    spacer.style.height = `${next}px`;
    spacer.style.marginTop = next > 0 ? `${-spacerGap()}px` : "";
  };

  const setScrollTop: ScrollCommands["setScrollTop"] = (
    top,
    { behavior = "auto", auto = false } = {},
  ) => {
    const viewport = options.viewport();
    if (!viewport) return;

    const next = Math.max(0, top);
    if (Math.abs(viewport.scrollTop - next) <= AT_EDGE_TOLERANCE) {
      viewport.scrollTop = next;
      options.state.commit();
      return;
    }
    if (auto) options.state.beginAutoScroll();
    viewport.scrollTo({ top: next, behavior });
    options.state.schedule();
  };

  const targetTopFor: ScrollCommands["targetTopFor"] = (element, command, margin) => {
    const viewport = options.viewport();
    if (!viewport) return 0;
    return computeTargetTopFor(
      element,
      command,
      margin,
      viewport,
      options.content(),
      options.measure.itemOffsetTop,
    );
  };

  return {
    spacerHeight: () => spacerHeightValue,
    setScrollTop,
    targetTopFor,
    scrollToStart(command) {
      if (!options.viewport()) return false;
      setSpacerHeight(0);
      options.state.setFree();
      setScrollTop(0, { behavior: command?.behavior ?? "auto" });
      options.scheduleVisibility();
      return true;
    },
    scrollToEnd(command) {
      if (!options.viewport()) return false;
      setSpacerHeight(0);
      if (options.options().autoScroll) options.state.setFollowing();
      else options.state.setFree();
      setScrollTop(options.measure.maxScrollTop(), {
        behavior: command?.behavior ?? "auto",
        auto: true,
      });
      options.scheduleVisibility();
      return true;
    },
    scrollToElement(element, command, { keepPreviousPeek = false } = {}) {
      const viewport = options.viewport();
      const content = options.content();
      if (!viewport || !content?.contains(element)) return false;

      const margin =
        (command?.scrollMargin ?? options.options().scrollMargin) +
        (keepPreviousPeek ? options.options().scrollPreviousItemPeek : 0);
      const target = targetTopFor(element, command, margin);
      setSpacerHeight(
        Math.max(0, target + viewport.clientHeight - options.measure.contentBottom()),
      );

      if (keepPreviousPeek) options.state.anchorTo();
      else options.state.settleJump();

      setScrollTop(target, { behavior: command?.behavior ?? "auto" });
      options.scheduleVisibility();
      return true;
    },
  };
}
