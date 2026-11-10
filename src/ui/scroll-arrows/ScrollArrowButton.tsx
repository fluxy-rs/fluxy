import { Show } from "solid-js";
import { ChevronDown, ChevronUp } from "lucide-solid";
import { clsx } from "../utils";
import type { ScrollArrowButtonProps } from "./ScrollArrows.types";

/**
 * 单个滚动提示箭头，覆盖在列表上/下沿。
 *
 * **纯装饰层**：`pointer-events-none` + `aria-hidden`，不参与指针命中，也不持有任何
 * 定时器与 rAF——悬停滚动由 `useHoverScroll` 在滚动容器上按指针坐标驱动
 * （见 DESIGN.md §3）。因此这里不会出现"箭头显隐 ↔ pointerenter 互相触发"的反馈循环，
 * 也不会抢走列表首/尾选项的点击。
 */
export function ScrollArrowButton(props: ScrollArrowButtonProps) {
  return (
    <Show when={props.visible}>
      <div
        aria-hidden="true"
        data-slot="scroll-arrow"
        data-direction={props.direction}
        class={clsx(
          "pointer-events-none absolute inset-x-0 z-10 flex h-6 select-none items-center justify-center bg-popover text-muted-foreground [&_svg]:size-4",
          props.direction === "down" ? "bottom-0" : "top-0",
          props.class,
        )}
      >
        {props.direction === "down" ? <ChevronDown /> : <ChevronUp />}
      </div>
    </Show>
  );
}
