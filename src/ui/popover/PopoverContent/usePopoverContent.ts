import { createSignal, createMemo, createEffect, onCleanup } from "solid-js";
import { createPositioner, type Positioner } from "../../lib";
import { usePopoverContext } from "../Popover/Popover.context";
import { createPopoverMiddleware, toPopoverPlacement } from "./PopoverContent.utils";
import type { PopoverContentProps } from "./PopoverContent.types";

export interface UsePopoverContentResult {
  ctx: ReturnType<typeof usePopoverContext>;
  pos: Positioner;
  isVisible: () => boolean;
  /** 驱动 data-open/data-closed 的动画状态；打开时比 isVisible 晚一帧。 */
  animationState: () => "open" | "closed";
}

export function usePopoverContent(props: () => PopoverContentProps): UsePopoverContentResult {
  const ctx = usePopoverContext("PopoverContent");

  const pos = createPositioner(ctx.reference, ctx.floating, {
    placement: () => toPopoverPlacement(props().side ?? "bottom", props().align ?? "center"),
    strategy: "fixed",
    middleware: () =>
      createPopoverMiddleware({
        sideOffset: props().sideOffset ?? 4,
        alignOffset: props().alignOffset ?? 0,
        collisionPadding: props().collisionPadding ?? 8,
      }),
  });

  const isVisible = createMemo(() => ctx.open() && !pos.middlewareData().hide?.referenceHidden);

  // 打开时延迟一帧再触发动画 class，给定位计算留出时间。
  const [animationState, setAnimationState] = createSignal<"open" | "closed">("closed");

  createEffect(() => {
    if (isVisible()) {
      const raf = requestAnimationFrame(() => setAnimationState("open"));
      onCleanup(() => cancelAnimationFrame(raf));
    } else {
      setAnimationState("closed");
    }
  });

  // 打开期间绑定 document 级监听：点击外部关闭 + Escape 关闭。
  createEffect(() => {
    if (!ctx.open()) return;

    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      const targetElement = target instanceof Element ? target : target?.parentElement;
      const insideNestedFloating = !!targetElement?.closest('[data-slot="select-content"]');
      if (
        !insideNestedFloating &&
        !ctx.reference()?.contains(target) &&
        !ctx.floating()?.contains(target)
      ) {
        ctx.closePopover();
        e.stopPropagation();
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        ctx.closePopover();
      }
    };

    document.addEventListener("pointerdown", onPointerDown, { capture: true });
    document.addEventListener("keydown", onKeyDown);
    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown, {
        capture: true,
      });
      document.removeEventListener("keydown", onKeyDown);
    });
  });

  return { ctx, pos, isVisible, animationState };
}
