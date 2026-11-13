import { createSignal, onCleanup, onMount } from "solid-js";

/**
 * Whether the window is too small for the two-column layout.
 *
 * This cannot be `engine.mobile()`: a car head unit runs Android on a 1280 px
 * landscape screen, and the platform test gave it the phone layout - a single
 * column, a bottom bar, and 900 px of empty space beside it. Height matters as
 * much as width, because a phone on its side is wide and still has no room.
 */
export function createCompact() {
  const query = "(max-width: 1023px), (max-height: 499px)";
  const [compact, setCompact] = createSignal(
    typeof window === "undefined" ? false : window.matchMedia(query).matches,
  );

  onMount(() => {
    const media = window.matchMedia(query);
    setCompact(media.matches);
    const listener = (event: MediaQueryListEvent) => setCompact(event.matches);
    media.addEventListener("change", listener);
    onCleanup(() => media.removeEventListener("change", listener));
  });

  return compact;
}
