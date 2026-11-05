import { createContext, useContext } from "solid-js";
import type { MessageScrollerContextValue } from "./message-scroller.types";

export const MessageScrollerContext = createContext<MessageScrollerContextValue>();

export function useMessageScrollerContext(component: string): MessageScrollerContextValue {
  const ctx = useContext(MessageScrollerContext);
  if (!ctx) {
    throw new Error(`<${component}> must be rendered inside <MessageScrollerProvider>`);
  }
  return ctx;
}
