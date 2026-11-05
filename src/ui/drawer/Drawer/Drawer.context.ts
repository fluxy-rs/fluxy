import { createContext, useContext } from "solid-js";
import type { DrawerContextValue } from "./Drawer.types";

export const DrawerContext = createContext<DrawerContextValue>();

export function useDrawerContext(component: string): DrawerContextValue {
  const ctx = useContext(DrawerContext);
  if (!ctx) {
    throw new Error(`<${component}> must be rendered inside <Drawer>`);
  }
  return ctx;
}
