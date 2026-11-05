import { createSignal, type Accessor } from "solid-js";
import type { MessageScrollerProviderProps } from "./message-scroller.types";
import { AT_EDGE_TOLERANCE } from "./message-scroller.utils";

type EngineOptions = Required<Omit<MessageScrollerProviderProps, "children">>;

/**
 */
export type ScrollMode =
  | "following-bottom"
  | "free-scrolling"
  | "anchored-to-message"
  | "settling-jump";

const AUTO_SCROLLING_TIMEOUT_MS = 180;

export interface ScrollStateOptions {
  viewport: Accessor<HTMLElement | undefined>;
  options: () => EngineOptions;
  contentBottom: () => number;
}

export interface ScrollState {
  isFollowing: () => boolean;
  isAnchored: () => boolean;
  scrollableStart: Accessor<boolean>;
  scrollableEnd: Accessor<boolean>;
  autoscrolling: Accessor<boolean>;

  setFollowing: () => void;
  setFree: () => void;
  anchorTo: () => void;
  settleJump: () => void;

  commit: () => void;
  schedule: () => void;
  beginAutoScroll: () => void;
  endAutoScroll: () => void;
  releaseFollow: () => void;
  dispose: () => void;
}

/**
 *
 *
 */
export function createScrollState(options: ScrollStateOptions): ScrollState {
  const [scrollableStart, setScrollableStart] = createSignal(false);
  const [scrollableEnd, setScrollableEnd] = createSignal(false);
  const [autoscrolling, setAutoscrolling] = createSignal(false);

  let mode: ScrollMode = options.options().autoScroll ? "following-bottom" : "free-scrolling";
  let lastScrollTop = 0;
  let autoscrollingTimer: number | null = null;
  let stateFrame: number | null = null;

  const computeScrollable = () => {
    const viewport = options.viewport();
    if (!viewport) return { start: false, end: false };

    const threshold = options.options().scrollEdgeThreshold;
    const bottom = options.contentBottom();
    return {
      start: viewport.scrollTop > threshold,
      end: bottom - viewport.scrollTop - viewport.clientHeight > threshold,
    };
  };

  const updateModeFromScroll = (state: { start: boolean; end: boolean }) => {
    const viewport = options.viewport();
    if (!viewport) return;

    const top = viewport.scrollTop;
    const movedUp = top < lastScrollTop - AT_EDGE_TOLERANCE;
    lastScrollTop = top;

    if (
      options.options().autoScroll &&
      !state.end &&
      mode !== "settling-jump" &&
      mode !== "anchored-to-message"
    ) {
      mode = "following-bottom";
    } else if (mode === "following-bottom" && state.end && movedUp && !autoscrolling()) {
      mode = "free-scrolling";
    }
  };

  const commit = () => {
    const state = computeScrollable();
    updateModeFromScroll(state);
    const exposed = mode === "following-bottom" ? { ...state, end: false } : state;
    setScrollableStart(exposed.start);
    setScrollableEnd(exposed.end);
  };

  const schedule = () => {
    if (stateFrame !== null) return;
    stateFrame = window.requestAnimationFrame(() => {
      stateFrame = null;
      commit();
    });
  };

  const endAutoScroll = () => {
    if (autoscrollingTimer !== null) {
      window.clearTimeout(autoscrollingTimer);
      autoscrollingTimer = null;
    }
    if (autoscrolling()) {
      setAutoscrolling(false);
      commit();
    }
  };

  const beginAutoScroll = () => {
    if (autoscrollingTimer !== null) {
      window.clearTimeout(autoscrollingTimer);
      autoscrollingTimer = null;
    }
    if (!autoscrolling()) {
      setAutoscrolling(true);
      commit();
    }
    autoscrollingTimer = window.setTimeout(() => {
      autoscrollingTimer = null;
      setAutoscrolling(false);
      commit();
    }, AUTO_SCROLLING_TIMEOUT_MS);
  };

  return {
    isFollowing: () => mode === "following-bottom",
    isAnchored: () => mode === "anchored-to-message",
    scrollableStart,
    scrollableEnd,
    autoscrolling,
    setFollowing() {
      mode = "following-bottom";
    },
    setFree() {
      mode = "free-scrolling";
    },
    anchorTo() {
      mode = "anchored-to-message";
    },
    settleJump() {
      mode = "settling-jump";
    },
    commit,
    schedule,
    beginAutoScroll,
    endAutoScroll,
    releaseFollow() {
      if (
        mode === "following-bottom" ||
        mode === "anchored-to-message" ||
        mode === "settling-jump"
      ) {
        endAutoScroll();
        mode = "free-scrolling";
      }
    },
    dispose() {
      if (autoscrollingTimer !== null) {
        window.clearTimeout(autoscrollingTimer);
        autoscrollingTimer = null;
      }
      if (stateFrame !== null) {
        window.cancelAnimationFrame(stateFrame);
        stateFrame = null;
      }
    },
  };
}
