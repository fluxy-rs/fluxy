import type { Accessor } from "solid-js";
import type { ReferenceElement, VirtualElement } from "../lib";

/**
 * Base UI 的 `Side`,额外支持逻辑方向 `inline-start` / `inline-end`。
 * 逻辑方向会按 `dir` 解析成 left / right(RTL 下相反)。
 */
export type ContextMenuSide = "top" | "right" | "bottom" | "left" | "inline-start" | "inline-end";

/** Base UI 的 `Align` */
export type ContextMenuAlign = "start" | "center" | "end";

/** Base UI 菜单的排列方向,决定方向键映射(默认 vertical) */
export type ContextMenuOrientation = "vertical" | "horizontal";

/**
 * 对齐 Base UI 的 `ContextMenu.Root.ChangeEventReason`。
 * 本实现会尽力给出语义正确的原因,部分原因(如 `trigger-hover`)只在子菜单里出现。
 */
export type ContextMenuChangeEventReason =
  | "trigger-hover"
  | "trigger-focus"
  | "trigger-press"
  | "outside-press"
  | "focus-out"
  | "list-navigation"
  | "escape-key"
  | "item-press"
  | "close-press"
  | "sibling-open"
  | "cancel-open"
  | "imperative-action"
  | "none";

/**
 * 对齐 Base UI 的 `ChangeEventDetails`。`cancel()` / `allowPropagation()` 用于在
 * 回调里干预组件的默认行为;Solid 下事件已派发,`allowPropagation` 只作为 API
 * 对齐保留(本实现不主动阻止事件传播)。
 */
export interface ContextMenuChangeEventDetails<R extends string = ContextMenuChangeEventReason> {
  reason: R;
  event: Event | undefined;
  trigger: Element | undefined;
  /** 阻止组件处理本次变更(例如受控模式下不关闭) */
  cancel: () => void;
  /** API 对齐保留:允许事件继续传播 */
  allowPropagation: () => void;
  readonly isCanceled: boolean;
  readonly isPropagationAllowed: boolean;
}

/** 一个已挂载菜单项在运行时的元信息,供键盘导航 / 无障碍 / 子菜单联动使用 */
export interface ContextMenuItemEntry {
  /** 稳定的 DOM id,同时用作 `aria-activedescendant` 的取值 */
  id: string;
  element: HTMLElement;
  disabled: () => boolean;
  /** 键盘字符导航时用于匹配的文本 */
  label: () => string;
  /** 是否是"打开子菜单"的项 */
  hasPopup: () => boolean;
  /** 键盘 Enter/Space 或点击时触发 */
  activate: () => void;
  /** 仅子菜单触发器:打开对应的子菜单 */
  openPopup?: (reason: ContextMenuChangeEventReason, event?: Event) => void;
}

/** 根组件通过 context 暴露的状态与动作 */
export interface ContextMenuContextValue {
  open: Accessor<boolean>;
  disabled: Accessor<boolean>;
  loopFocus: Accessor<boolean>;
  orientation: Accessor<ContextMenuOrientation>;
  highlightItemOnHover: Accessor<boolean>;
  /** 触发器元素,关闭时把焦点还给它 */
  trigger: Accessor<HTMLElement | undefined>;
  setTrigger: (el: HTMLElement | undefined) => void;
  /** 右键菜单的虚拟锚点(鼠标坐标) */
  anchor: Accessor<VirtualElement | undefined>;
  contentId: string;
  /** 关闭后是否把焦点还给触发器(Base UI Popup.finalFocus 的布尔子集) */
  finalFocus: Accessor<boolean>;
  setFinalFocus: (value: boolean) => void;
  /** 在指定坐标打开(已打开时仅更新锚点) */
  openAt: (
    x: number,
    y: number,
    contextElement: Element | undefined,
    event?: Event,
    reason?: ContextMenuChangeEventReason,
  ) => void;
  /** 关闭整个菜单树 */
  closeAll: (reason: ContextMenuChangeEventReason, event?: Event) => void;
  /** 注册浮层容器元素(用于判断"点击是否发生在菜单内部") */
  registerMenuElement: (el: HTMLElement) => () => void;
  isInsideMenu: (node: Node | null) => boolean;
}

