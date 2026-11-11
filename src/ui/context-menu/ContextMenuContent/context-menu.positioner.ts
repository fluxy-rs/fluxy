import { createSignal, type Accessor } from "solid-js";
import {
  containingBlockOffset,
  createPositioner,
  flip,
  hide,
  offset,
  shift,
  size,
  type Placement,
  type Positioner,
  type ReferenceElement,
} from "../../lib";
import type { ContextMenuAlign, ContextMenuSide } from "../context-menu.types";
import { toContextMenuPlacement } from "../context-menu.utils";

export interface PopupPositionerOptions {
  /** 定位锚点：根菜单是鼠标坐标的虚拟元素，子菜单是子菜单触发器 */
  reference: Accessor<ReferenceElement | undefined>;
  /** 负责定位的外层元素 */
  positionerElement: Accessor<HTMLElement | undefined>;
  side: Accessor<ContextMenuSide>;
  align: Accessor<ContextMenuAlign>;
  dir: Accessor<"ltr" | "rtl" | "auto" | undefined>;
  sideOffset: Accessor<number>;
  alignOffset: Accessor<number>;
  collisionPadding: Accessor<number>;
}

export interface PopupPositioner {
  pos: Positioner;
  availableHeight: Accessor<number | undefined>;
  placement: Accessor<Placement>;
}

/**
 * 浮层的定位配置。
 *
 * 单一职责：把 `side` / `align` / 偏移 / 碰撞内边距这些 props 翻译成
 * middleware 组合（offset → flip → shift → size → hide → containingBlockOffset），
 * 并把 size 中间件算出的可用高度暴露给调用方（用于 max-height 滚动）。
 *
 * `containingBlockOffset` 放在最后：Portal 里的浮层相对视口定位，
 * 但祖先若有 `transform`，fixed 定位的参考系会变，需要补上这段偏移。
 */
export function createPopupPositioner(options: PopupPositionerOptions): PopupPositioner {
  const [availableHeight, setAvailableHeight] = createSignal<number>();

  const pos = createPositioner(options.reference, options.positionerElement, {
    placement: () => toContextMenuPlacement(options.side(), options.align(), options.dir()),
    strategy: "fixed",
    middleware: () => [
      offset({
        mainAxis: options.sideOffset(),
        crossAxis: options.alignOffset(),
      }),
      flip(),
      shift({ padding: options.collisionPadding() }),
      size({
        padding: options.collisionPadding(),
        apply: ({ availableHeight }) => setAvailableHeight(availableHeight),
      }),
      hide(),
      containingBlockOffset(),
    ],
  });

  return { pos, availableHeight, placement: pos.placement };
}
