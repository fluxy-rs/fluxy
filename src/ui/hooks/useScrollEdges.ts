import { createEffect, createSignal, onCleanup, type Accessor } from "solid-js";

export interface ScrollEdges {
  canScrollUp: Accessor<boolean>;
  canScrollDown: Accessor<boolean>;
  refresh: () => void;
}

const ENTER_THRESHOLD = 4;
const EXIT_THRESHOLD = 1;

/**
 *
 *
 */
export function useScrollEdges(target: Accessor<HTMLElement | undefined>): ScrollEdges {
  const [canScrollUp, setCanScrollUp] = createSignal(false);
  const [canScrollDown, setCanScrollDown] = createSignal(false);

  const refresh = () => {
    const el = target();
    if (!el) {
      setCanScrollUp(false);
      setCanScrollDown(false);
      return;
    }
    const top = el.scrollTop;
    const remaining = el.scrollHeight - el.clientHeight - top;
    setCanScrollUp((prev) => (prev ? top > EXIT_THRESHOLD : top > ENTER_THRESHOLD));
    setCanScrollDown((prev) => (prev ? remaining > EXIT_THRESHOLD : remaining > ENTER_THRESHOLD));
  };

  createEffect(() => {
    const el = target();
    if (!el) return;

    refresh();

    const resizeObserver = new ResizeObserver(refresh);
    resizeObserver.observe(el);
    if (el.firstElementChild) resizeObserver.observe(el.firstElementChild);

    const mutationObserver = new MutationObserver(refresh);
    mutationObserver.observe(el, { childList: true, subtree: true });

    el.addEventListener("scroll", refresh, { passive: true });
    onCleanup(() => {
      resizeObserver.disconnect();
      mutationObserver.disconnect();
      el.removeEventListener("scroll", refresh);
    });
  });

  return { canScrollUp, canScrollDown, refresh };
}