/** 每个浮层(根菜单 / 子菜单)各自维护的运行时上下文 */
export interface ContextMenuPopupContextValue {
  root: ContextMenuContextValue;
  /** 父级浮层;根菜单为 undefined */
  parent: ContextMenuPopupContextValue | undefined;
  /** 该浮层所属的子菜单上下文;根菜单为 undefined */
  submenu?: ContextMenuSubmenuContextValue;
  menuId: string;
  isSubmenu: boolean;
  /** 真正承载焦点的 popup 元素 */
  popup: Accessor<HTMLElement | undefined>;
  setPopup: (el: HTMLElement | undefined) => void;
  /** 以 DOM 顺序排列的已注册菜单项 */
  items: Accessor<ContextMenuItemEntry[]>;
  registerItem: (entry: ContextMenuItemEntry) => () => void;
  activeId: Accessor<string | undefined>;
  setActiveId: (id: string | undefined) => void;
  activeEntry: () => ContextMenuItemEntry | undefined;
  moveActive: (delta: 1 | -1) => void;
  focusFirst: () => void;
  focusLast: () => void;
  /** 高亮指定项,并关闭其它项已打开的子菜单 */
  highlight: (id: string | undefined) => void;
  /** 当前由本浮层打开的二级菜单对应的项 id */
  openPopupState: Accessor<ContextMenuOpenPopupState | undefined>;
  openPopup: (id: string, reason: ContextMenuChangeEventReason, event?: Event) => void;
  closeOpenPopup: () => void;
  closeAll: (reason: ContextMenuChangeEventReason, event?: Event) => void;
  highlightItemOnHover: Accessor<boolean>;
}

export interface ContextMenuOpenPopupState {
  id: string;
  reason: ContextMenuChangeEventReason;
  event: Event | undefined;
}

/** 子菜单根组件提供的上下文 */
export interface ContextMenuSubmenuContextValue {
  root: ContextMenuContextValue;
  /** 子菜单触发器所属的父浮层 */
  parentPopup: ContextMenuPopupContextValue;
  open: Accessor<boolean>;
  setOpen: (open: boolean, reason: ContextMenuChangeEventReason, event?: Event) => void;
  disabled: Accessor<boolean>;
  closeParentOnEsc: Accessor<boolean>;
  loopFocus: Accessor<boolean>;
  orientation: Accessor<ContextMenuOrientation>;
  highlightItemOnHover: Accessor<boolean>;
  /** 子菜单触发器元素(同时是子菜单的定位锚点) */
  trigger: Accessor<HTMLElement | undefined>;
  setTrigger: (el: HTMLElement | undefined) => void;
  /** 子菜单触发器在父浮层里注册的项 id */
  itemId: Accessor<string | undefined>;
  setItemId: (id: string | undefined) => void;
  openSubmenu: (reason: ContextMenuChangeEventReason, event?: Event) => void;
  closeSubmenu: (
    reason: ContextMenuChangeEventReason,
    event?: Event,
    focusTrigger?: boolean,
  ) => void;
  /** 鼠标移出后延迟关闭(带防抖,避免穿越间隙时闪烁) */
  scheduleClose: (delay?: number) => void;
  cancelClose: () => void;
}

/** RadioGroup 共享给 RadioItem 的选中状态 */
export interface ContextMenuRadioGroupContextValue {
  value: Accessor<any>;
  setValue: (value: any, event?: Event) => void;
  disabled: Accessor<boolean>;
}

/** Group 与 GroupLabel 通过 id 建立 aria 关联 */
export interface ContextMenuGroupContextValue {
  labelId: Accessor<string | undefined>;
  setLabelId: (id: string | undefined) => void;
}

/** Popup 定位所需的最小配置 */
export interface ContextMenuPositioningOptions {
  reference: Accessor<ReferenceElement | undefined>;
  side: Accessor<ContextMenuSide>;
  align: Accessor<ContextMenuAlign>;
  sideOffset: Accessor<number>;
  alignOffset: Accessor<number>;
  collisionPadding: Accessor<number>;
  dir: Accessor<"ltr" | "rtl" | "auto" | undefined>;
}
