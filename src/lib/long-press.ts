/**
 * Long-press detection for a touch target.
 *
 * The pointer has to stay within a few pixels for the whole delay: a scroll
 * that starts on the bubble must not open the menu halfway down the list. The
 * returned handlers are plain callbacks, so a caller can attach them to any
 * element (or to a `<Bubble>`), and tests can drive them without a browser.
 */
export function createLongPress(onLongPress: () => void, delayMs = 450, tolerancePx = 10) {
  let timer: number | undefined;
  let origin: { x: number; y: number } | undefined;

  const cancel = () => {
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timer = undefined;
    }
    origin = undefined;
  };

  return {
    start(event: PointerEvent) {
      // A mouse gets the desktop menu; this is for fingers and pens.
      if (event.pointerType === "mouse") return;
      cancel();
      origin = { x: event.clientX, y: event.clientY };
      timer = window.setTimeout(() => {
        timer = undefined;
        onLongPress();
      }, delayMs);
    },
    move(event: PointerEvent) {
      if (!origin || timer === undefined) return;
      if (Math.hypot(event.clientX - origin.x, event.clientY - origin.y) > tolerancePx) {
        cancel();
      }
    },
    cancel,
  };
}
