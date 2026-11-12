/**
 * Long-press detection for a touch target.
 *
 * The pointer has to stay within a few pixels for the whole delay: a scroll
 * that starts on the bubble must not open the menu halfway down the list. The
 * returned handlers are plain callbacks, so a caller can attach them to any
 * element (or to a `<Bubble>`), and tests can drive them without a browser.
 */
export function createLongPress(
  onLongPress: (event: PointerEvent) => void,
  delayMs = 450,
  tolerancePx = 10,
) {
  let timer: number | undefined;
  let origin: { x: number; y: number } | undefined;
  // The menu is placed from the element that was pressed, so the event has to
  // survive until the timer fires.
  let pressed: PointerEvent | undefined;

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
      pressed = event;
      origin = { x: event.clientX, y: event.clientY };
      timer = window.setTimeout(() => {
        timer = undefined;
        if (pressed) onLongPress(pressed);
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
