import { createMemo, createSignal, mergeProps, splitProps } from "solid-js";
import type { ScrollBarProps } from "./ScrollBar.types";
import { clsx } from "../../utils";
import { useScrollAreaContext } from "../ScrollArea";
import {
  computeAriaValueNow,
  computeThumbStyle,
  isScrollBarVisible,
  isVertical,
} from "./ScrollBar.utils";
import { useScrollBarInteraction } from "./useScrollBarInteraction";

/**
 *
 */
export const ScrollBar = (props: ScrollBarProps) => {
  const ctx = useScrollAreaContext();

  const merged = mergeProps({ orientation: "vertical" } as const, props);

  const [local, others] = splitProps(merged, ["orientation", "class", "classList"]);

  const [trackEl, setTrackEl] = createSignal<HTMLDivElement>();
  const [active, setActive] = createSignal(false);

  const orientation = () => local.orientation;
  const isVerticalAxis = () => isVertical(orientation());

  const metrics = createMemo(() => (isVerticalAxis() ? ctx.vertical() : ctx.horizontal()));

  const thumbStyle = createMemo(() => {
    const el = trackEl();
    const trackSize = el ? (isVerticalAxis() ? el.clientHeight : el.clientWidth) : 0;
    return computeThumbStyle(orientation(), metrics(), trackSize);
  });

  const ariaValueNow = createMemo(() => computeAriaValueNow(metrics()));

  const visible = createMemo(() =>
    isScrollBarVisible(ctx.hovering(), ctx.dragging(), orientation(), active()),
  );

  const interaction = useScrollBarInteraction({
    track: trackEl,
    viewport: ctx.viewportRef,
    orientation,
    onDragChange: (dragging) => {
      ctx.setDragging(dragging ? orientation() : null);
      setActive(dragging);
    },
  });

  return (
    <div
      ref={setTrackEl}
      data-slot="scroll-area-scrollbar"
      // WCAG 4.1.2: expose as scrollbar widget
      role="scrollbar"
      aria-controls="scroll-area-viewport"
      aria-orientation={orientation()}
      aria-valuenow={ariaValueNow()}
      aria-valuemin={0}
      aria-valuemax={100}
      tabIndex={0}
      onPointerDown={interaction.onTrackPointerDown}
      onKeyDown={interaction.onKeyDown}
      data-orientation={orientation()}
      data-hovering={ctx.hovering() ? "" : undefined}
      class={clsx(
        // Layout
        "absolute touch-none select-none transition-opacity duration-150",
        // Opacity — visible on hover/drag, hidden otherwise
        visible() ? "opacity-100" : "opacity-0",
        // Axis-specific positioning & size
        isVerticalAxis()
          ? "right-0 top-0 h-full w-2.5 border-l border-l-transparent p-px flex flex-col"
          : "bottom-0 left-0 w-full h-2.5 border-t border-t-transparent p-px flex flex-row",
        // Keyboard focus ring
        "outline-none focus-visible:ring-2 focus-visible:ring-neutral-400/70 dark:focus-visible:ring-neutral-500/70",
        local.class,
        local.classList,
      )}
      classList={local.classList}
      {...others}
    >
      <div
        data-slot="scroll-area-thumb"
        onPointerDown={interaction.onThumbPointerDown}
        data-orientation={orientation()}
        class={clsx(
          "relative rounded-full bg-border",
          // Hover / active state
          "hover:bg-neutral-400 dark:hover:bg-white/30",
          active() ? "bg-neutral-500 dark:bg-white/40" : "",
          "transition-colors duration-100",
          // Ensure thumb is always reachable
          isVerticalAxis() ? "min-h-5 w-full" : "min-w-5 h-full",
        )}
        style={thumbStyle()}
      />
    </div>
  );
};
