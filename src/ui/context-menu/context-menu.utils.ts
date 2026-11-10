import { getSide, getTransformOrigin, toPlacement, type Placement, type Side } from "../lib";
import { createChangeEventDetails as createSharedChangeEventDetails } from "../utils";
import type {
  ContextMenuAlign,
  ContextMenuChangeEventDetails,
  ContextMenuChangeEventReason,
  ContextMenuSide,
  ContextMenuSubmenuContextValue,
} from "./context-menu.types";

type Dir = "ltr" | "rtl" | "auto" | undefined;

/** 逻辑方向 -> 物理方向(LTR 下 inline-start=left,inline-end=right;RTL 相反) */
function resolveLogicalSide(side: ContextMenuSide, dir: Dir): Side {
  const physical = toPlacement(side, "center", dir);
  return getSide(physical);
}

/**
 * 把 Base UI 风格的 `side` + `align` 拆成核心定位库统一的 `Placement`。
 * `align="center"` 时不带后缀(即居中)。
 *
 * 换算本身复用 `~/lib/positioner` 的 `toPlacement`,此处只做 ContextMenu 侧
 * 类型(`ContextMenuAlign`)到通用 `Alignment` 的收窄。
 */
export function toContextMenuPlacement(
  side: ContextMenuSide,
  align: ContextMenuAlign,
  dir: Dir,
): Placement {
  return toPlacement(side, align === "center" ? "center" : align, dir);
}

/**
 * 根据最终生效的 placement 计算动画缩放原点(贴在锚点上的那个角)。
 * 实现统一由 `~/lib/positioner` 提供(Popover/Tooltip/HoverCard 同一份)。
 */
export function getContextMenuTransformOrigin(placement: Placement): string {
  return getTransformOrigin(placement);
}

/**
 * 计算写回 DOM 的 `data-side`。若用户传入的是逻辑方向且最终没有翻转到另一侧,
 * 依旧回写逻辑值,这样 shadcn 的 `data-[side=inline-end]:...` 样式才能命中。
 */
export function resolveContextMenuDataSide(
  requested: ContextMenuSide,
  placement: Placement,
  dir: Dir,
): ContextMenuSide {
  const actual = getSide(placement);
  if (requested === "inline-start" || requested === "inline-end") {
    const expected = resolveLogicalSide(requested, dir);
    if (expected === actual) return requested;
  }
  return actual;
}

/** 生成对齐 Base UI 的 ChangeEventDetails(实现复用 `~/utils`) */
export function createChangeEventDetails<R extends string = ContextMenuChangeEventReason>(
  reason: R,
  event?: Event,
  trigger?: Element,
): ContextMenuChangeEventDetails<R> {
  return createSharedChangeEventDetails(reason, event, trigger);
}

/**
 * 沿父链取消所有祖先子菜单已排期的关闭。
 * 多级菜单里,鼠标从父级内容移到子级内容时,父级内容的 pointerleave 会给父级
 * 排一个延迟关闭;子级内容的 pointerenter 必须把祖先的定时器一并取消,
 * 否则父级会在 grace 后关闭,连带整棵菜单树消失。
 */
export function cancelSubmenuCloseChain(submenu: ContextMenuSubmenuContextValue | undefined): void {
  let current = submenu;
  while (current) {
    current.cancelClose();
    current = current.parentPopup.submenu;
  }
}
