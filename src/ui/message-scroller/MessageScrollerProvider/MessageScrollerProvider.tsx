import { mergeProps, splitProps, type JSX } from "solid-js";
import { MessageScrollerContext } from "../message-scroller.context";
import { useMessageScrollerEngine } from "../useMessageScrollerEngine";
import type { MessageScrollerProviderProps } from "../message-scroller.types";

export function MessageScrollerProvider(props: MessageScrollerProviderProps): JSX.Element {
  const merged = mergeProps(
    {
      autoScroll: false,
      defaultScrollPosition: "end" as const,
      scrollEdgeThreshold: 8,
      scrollMargin: 0,
      scrollPreviousItemPeek: 64,
    },
    props,
  );
  const [local] = splitProps(merged, [
    "autoScroll",
    "defaultScrollPosition",
    "scrollEdgeThreshold",
    "scrollMargin",
    "scrollPreviousItemPeek",
    "children",
  ]);

  const ctx = useMessageScrollerEngine(() => ({
    autoScroll: local.autoScroll,
    defaultScrollPosition: local.defaultScrollPosition,
    scrollEdgeThreshold: local.scrollEdgeThreshold,
    scrollMargin: local.scrollMargin,
    scrollPreviousItemPeek: local.scrollPreviousItemPeek,
  }));

  return (
    <MessageScrollerContext.Provider value={ctx}>{local.children}</MessageScrollerContext.Provider>
  );
}
