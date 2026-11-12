import { Show, onCleanup, splitProps } from "solid-js";
import { Portal } from "solid-js/web";
import { clsx } from "../../utils";
import { getAlignment, getSide } from "../../lib";
import { usePopoverContent } from "./usePopoverContent";
import { getPopoverTransformOrigin } from "./PopoverContent.utils";
import type { PopoverContentProps } from "./PopoverContent.types";

/**
 * Popover 的浮层内容。挂 Portal 到 body，用 position:fixed 定位。
 * 当前实现打开时挂载、关闭时直接卸载；进场动画由 data-open + RAF 驱动。
 */
export function PopoverContent(props: PopoverContentProps) {
  const { ctx, pos, animationState } = usePopoverContent(() => props);

  const [local, rest] = splitProps(props, [
    "side",
    "align",
    "sideOffset",
    "alignOffset",
    "collisionPadding",
    "class",
    "classList",
    "style",
    "children",
  ]);

  return (
    <Show when={ctx.open()}>
      <Portal>
        {/*
          外层：只负责定位（position:fixed 来自 floatingStyles），不参与动画。
          内层：真正的视觉样式 + 进出场动画。分层的原因和 Select/Tooltip 相同：
          同一个元素不能同时用 transform 做定位和做动画。
        */}
        <div
          ref={(el) => {
            ctx.setFloating(el);
            onCleanup(() => ctx.setFloating(undefined));
          }}
          data-placement={pos.placement()}
          style={{
            ...pos.floatingStyles(),
            "z-index": 50,
            opacity: pos.isPositioned() ? 1 : 0,
          }}
        >
          <div
            id={ctx.contentId}
            role="dialog"
            data-slot="popover-content"
            data-open={animationState() === "open" ? "" : undefined}
            data-closed={animationState() === "closed" ? "" : undefined}
            data-side={getSide(pos.placement())}
            data-align={getAlignment(pos.placement()) ?? "center"}
            style={{
              "transform-origin": getPopoverTransformOrigin(pos.placement()),
              ...(typeof local.style === "object" ? local.style : undefined),
            }}
            class={clsx(
              "z-50 flex max-h-[calc(100dvh-1rem)] w-72 max-w-[calc(100vw-1rem)] flex-col gap-2.5 overflow-x-hidden overflow-y-auto overscroll-contain rounded-lg bg-popover p-2.5 text-sm text-popover-foreground shadow-md ring-1 ring-foreground/10 outline-hidden duration-100",
              "data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
              "data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95",
              "data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
              local.class,
            )}
            classList={local.classList}
            {...rest}
          >
            {local.children}
          </div>
        </div>
      </Portal>
    </Show>
  );
}
