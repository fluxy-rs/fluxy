import {
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  onCleanup,
  type JSX,
} from "solid-js";
import { createVirtualElement, type VirtualElement } from "../../lib";
import { useScrollLock } from "../../hooks";
import { ContextMenuContext } from "./ContextMenu.context";
import type { ContextMenuProps } from "./ContextMenu.types";
import type { ContextMenuChangeEventReason, ContextMenuContextValue } from "../context-menu.types";
import { createChangeEventDetails } from "../context-menu.utils";

/** 菜单自身(根菜单/子菜单)的 popup 容器,允许在锁滚动期间继续在菜单内部滚动 */
const MENU_POPUP_SELECTOR =
  '[data-slot="context-menu-content"],[data-slot="context-menu-sub-content"]';

/**
 * ContextMenu 根组件:不渲染任何 DOM,只负责状态管理 + 提供 context。
 *
 * 这是 shadcn 基于 Base UI 的 ContextMenu 的 SolidJS 移植:Base UI 的 Root
 * 同样不渲染自己的 HTML 元素,只作为状态容器。
 *
 * 与普通菜单不同的地方在于"锚点":右键菜单的定位参照是鼠标点击的像素坐标,
 * 因此这里用 `createVirtualElement` 生成一个宽高为 0 的虚拟参照元素,
 * 再交给内容层用 position:fixed 定位(细节见 ContextMenuContent)。
 *
 * @example
 * ```tsx
 * <ContextMenu>
 *   <ContextMenuTrigger>右键点击这里</ContextMenuTrigger>
 *   <ContextMenuContent>
 *     <ContextMenuItem>个人资料</ContextMenuItem>
 *   </ContextMenuContent>
 * </ContextMenu>
 * ```
 */
export function ContextMenu(props: ContextMenuProps): JSX.Element {
  const [internalOpen, setInternalOpen] = createSignal(props.defaultOpen ?? false);
  const open = createMemo(() => (props.open !== undefined ? props.open : internalOpen()));
  const disabled = createMemo(() => !!props.disabled);
  const loopFocus = createMemo(() => props.loopFocus ?? true);
  const orientation = createMemo(() => props.orientation ?? "vertical");
  const highlightItemOnHover = createMemo(() => props.highlightItemOnHover ?? true);
  const modal = createMemo(() => props.modal ?? true);

  // 打开期间锁定页面滚动(Base UI menu 的 modal 默认行为),关闭时恢复。
  // 菜单自身(含子菜单)允许继续滚动。
  useScrollLock(() => open() && modal(), {
    allowedSelector: MENU_POPUP_SELECTOR,
  });

  const [finalFocus, setFinalFocus] = createSignal(true);
  const [trigger, setTrigger] = createSignal<HTMLElement>();
  const [anchor, setAnchor] = createSignal<VirtualElement>();
  const contentId = `context-menu-content-${createUniqueId()}`;

  // 已挂载的浮层容器(根菜单 + 各级子菜单),用于判断指针是否落在菜单内部。
  // 这里刻意不用响应式结构:它只在事件回调里读取,变化本身不需要驱动渲染。
  const menuElements = new Set<HTMLElement>();

  const restoreFocus = () => {
    const el = trigger();
    if (el && typeof el.focus === "function") {
      el.focus({ preventScroll: true });
    }
  };

  const commit = (next: boolean, reason: ContextMenuChangeEventReason, event?: Event) => {
    const details = createChangeEventDetails(reason, event, trigger());
    // 先回调再落状态：调用方可以在 details.cancel() 里否决本次变更
    // （与 ToggleGroup / TimePicker / Drawer 一致，对齐 Base UI 的语义）
    props.onOpenChange?.(next, details);
    if (details.isCanceled) return;
    if (props.open === undefined) setInternalOpen(next);
  };

  const openAt = (
    x: number,
    y: number,
    contextElement: Element | undefined,
    event?: Event,
    reason: ContextMenuChangeEventReason = "trigger-press",
  ) => {
    if (disabled()) return;
    // 每次打开都换一个新的虚拟元素:即使菜单已经打开,右键到新位置也会
    // 触发 positioner 重新计算(autoUpdate 之外的一条路径)。
    setAnchor(createVirtualElement({ x, y, contextElement }));
    if (!open()) commit(true, reason, event);
  };

  const closeAll = (reason: ContextMenuChangeEventReason, event?: Event) => {
    const wasOpen = open();
    if (!wasOpen) return;
    commit(false, reason, event);
    // cancel() 之后菜单并没有真的关闭，因此也不该动焦点
    if (open() === wasOpen) return;
    // 点击菜单外部时焦点应该留在用户刚点的目标上,不能把焦点抢回触发器。
    if (reason !== "outside-press" && finalFocus()) restoreFocus();
  };

  const isInsideMenu = (node: Node | null): boolean => {
    if (!node || !(node instanceof Element)) return false;
    for (const el of menuElements) {
      if (el.contains(node)) return true;
    }
    return false;
  };

  const isInsideTrigger = (node: Node | null): boolean => {
    if (!node || !(node instanceof Element)) return false;
    return !!trigger()?.contains(node);
  };

  const registerMenuElement = (el: HTMLElement) => {
    menuElements.add(el);
    return () => {
      menuElements.delete(el);
    };
  };

  // 打开期间绑定 document 级监听:点击外部/再次右键外部关闭 + Escape 兜底关闭。
  // 菜单内部的 Escape 会由浮层自己处理并 stopPropagation,不会走到这里。
  createEffect(() => {
    if (!open()) return;

    const onPointerDown = (e: PointerEvent) => {
      // 右键交给 contextmenu 处理:否则会先被这里关掉、又被触发器重新打开,
      // 中间闪一帧。
      if (e.button === 2) return;
      // 注意这里不把触发器算作"内部":菜单打开时左键点触发器应当关闭菜单。
      if (!isInsideMenu(e.target as Node | null)) {
        closeAll("outside-press", e);
      }
    };

    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as Node | null;
      // 在触发器上再次右键只更新锚点,由触发器自己的 contextmenu 处理。
      if (!isInsideMenu(target) && !isInsideTrigger(target)) {
        closeAll("outside-press", e);
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      // 菜单浮层自己处理 Escape 时会 preventDefault + stopPropagation。
      // 这里再检查一次 defaultPrevented:键盘事件在 Solid 里是委托到 document 的,
      // 同元素上的其它监听器不会被 stopPropagation 拦下,靠这个标记避免重复关闭。
      if (e.defaultPrevented) return;
      if (e.key === "Escape") closeAll("escape-key", e);
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("contextmenu", onContextMenu, true);
    document.addEventListener("keydown", onKeyDown);
    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("contextmenu", onContextMenu, true);
      document.removeEventListener("keydown", onKeyDown);
    });
  });

  const ctx: ContextMenuContextValue = {
    open,
    disabled,
    loopFocus,
    orientation,
    highlightItemOnHover,
    trigger,
    setTrigger,
    anchor,
    contentId,
    finalFocus,
    setFinalFocus,
    openAt,
    closeAll,
    registerMenuElement,
    isInsideMenu,
  };

  return <ContextMenuContext.Provider value={ctx}>{props.children}</ContextMenuContext.Provider>;
}
