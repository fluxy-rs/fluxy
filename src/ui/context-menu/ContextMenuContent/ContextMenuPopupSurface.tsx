import { createEffect, createSignal, onCleanup, Show, type Accessor, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { getAlignment } from "../../lib";
import { clsx } from "../../utils";
import { ScrollArrows } from "../../scroll-arrows";
import type { ContextMenuSide } from "../context-menu.types";
import { ContextMenuPopupContext } from "../context-menu.context";
import { getContextMenuTransformOrigin, resolveContextMenuDataSide } from "../context-menu.utils";
import type { ContextMenuPopupRuntime } from "./useContextMenuContent";

/**
 * popup 的视觉样式,直接对齐 shadcn base 版 `ContextMenuContent`。
 * 默认圆角用 base-nova 的 rounded-lg,与本项目 Popover/Select 的观感一致。
 */
const CONTENT_CLASS = clsx(
  "z-50 max-h-(--available-height) min-w-36 origin-(--transform-origin) overflow-x-hidden overflow-y-auto rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10 duration-100 outline-none",
  "no-scrollbar [&::-webkit-scrollbar]:hidden",
  "data-[side=bottom]:slide-in-from-top-2 data-[side=inline-end]:slide-in-from-left-2 data-[side=inline-start]:slide-in-from-right-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
  "data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95",
  "data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
);

export interface ContextMenuPopupSurfaceProps {
  runtime: ContextMenuPopupRuntime;
  /** popup 上的 data-slot 值(根菜单 / 子菜单不同) */
  dataSlot: string;
  /** positioner 上的 data-slot 值,默认 context-menu-positioner */
  positionerSlot?: string;
  contentId: string;
  open: Accessor<boolean>;
  side: Accessor<ContextMenuSide>;
  dir: Accessor<"ltr" | "rtl" | "auto" | undefined>;
  registerMenuElement: (el: HTMLElement) => () => void;
  class?: string;
  style?: JSX.CSSProperties;
  onKeyDown?: (e: KeyboardEvent) => void;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
  children: JSX.Element;
  /** 其余透传到 popup 元素的属性 */
  rest: Record<string, any>;
}

/**
 * 菜单浮层的展示骨架,根菜单与子菜单共用。
 *
 * 结构分两层(与 Popover/Select 一致):外层只负责 translate 定位,内层负责
 * 视觉样式与进出场动画。同一个元素不能既用 transform 定位又用 transform 做
 * 缩放动画,否则动画播放期间会把定位 transform 整个顶掉。
 */
export function ContextMenuPopupSurface(props: ContextMenuPopupSurfaceProps) {
  const runtime = props.runtime;

  // 打开时延迟一帧再触发进场动画,给首次定位计算留出时间,避免"先出现在
  // 旧位置再跳过去"。
  const [animationState, setAnimationState] = createSignal<"open" | "closed">("closed");

  createEffect(() => {
    if (props.open()) {
      const raf = requestAnimationFrame(() => setAnimationState("open"));
      onCleanup(() => cancelAnimationFrame(raf));
    } else {
      setAnimationState("closed");
    }
  });

  // popup 挂载后聚焦它并高亮第一项(键盘事件依赖 popup 持有焦点)。
  createEffect(() => {
    if (props.open() && runtime.popupEl()) runtime.setupOnOpen();
  });

  const placement = () => runtime.placement();
  const dataSide = () => resolveContextMenuDataSide(props.side(), placement(), props.dir());
  const dataAlign = () => getAlignment(placement()) ?? "center";
  const transformOrigin = () => getContextMenuTransformOrigin(placement());

  return (
    <Show when={props.open()}>
      <Portal>
        <div
          ref={(el) => {
            runtime.setPositionerEl(el);
            const unregister = props.registerMenuElement(el);
            onCleanup(() => {
              unregister();
              runtime.setPositionerEl(undefined);
            });
          }}
          data-slot={props.positionerSlot ?? "context-menu-positioner"}
          data-side={dataSide()}
          data-align={dataAlign()}
          class="isolate z-50 outline-none"
          style={{
            ...runtime.pos.floatingStyles(),
            "z-index": 50,
            opacity: runtime.pos.isPositioned() ? 1 : 0,
          }}
        >
          <div
            ref={(el) => {
              runtime.setPopupEl(el);
              onCleanup(() => runtime.setPopupEl(undefined));
            }}
            id={props.contentId}
            role="menu"
            tabIndex={-1}
            data-slot={props.dataSlot}
            data-open={animationState() === "open" ? "" : undefined}
            data-side={dataSide()}
            data-align={dataAlign()}
            aria-activedescendant={runtime.popupCtx.activeId()}
            class={clsx(CONTENT_CLASS, props.class)}
            style={{
              "transform-origin": transformOrigin(),
              "--transform-origin": transformOrigin(),
              "max-height":
                runtime.availableHeight() != null ? `${runtime.availableHeight()}px` : undefined,
              "--available-height":
                runtime.availableHeight() != null ? `${runtime.availableHeight()}px` : undefined,
              ...(typeof props.style === "object" ? props.style : undefined),
            }}
            onKeyDown={(e) => {
              props.onKeyDown?.(e);
              runtime.onKeyDown(e);
            }}
            onPointerEnter={() => props.onPointerEnter?.()}
            onPointerLeave={() => props.onPointerLeave?.()}
            {...props.rest}
          >
            <ContextMenuPopupContext.Provider value={runtime.popupCtx}>
              {props.children}
            </ContextMenuPopupContext.Provider>
          </div>
          <ScrollArrows target={runtime.popupEl} upClass="rounded-t-lg" downClass="rounded-b-lg" />
        </div>
      </Portal>
    </Show>
  );
}
