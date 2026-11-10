import { createSignal, createUniqueId, onCleanup, type Accessor } from "solid-js";
import type { Placement, Positioner, ReferenceElement } from "../../lib";
import type {
  ContextMenuAlign,
  ContextMenuContextValue,
  ContextMenuOpenPopupState,
  ContextMenuOrientation,
  ContextMenuPopupContextValue,
  ContextMenuSide,
  ContextMenuSubmenuContextValue,
} from "../context-menu.types";
import { handleMenuKeyDown } from "../context-menu.keyboard";
import { createHighlight } from "./context-menu.highlight";
import { createPopupPositioner } from "./context-menu.positioner";
import { createItemCollection } from "./context-menu.popup-items";
import { createTypeahead } from "../context-menu.typeahead";

export interface CreateContextMenuPopupOptions {
  root: ContextMenuContextValue;
  /** 该浮层自身的打开状态(根菜单 = root.open,子菜单 = submenu.open) */
  open: Accessor<boolean>;
  /** 父级浮层:根菜单为 undefined,子菜单为触发它的那个浮层 */
  parent?: ContextMenuPopupContextValue;
  isSubmenu: boolean;
  /** 定位锚点:根菜单是鼠标坐标的虚拟元素,子菜单是子菜单触发器 */
  reference: Accessor<ReferenceElement | undefined>;
  side: Accessor<ContextMenuSide>;
  align: Accessor<ContextMenuAlign>;
  sideOffset: Accessor<number>;
  alignOffset: Accessor<number>;
  collisionPadding: Accessor<number>;
  dir: Accessor<"ltr" | "rtl" | "auto" | undefined>;
  /** 子菜单可以覆盖根组件的行为;不传则回退到 root 上的值 */
  loopFocus?: Accessor<boolean>;
  orientation?: Accessor<ContextMenuOrientation>;
  highlightItemOnHover?: Accessor<boolean>;
  /** 子菜单场景下提供,用于 ArrowLeft / Escape 只关闭当前子菜单 */
  submenu?: ContextMenuSubmenuContextValue;
}

export interface ContextMenuPopupRuntime {
  popupCtx: ContextMenuPopupContextValue;
  pos: Positioner;
  /** 真正承载焦点的 popup 元素 */
  popupEl: Accessor<HTMLElement | undefined>;
  setPopupEl: (el: HTMLElement | undefined) => void;
  /** 负责定位的外层元素 */
  positionerEl: Accessor<HTMLElement | undefined>;
  setPositionerEl: (el: HTMLElement | undefined) => void;
  /** size 中间件算出的可用高度,用于 max-height 滚动 */
  availableHeight: Accessor<number | undefined>;
  placement: Accessor<Placement>;
  onKeyDown: (e: KeyboardEvent) => void;
  /** 打开后聚焦 popup 并把高亮落到第一项 */
  setupOnOpen: () => void;
}

/**
 * 一个浮层(根菜单或子菜单)的运行时:定位、菜单项注册、高亮/键盘导航、
 * 类型头(typeahead)以及开关联动都收敛在这里。`ContextMenuContent` 与
 * `ContextMenuSubContent` 共用它,只是传入的锚点与父子关系不同。
 *
 * 交互模型:焦点始终落在 popup 上,高亮项通过 `aria-activedescendant` 表达
 * (而不是把真实焦点在菜单项之间搬来搬去)。这与本项目 Select 的做法一致,
 * 既避免了焦点在 Portal / 子菜单之间的来回跳,也让"关闭后把焦点还给触发器"
 * 变得非常简单。
 */
export function createContextMenuPopupRuntime(
  options: CreateContextMenuPopupOptions,
): ContextMenuPopupRuntime {
  const { root } = options;
  const loopFocus = () => options.loopFocus?.() ?? root.loopFocus();
  const orientation = () => options.orientation?.() ?? root.orientation();
  const highlightItemOnHover = () =>
    options.highlightItemOnHover?.() ?? root.highlightItemOnHover();

  const [popupEl, setPopupEl] = createSignal<HTMLElement>();
  const [positionerEl, setPositionerEl] = createSignal<HTMLElement>();
  const { registerItem, orderedItems, enabledItems } = createItemCollection();
  const [openPopupState, setOpenPopupState] = createSignal<ContextMenuOpenPopupState>();
  const menuId = `context-menu-popup-${createUniqueId()}`;

  const { pos, availableHeight, placement } = createPopupPositioner({
    reference: options.reference,
    positionerElement: positionerEl,
    side: options.side,
    align: options.align,
    dir: options.dir,
    sideOffset: options.sideOffset,
    alignOffset: options.alignOffset,
    collisionPadding: options.collisionPadding,
  });

  const closeOpenPopup = () => setOpenPopupState(undefined);

  const highlight = createHighlight({
    orderedItems,
    enabledItems,
    loopFocus,
    openPopupId: () => openPopupState()?.id,
    closeOpenPopup,
    open: options.open,
  });
  const { activeId, activeEntry, setActiveId, moveActive, focusFirst, focusLast } = highlight;

  // --- 键盘字符导航 ---
  const typeahead = createTypeahead({
    candidates: () => enabledItems().map((item) => ({ id: item.id, label: item.label() })),
    onMatch: (id) => setActiveId(id),
  });

  onCleanup(() => typeahead.dispose());

  const onKeyDown = (event: KeyboardEvent) =>
    handleMenuKeyDown(event, {
      horizontal: orientation() === "horizontal",
      activeEntry,
      moveActive,
      focusFirst,
      focusLast,
      typeahead: (char) => typeahead.handle(char),
      submenu: options.submenu,
      closeAll: (reason, e) => root.closeAll(reason, e),
    });

  const popupCtx: ContextMenuPopupContextValue = {
    root,
    parent: options.parent,
    submenu: options.submenu,
    menuId,
    isSubmenu: options.isSubmenu,
    popup: popupEl,
    setPopup: (el) => setPopupEl(el),
    items: orderedItems,
    registerItem,
    activeId,
    setActiveId,
    activeEntry,
    moveActive,
    focusFirst,
    focusLast,
    highlight: (id) => setActiveId(id),
    openPopupState,
    openPopup: (id, reason, event) => setOpenPopupState({ id, reason, event }),
    closeOpenPopup,
    closeAll: (reason, event) => root.closeAll(reason, event),
    highlightItemOnHover,
  };

  /** 浮层挂载完成后的收尾:聚焦 popup(键盘事件依赖它)并高亮第一项 */
  const setupOnOpen = () => {
    queueMicrotask(() => {
      if (!options.open()) return;
      popupEl()?.focus({ preventScroll: true });
      if (activeId() === undefined) focusFirst();
    });
  };

  return {
    popupCtx,
    pos,
    popupEl,
    setPopupEl: (el) => setPopupEl(el),
    positionerEl,
    setPositionerEl: (el) => setPositionerEl(el),
    availableHeight,
    placement,
    onKeyDown,
    setupOnOpen,
  };
}
