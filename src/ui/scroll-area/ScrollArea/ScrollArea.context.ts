import { type Accessor, createContext, type Setter, useContext } from "solid-js";
import type { ScrollMetrics } from "./ScrollArea.types";

interface ScrollAreaContextValue {
  hovering: Accessor<boolean>;
  viewportRef: () => HTMLDivElement | undefined;
  dragging: Accessor<"vertical" | "horizontal" | null>;
  setDragging: Setter<"vertical" | "horizontal" | null>;
  vertical: Accessor<ScrollMetrics>;
  horizontal: Accessor<ScrollMetrics>;
}

export const ScrollAreaContext = createContext<ScrollAreaContextValue>();

export const useScrollAreaContext = () => {
  const context = useContext(ScrollAreaContext);
  if (!context) {
    throw new Error("useScrollAreaContext must be used inside <ScrollArea>");
  }
  return context;
};
