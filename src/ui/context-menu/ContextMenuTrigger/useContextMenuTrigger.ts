import { onCleanup } from "solid-js";
import { useContextMenuContext } from "../ContextMenu/ContextMenu.context";
import { createLongPress } from "./context-menu.long-press";
import {
  LONG_PRESS_DELAY,
  LONG_PRESS_MOVE_TOLERANCE,
  isContextMenuKey,
  isLongPressPointer,
  menuAnchorFromRect,
} from "./context-menu.trigger.utils";

export interface UseContextMenuTriggerResult {
  ctx: ReturnType<typeof useContextMenuContext>;
  /**
   * 把右键 / 长按 / 键盘唤起菜单的原生事件绑定到真正的 DOM 元素上（通过 ref 调用）。
   * 用 addEventListener 而不是 JSX 的 onXxx prop，是为了不占用这些 prop 名——
   * 调用方可以在触发器上自由传自己的 `onClick` / `onContextMenu`，两者互不覆盖。
   * 这与 TooltipTrigger 是同一套约定。
   */
  attachListeners: (el: Element) => void;
}

/**
 * ContextMenuTrigger 的事件接线：
 * - contextmenu（右键）：锚定到鼠标坐标；
 * - pointerdown（触摸/手写笔）长按 500ms：锚定到按下坐标（手势本身在
 *   `context-menu.long-press`）；
 * - keydown（Shift+F10 / 菜单键）：锚定到触发器中心，满足键盘可达性。
 *
 * 判定与锚点计算都在 `context-menu.trigger.utils`，本 hook 只做绑定与清理。
 */
export function useContextMenuTrigger(): UseContextMenuTriggerResult {
  const ctx = useContextMenuContext("ContextMenuTrigger");

  const attachListeners = (el: Element) => {
    // 长按到点时要"回到按下时的坐标与事件"，因此在闭包里记住这一次按下
    let press: { x: number; y: number; event: PointerEvent } | undefined;

    const longPress = createLongPress({
      delayMs: LONG_PRESS_DELAY,
      tolerancePx: LONG_PRESS_MOVE_TOLERANCE,
      onTrigger: () => {
        if (!press) return;
        ctx.openAt(press.x, press.y, el, press.event, "trigger-press");
      },
    });

    const onContextMenu = (event: Event) => {
      if (ctx.disabled()) return;
      const e = event as MouseEvent;
      e.preventDefault();
      ctx.openAt(e.clientX, e.clientY, el, e, "trigger-press");
    };

    const onPointerDown = (event: Event) => {
      const e = event as PointerEvent;
      // 鼠标右键由 contextmenu 处理，这里只负责触摸/手写笔长按
      if (!isLongPressPointer(e) || ctx.disabled()) return;
      press = { x: e.clientX, y: e.clientY, event: e };
      longPress.start(e.clientX, e.clientY);
    };

    const onPointerMove = (event: Event) => {
      const e = event as PointerEvent;
      longPress.move(e.clientX, e.clientY);
    };

    const onPointerEnd = () => {
      press = undefined;
      longPress.cancel();
    };

    const onKeyDown = (event: Event) => {
      if (ctx.disabled()) return;
      const e = event as KeyboardEvent;
      if (!isContextMenuKey(e)) return;
      e.preventDefault();
      const anchor = menuAnchorFromRect(el.getBoundingClientRect());
      ctx.openAt(anchor.x, anchor.y, el, e, "trigger-press");
    };

    el.addEventListener("contextmenu", onContextMenu);
    el.addEventListener("pointerdown", onPointerDown);
    el.addEventListener("pointermove", onPointerMove);
    el.addEventListener("pointerup", onPointerEnd);
    el.addEventListener("pointercancel", onPointerEnd);
    el.addEventListener("keydown", onKeyDown);

    onCleanup(() => {
      longPress.cancel();
      el.removeEventListener("contextmenu", onContextMenu);
      el.removeEventListener("pointerdown", onPointerDown);
      el.removeEventListener("pointermove", onPointerMove);
      el.removeEventListener("pointerup", onPointerEnd);
      el.removeEventListener("pointercancel", onPointerEnd);
      el.removeEventListener("keydown", onKeyDown);
    });
  };

  return { ctx, attachListeners };
}
