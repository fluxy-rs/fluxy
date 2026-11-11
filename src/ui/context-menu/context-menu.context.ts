import { createContext, useContext } from "solid-js";
import type {
  ContextMenuGroupContextValue,
  ContextMenuPopupContextValue,
  ContextMenuRadioGroupContextValue,
  ContextMenuSubmenuContextValue,
} from "./context-menu.types";

/**
 * 内部运行时 context。与 `ContextMenu/ContextMenu.context.ts`(根状态)分开:
 * 根 context 由公开的 `<ContextMenu>` 提供,这里几个由各个浮层/分组组件提供,
 * 属于实现细节,不对外导出。
 */

export const ContextMenuPopupContext = createContext<ContextMenuPopupContextValue>();

export function useContextMenuPopup(component: string): ContextMenuPopupContextValue {
  const ctx = useContext(ContextMenuPopupContext);
  if (!ctx) {
    throw new Error(`<${component}> 必须渲染在 <ContextMenuContent> 内部`);
  }
  return ctx;
}

export const ContextMenuSubmenuContext = createContext<ContextMenuSubmenuContextValue>();

export function useContextMenuSubmenu(component: string): ContextMenuSubmenuContextValue {
  const ctx = useContext(ContextMenuSubmenuContext);
  if (!ctx) {
    throw new Error(`<${component}> 必须渲染在 <ContextMenuSub> 内部`);
  }
  return ctx;
}

export const ContextMenuRadioGroupContext = createContext<ContextMenuRadioGroupContextValue>();

export function useContextMenuRadioGroup(component: string): ContextMenuRadioGroupContextValue {
  const ctx = useContext(ContextMenuRadioGroupContext);
  if (!ctx) {
    throw new Error(`<${component}> 必须渲染在 <ContextMenuRadioGroup> 内部`);
  }
  return ctx;
}

export const ContextMenuGroupContext = createContext<ContextMenuGroupContextValue>();

export function useContextMenuGroup(component: string): ContextMenuGroupContextValue {
  const ctx = useContext(ContextMenuGroupContext);
  if (!ctx) {
    throw new Error(`<${component}> 必须渲染在 <ContextMenuGroup> 内部`);
  }
  return ctx;
}
