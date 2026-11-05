import { onCleanup, onMount } from "solid-js";
import { useMessageScrollerContext } from "./message-scroller.context";
import type { MessageScrollerVisibilityValue } from "./message-scroller.types";

/**
 */
export function useMessageScrollerVisibility(): MessageScrollerVisibilityValue {
  const ctx = useMessageScrollerContext("useMessageScrollerVisibility");

  onMount(() => {
    const unsubscribe = ctx.subscribeVisibility();
    onCleanup(unsubscribe);
  });

  return {
    currentAnchorId: ctx.currentAnchorId,
    visibleMessageIds: ctx.visibleMessageIds,
  };
}
