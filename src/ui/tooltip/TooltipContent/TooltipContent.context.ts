import { createContext, useContext, type Accessor } from "solid-js";
import type { MiddlewareData, Placement } from "../../lib";

export interface TooltipContentContextValue {
  middlewareData: Accessor<MiddlewareData>;
  reference: Accessor<Element | undefined>;
  placement: Accessor<Placement>;
  setArrowElement: (el: Element) => void;

  animationState: Accessor<"open" | "closed">;
}

export const TooltipContentContext = createContext<TooltipContentContextValue>();

export function useTooltipContentContext(component: string): TooltipContentContextValue {
  const ctx = useContext(TooltipContentContext);
  if (!ctx) {
    throw new Error(`<${component}> must be rendered inside <TooltipContent>`);
  }
  return ctx;
}
