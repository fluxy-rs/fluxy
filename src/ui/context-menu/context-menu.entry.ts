import { createUniqueId, onCleanup, onMount } from "solid-js";
import { useContextMenuPopup } from "./context-menu.context";
import type {
  ContextMenuChangeEventReason,
  ContextMenuItemEntry,
  ContextMenuPopupContextValue,
} from "./context-menu.types";

export interface UseContextMenuEntryOptions {
  /** 用于报错信息里指明是哪个组件 */
  component: string;
  /** 菜单项的真实 DOM 元素 */
  element: () => HTMLElement | undefined;
  disabled: () => boolean;
  /** 键盘字符导航用的文本;缺省时回退到元素的 textContent */
  label: () => string | undefined;
  hasPopup?: () => boolean;
  /** 键盘 Enter/Space 激活:默认触发一次 click,复用同一条 onClick 逻辑 */
  activate?: () => void;
  openPopup?: (reason: ContextMenuChangeEventReason, event?: Event) => void;
}

export interface UseContextMenuEntryResult {
  popup: ContextMenuPopupContextValue;
  /** 稳定的 DOM id,直接用于 id / aria-activedescendant */
  id: string;
  isActive: () => boolean;
  /** 鼠标进入时高亮(受 highlightItemOnHover 控制) */
  highlight: () => void;
}

/**
 * 菜单项(Item / CheckboxItem / RadioItem / SubmenuTrigger)共用的注册与高亮逻辑。
 * 每个项在挂载时把自己登记到所属浮层,卸载时注销,键盘导航与 typeahead 都基于
 * 这份注册表。
 */
export function useContextMenuEntry(
  options: UseContextMenuEntryOptions,
): UseContextMenuEntryResult {
  const popup = useContextMenuPopup(options.component);
  const id = `context-menu-item-${createUniqueId()}`;

  onMount(() => {
    const element = options.element();
    if (!element) return;

    const entry: ContextMenuItemEntry = {
      id,
      element,
      disabled: options.disabled,
      label: () => options.label() ?? element.textContent?.trim() ?? "",
      hasPopup: options.hasPopup ?? (() => false),
      activate: options.activate ?? (() => element.click()),
      openPopup: options.openPopup,
    };

    const unregister = popup.registerItem(entry);
    onCleanup(unregister);
  });

  return {
    popup,
    id,
    isActive: () => popup.activeId() === id,
    highlight: () => {
      if (!popup.highlightItemOnHover() || options.disabled()) return;
      popup.highlight(id);
    },
  };
}
