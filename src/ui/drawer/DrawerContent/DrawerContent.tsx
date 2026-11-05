import { Show, createEffect, createSignal, onCleanup, splitProps } from "solid-js";
import { Portal } from "solid-js/web";
import { clsx } from "../../utils";
import { useDrawerContext } from "../Drawer/Drawer.context";
import { DrawerOverlay } from "../DrawerOverlay";
import { DrawerSwipeHandle } from "../DrawerSwipeHandle";
import {
  closedTransform,
  drawerPopupBase,
  drawerPopupDirectionClass,
  isHorizontal,
} from "./DrawerContent.styles";
import type { DrawerContentProps } from "./DrawerContent.types";
import { useDrawerSwipe } from "./useDrawerSwipe";

/**
 */
export function DrawerContent(props: DrawerContentProps) {
  const ctx = useDrawerContext("DrawerContent");
  const [local, rest] = splitProps(props, ["class", "classList", "children"]);
  const swipe = useDrawerSwipe(ctx);
  const [entered, setEntered] = createSignal(false);

  const direction = () => ctx.swipeDirection();
  const horizontal = () => isHorizontal(direction());
  const axis = () => (horizontal() ? "x" : "y");
  const visible = () => ctx.open() && entered();

  createEffect(() => {
    if (ctx.show()) {
      setEntered(false);
      const raf = requestAnimationFrame(() => setEntered(true));
      onCleanup(() => cancelAnimationFrame(raf));
    } else {
      setEntered(false);
    }
  });

  createEffect(() => {
    if (visible()) ctx.popup()?.focus({ preventScroll: true });
  });

  const transform = () => {
    if (swipe.dragging()) {
      return `translate3d(${swipe.dragX()}px, ${swipe.dragY()}px, 0)`;
    }
    return visible() ? "translate3d(0, 0, 0)" : closedTransform(direction());
  };

  return (
    <Show when={ctx.show()}>
      <Portal>
        <Show when={ctx.modal()}>
          <DrawerOverlay />
        </Show>
        <div
          data-slot="drawer-viewport"
          data-modal={ctx.modal() ? "true" : "false"}
          class="pointer-events-none fixed inset-0 z-50 select-none data-[modal=true]:pointer-events-auto"
        >
          <div
            {...rest}
            ref={(el) => {
              ctx.setPopup(el);
              swipe.attach(el);
            }}
            id={ctx.contentId}
            role="dialog"
            aria-modal={ctx.modal() ? "true" : undefined}
            aria-labelledby={ctx.titleId()}
            aria-describedby={ctx.descriptionId()}
            tabindex={-1}
            data-slot="drawer-popup"
            data-state={ctx.open() ? "open" : "closed"}
            data-swipe-axis={axis()}
            data-swipe-direction={direction()}
            data-swiping={swipe.dragging() ? "" : undefined}
            style={{
              transform: transform(),
              transition: swipe.dragging()
                ? "none"
                : "transform 450ms cubic-bezier(0.32, 0.72, 0, 1)",
            }}
            class={clsx(drawerPopupBase, drawerPopupDirectionClass(direction()), local.class)}
            classList={local.classList}
            onTransitionEnd={(event) => {
              if (event.target !== event.currentTarget) return;
              if (!ctx.open()) {
                ctx.restoreFocus();
                ctx.setShow(false);
              }
            }}
          >
            <Show when={ctx.showSwipeHandle()}>
              <DrawerSwipeHandle />
            </Show>
            <div
              data-slot="drawer-content"
              class="flex min-h-0 flex-1 flex-col overflow-hidden overscroll-contain"
            >
              {local.children}
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
