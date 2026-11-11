import { createContext, useContext } from "solid-js";
import type { PopoverContextValue } from "./Popover.types";

export const PopoverContext = createContext<PopoverContextValue>();

export function usePopoverContext(component: string): PopoverContextValue {
  const ctx = useContext(PopoverContext);
  if (!ctx) {
    throw new Error(`<${component}> 必须渲染在 <Popover> 内部`);
  }
  return ctx;
}
