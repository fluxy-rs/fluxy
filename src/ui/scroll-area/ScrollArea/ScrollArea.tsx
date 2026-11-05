import { createSignal, splitProps } from "solid-js";
import type { ScrollAreaProps } from "./ScrollArea.types";
import { clsx } from "../../utils";
import { ScrollBar } from "../ScrollBar";
import { ScrollAreaContext } from "./ScrollArea.context";
import { useScrollAreaMetrics } from "./useScrollAreaMetrics";

/**
 *
 */
export const ScrollArea = (props: ScrollAreaProps) => {
  const [local, others] = splitProps(props, [
    "orientation",
    "class",
    "classList",
    "children",
    "aria-label",
  ]);

  const [hovering, setHovering] = createSignal(false);
  const [dragging, setDragging] = createSignal<"vertical" | "horizontal" | null>(null);

  const metrics = useScrollAreaMetrics();

  const overflowClass = () => {
    if (local.orientation === "vertical") {
      return "overflow-y-scroll overflow-x-hidden";
    }
    if (local.orientation === "horizontal") {
      return "overflow-x-scroll overflow-y-hidden";
    }
    return "overflow-scroll";
  };

  return (
    <div
      data-slot="scroll-area"
      class={clsx("relative", local.class, local.classList)}
      classList={local.classList}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      {...others}
    >
      <ScrollAreaContext.Provider
        value={{
          hovering,
          viewportRef: metrics.viewportRef,
          dragging,
          setDragging,
          vertical: metrics.vertical,
          horizontal: metrics.horizontal,
        }}
      >
        <div
          ref={metrics.setup}
          data-slot="scroll-area-viewport"
          role="region"
          aria-label={local["aria-label"] ?? "Scrollable content"}
          class={clsx(
            "size-full rounded-[inherit] transition-[color,box-shadow] outline-none",
            // Native scrollbars hidden via CSS; custom ones provided below
            "scrollbar-none [&::-webkit-scrollbar]:hidden",
            // Focus ring — meets WCAG 2.4.7
            "focus-visible:ring-2 focus-visible:ring-offset-1",
            "focus-visible:ring-ring/50",
            overflowClass(),
          )}
        >
          {local.children}
        </div>
        <ScrollBar orientation={local.orientation} />
      </ScrollAreaContext.Provider>
    </div>
  );
};
