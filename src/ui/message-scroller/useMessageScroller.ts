import { useMessageScrollerContext } from "./message-scroller.context";

export function useMessageScroller() {
  const ctx = useMessageScrollerContext("useMessageScroller");
  return {
    scrollToStart: ctx.scrollToStart,
    scrollToEnd: ctx.scrollToEnd,
    scrollToMessage: ctx.scrollToMessage,
  };
}
