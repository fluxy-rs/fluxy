import { createContext, useContext } from "solid-js";
import type { ContextMenuContextValue } from "../context-menu.types";

export const ContextMenuContext = createContext<ContextMenuContextValue>();

export function useContextMenuContext(component: string): ContextMenuContextValue {
  const ctx = useContext(ContextMenuContext);
  if (!ctx) {
    throw new Error(`<${component}> 必须渲染在 <ContextMenu> 内部`);
  }
  return ctx;
}
