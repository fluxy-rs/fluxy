import { onCleanup } from "solid-js";
import { usePopoverContext } from "../Popover/Popover.context";
import type { PopoverTriggerProps } from "./PopoverTrigger.types";
import type { ValidComponent } from "solid-js";

export interface UsePopoverTriggerResult {
  ctx: ReturnType<typeof usePopoverContext>;
  isDisabled: () => boolean;
  /**
   * 把开关浮层相关的原生事件监听器绑定到真正的 DOM 元素上（通过 ref 调用）。
   * 使用 addEventListener 而不是 JSX 的 onXxx prop，是为了不占用这些 prop 名——
   * 调用方自己传的 onClick 不会被覆盖。
   */
  attachListeners: (el: Element) => void;
}

export function usePopoverTrigger<T extends ValidComponent>(
  props: () => PopoverTriggerProps<T>,
): UsePopoverTriggerResult {
  const ctx = usePopoverContext("PopoverTrigger");

  const isDisabled = () => ctx.disabled() || !!props().disabled;

  const attachListeners = (el: Element) => {
    const onClick = () => {
      if (isDisabled()) return;
      ctx.togglePopover();
    };

    const onKeyDown = (e: Event) => {
      if (isDisabled()) return;
      const keyboardEvent = e as KeyboardEvent;

      if (!ctx.open()) {
        if (["ArrowDown", "ArrowUp", "Enter", " "].includes(keyboardEvent.key)) {
          keyboardEvent.preventDefault();
          ctx.openPopover();
        }
        return;
      }

      if (keyboardEvent.key === "Escape") {
        keyboardEvent.stopPropagation();
        ctx.closePopover();
      }
    };

    el.addEventListener("click", onClick);
    el.addEventListener("keydown", onKeyDown);

    onCleanup(() => {
      el.removeEventListener("click", onClick);
      el.removeEventListener("keydown", onKeyDown);
    });
  };

  return { ctx, isDisabled, attachListeners };
}
