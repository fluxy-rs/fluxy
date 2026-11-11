import { createEffect, onCleanup, type Accessor } from "solid-js";

/** 悬停到边缘带后、开始持续滚动前的停留时间（ms）：避免只是掠过就滚起来 */
export const HOVER_SCROLL_DELAY = 150;
/**
 * 边缘判定带高度（px），与箭头高度一致（`h-6` = 24px）。
 * 改箭头高度时这里也要跟着改。
 */
export const HOVER_BAND_PX = 24;

export interface UseHoverScrollOptions {
  /** 滚动容器 */
  target: Accessor<HTMLElement | undefined>;
  /** 上方 / 下方是否还有内容（与箭头的显隐同源，来自 `useScrollEdges`） */
  canScrollUp: Accessor<boolean>;
  canScrollDown: Accessor<boolean>;
  /** 是否启用 */
  enabled: Accessor<boolean>;
}

/**
 * 悬停在滚动容器的上/下边缘带时持续滚动。
 *
 * 为什么监听**容器**而不是箭头：箭头是 `pointer-events-none` 的装饰层（见 DESIGN.md §2），
 * 一旦让它参与指针命中，就会抢走列表首/尾选项的点击。改为在容器上按指针坐标判断
 * "是否停在边缘带内"，于是：
 * - 不需要箭头命中指针，边缘点击照常落到列表项上；
 * - 滚动由指针位置驱动，与箭头是否可见无关，**结构上消除**了
 *   "箭头显隐 ↔ pointerenter 互相触发" 的反馈循环；
 * - 指针按下 / 滚轮 / 键盘随时夺回控制权。
 */
export function useHoverScroll(options: UseHoverScrollOptions): void {
  let frame: number | undefined;
  let timer: number | undefined;
  let direction: "up" | "down" | undefined;

  const stopLoop = () => {
    if (frame !== undefined) {
      cancelAnimationFrame(frame);
      frame = undefined;
    }
    direction = undefined;
  };

  const stop = () => {
    if (timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
    stopLoop();
  };

  onCleanup(stop);

  createEffect(() => {
    const element = options.target();
    const enabled = options.enabled();
    if (!element || !enabled) return;

    const loop = () => {
      frame = undefined;
      // 每次循环都重新确认方向仍然有效（指针可能已离开，或内容已到边界）
      const current = direction;
      if (current === undefined) return;

      const step = Math.max(4, element.clientHeight / 24);
      const before = element.scrollTop;
      element.scrollTop = before + (current === "down" ? step : -step);
      // 到边界（或内容放得下）时 scrollTop 不再变化 → 停止
      if (element.scrollTop === before) {
        stopLoop();
        return;
      }
      frame = requestAnimationFrame(loop);
    };

    const start = (next: "up" | "down") => {
      stopLoop();
      direction = next;
      frame = requestAnimationFrame(loop);
    };

    /** 进入边缘带：停留一段时间后才开始滚动；期间指针一动就重新计时 */
    const schedule = (next: "up" | "down") => {
      if (direction === next) return;
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
      stopLoop();
      timer = window.setTimeout(() => {
        timer = undefined;
        start(next);
      }, HOVER_SCROLL_DELAY);
    };

    const onPointerMove = (event: PointerEvent) => {
      const rect = element.getBoundingClientRect();
      const fromTop = event.clientY - rect.top;
      const fromBottom = rect.bottom - event.clientY;

      if (fromTop <= HOVER_BAND_PX && options.canScrollUp()) schedule("up");
      else if (fromBottom <= HOVER_BAND_PX && options.canScrollDown()) schedule("down");
      else stop();
    };

    element.addEventListener("pointermove", onPointerMove);
    element.addEventListener("pointerleave", stop);
    // 按下即停：否则指针下的选项会在按下与抬起之间继续移动，导致点不中
    element.addEventListener("pointerdown", stop);
    element.addEventListener("touchstart", stop, { passive: true });
    element.addEventListener("wheel", stop, { passive: true });
    element.addEventListener("keydown", stop);
    onCleanup(() => {
      stop();
      element.removeEventListener("pointermove", onPointerMove);
      element.removeEventListener("pointerleave", stop);
      element.removeEventListener("pointerdown", stop);
      element.removeEventListener("touchstart", stop);
      element.removeEventListener("wheel", stop);
      element.removeEventListener("keydown", stop);
    });
  });
}
