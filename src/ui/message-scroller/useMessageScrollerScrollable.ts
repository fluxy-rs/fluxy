import { useMessageScrollerContext } from "./message-scroller.context";

export function useMessageScrollerScrollable() {
  const ctx = useMessageScrollerContext("useMessageScrollerScrollable");
  return { start: ctx.scrollableStart, end: ctx.scrollableEnd };
}
