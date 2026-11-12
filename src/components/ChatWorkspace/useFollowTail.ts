import { createEffect, onCleanup, onMount } from "solid-js";

/**
 * Keeps a scroller at the tail while the conversation grows.
 *
 * "Following" is decided from the scroll event rather than from the last
 * append, so the check is made against the layout the user actually saw. A
 * line or two of slack counts as following: the last message can be taller
 * than the viewport, and an exact comparison would unpin on a fractional
 * scroll position.
 */
export function useFollowTail(input: {
  deviceId: () => string | undefined;
  count: () => number;
  viewport: () => HTMLElement | undefined;
}) {
  let pinned = true;
  let lastDeviceId: string | undefined;
  let lastCount = 0;

  createEffect(() => {
    const deviceId = input.deviceId();
    const count = input.count();
    const switched = deviceId !== lastDeviceId;
    const grew = count > lastCount;
    lastDeviceId = deviceId;
    lastCount = count;
    if (!switched && !grew) return;
    if (!switched && !pinned) return;
    queueMicrotask(() => {
      const list = input.viewport();
      if (!list) return;
      list.scrollTop = list.scrollHeight;
      pinned = true;
    });
  });

  onMount(() => {
    const list = input.viewport();
    if (!list) return;
    const onScroll = () => {
      pinned = list.scrollHeight - list.scrollTop - list.clientHeight < 48;
    };
    list.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => list.removeEventListener("scroll", onScroll));
  });
}
