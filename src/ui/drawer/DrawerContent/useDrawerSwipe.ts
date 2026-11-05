import { createSignal, onCleanup } from "solid-js";
import { isHorizontal } from "./DrawerContent.styles";
import {
  canSwipeFrom,
  clampSwipeOffset,
  isClosingSwipe,
  resolveDismissThreshold,
} from "./drawer-swipe.utils";
import type { DrawerContextValue } from "../Drawer/Drawer.types";

/**
 *
 */
export function useDrawerSwipe(ctx: DrawerContextValue) {
  const [dragging, setDragging] = createSignal(false);
  const [dragX, setDragX] = createSignal(0);
  const [dragY, setDragY] = createSignal(0);

  const direction = () => ctx.swipeDirection();
  const horizontal = () => isHorizontal(direction());

  const attach = (popup: HTMLElement) => {
    let startX = 0;
    let startY = 0;

    const reset = () => {
      setDragging(false);
      setDragX(0);
      setDragY(0);
    };

    const finish = (event: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      if (!dragging()) return;

      const dir = direction();
      const panelSize = horizontal() ? popup.offsetWidth : popup.offsetHeight;
      const threshold = resolveDismissThreshold(panelSize);
      const delta = horizontal() ? dragX() : dragY();

      if (isClosingSwipe(dir, delta, threshold)) {
        setDragging(false);
        setDragX(0);
        setDragY(0);
        ctx.setOpen(false, "swipe", event);
      } else {
        reset();
      }
    };

    const onMove = (event: PointerEvent) => {
      if (!dragging()) return;
      const dir = direction();
      if (horizontal()) {
        setDragX(clampSwipeOffset(dir, event.clientX - startX));
      } else {
        setDragY(clampSwipeOffset(dir, event.clientY - startY));
      }
    };

    const onUp = (event: PointerEvent) => finish(event);

    const onTouchMove = (event: TouchEvent) => {
      if (!dragging()) return;
      const offset = horizontal() ? dragX() : dragY();
      if (offset !== 0) event.preventDefault();
    };

    const onPointerDown = (event: PointerEvent) => {
      if (event.button !== 0) return;
      const target = event.target as Element | null;
      if (!target || !canSwipeFrom(target, popup, horizontal(), direction())) {
        return;
      }
      startX = event.clientX;
      startY = event.clientY;
      setDragging(true);
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onUp);
    };

    popup.addEventListener("pointerdown", onPointerDown);
    popup.addEventListener("touchmove", onTouchMove, { passive: false });
    onCleanup(() => {
      popup.removeEventListener("pointerdown", onPointerDown);
      popup.removeEventListener("touchmove", onTouchMove);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    });
  };

  return { dragging, dragX, dragY, attach };
}
