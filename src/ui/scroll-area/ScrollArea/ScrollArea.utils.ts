import type { ScrollMetrics } from "./ScrollArea.types";

export function computeMetrics(client: number, scroll: number, scrollSize: number): ScrollMetrics {
  const scrollable = scrollSize > client + 1;
  if (!scrollable) return { thumbRatio: 1, thumbOffset: 0, scrollable: false };
  const maxScroll = scrollSize - client;
  return {
    thumbRatio: client / scrollSize,
    thumbOffset: scroll / maxScroll,
    scrollable,
  };
}
