/** 触摸长按唤起菜单的时间阈值（毫秒），与 Base UI 保持一致 */
export const LONG_PRESS_DELAY = 500;
/** 长按期间允许的手指抖动距离（像素），超过即取消 */
export const LONG_PRESS_MOVE_TOLERANCE = 10;

/**
 * 触发器"唤起菜单"的判定与锚点计算。
 *
 * 单一职责：只回答"这次事件该不该唤起""锚点坐标取哪里"，
 * 不注册事件、不持有定时器状态。
 */

/**
 * 键盘唤起：菜单键（ContextMenu）或 Shift+F10。
 * 两者都是无障碍规范里"打开上下文菜单"的标准按键。
 */
export function isContextMenuKey(event: KeyboardEvent): boolean {
  return event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");
}

/**
 * 指针长按：只处理触摸/手写笔。
 * 鼠标右键由原生 `contextmenu` 事件负责，再处理一次会重复打开。
 */
export function isLongPressPointer(event: PointerEvent): boolean {
  return event.pointerType !== "mouse";
}

/** 手指是否已抖出容差（超过容差即取消长按，让用户能正常滚动） */
export function isBeyondTolerance(
  origin: { x: number; y: number },
  x: number,
  y: number,
  tolerance: number = LONG_PRESS_MOVE_TOLERANCE,
): boolean {
  return Math.hypot(x - origin.x, y - origin.y) > tolerance;
}

/** 键盘唤起时的锚点：触发器矩形的中心（菜单贴着触发器弹出） */
export function menuAnchorFromRect(rect: {
  left: number;
  top: number;
  width: number;
  height: number;
}): { x: number; y: number } {
  return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
}
