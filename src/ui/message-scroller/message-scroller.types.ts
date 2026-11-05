import type { Accessor, ParentProps } from "solid-js";
import type { BaseProps, PolymorphicProps } from "../types";

export type MessageScrollerDefaultScrollPosition = "start" | "end" | "last-anchor";
export type MessageScrollerButtonDirection = "start" | "end";
export type MessageScrollerScrollAlign = "start" | "center" | "end" | "nearest";

export interface MessageScrollerScrollOptions {
  align?: MessageScrollerScrollAlign;
  behavior?: ScrollBehavior;
  scrollMargin?: number;
}

export interface MessageScrollerScrollable {
  start: boolean;
  end: boolean;
}

export interface MessageScrollerVisibilityState {
  currentAnchorId: string | null;
  visibleMessageIds: string[];
}

export interface MessageScrollerProviderProps extends ParentProps {
  autoScroll?: boolean;
  defaultScrollPosition?: MessageScrollerDefaultScrollPosition;
  scrollEdgeThreshold?: number;
  scrollPreviousItemPeek?: number;
  scrollMargin?: number;
}

export interface MessageScrollerItemEntry {
  id?: string;
  element: HTMLElement;
}

export interface MessageScrollerContextValue {
  viewport: Accessor<HTMLElement | undefined>;
  setViewport: (el: HTMLElement | undefined) => void;
  content: Accessor<HTMLElement | undefined>;
  setContent: (el: HTMLElement | undefined) => void;
  setSpacer: (el: HTMLElement | undefined) => void;
  registerItem: (entry: MessageScrollerItemEntry) => () => void;
  preserveScrollOnPrepend: Accessor<boolean>;
  setPreserveScrollOnPrepend: (value: boolean) => void;
  scrollableStart: Accessor<boolean>;
  scrollableEnd: Accessor<boolean>;
  autoscrolling: Accessor<boolean>;
  pendingScroll: Accessor<boolean>;
  scrollToStart: (options?: MessageScrollerScrollOptions) => boolean;
  scrollToEnd: (options?: MessageScrollerScrollOptions) => boolean;
  scrollToMessage: (messageId: string, options?: MessageScrollerScrollOptions) => boolean;
  currentAnchorId: Accessor<string | null>;
  visibleMessageIds: Accessor<string[]>;
  subscribeVisibility: () => () => void;
  options: Accessor<Required<Omit<MessageScrollerProviderProps, "children">>>;
}

export interface MessageScrollerVisibilityValue {
  currentAnchorId: Accessor<string | null>;
  visibleMessageIds: Accessor<string[]>;
}

export type MessageScrollerProps = PolymorphicProps<"div", BaseProps, false>;
export type MessageScrollerViewportProps = PolymorphicProps<
  "div",
  BaseProps & {
    "aria-label"?: string;
    preserveScrollOnPrepend?: boolean;
  },
  false
>;
export type MessageScrollerContentProps = PolymorphicProps<
  "div",
  BaseProps & {
    "aria-busy"?: boolean;
    spacerClassName?: string;
  },
  false
>;
export type MessageScrollerItemProps = PolymorphicProps<
  "div",
  BaseProps & { messageId?: string; scrollAnchor?: boolean },
  false
>;
export type MessageScrollerButtonProps = PolymorphicProps<
  "button",
  BaseProps & {
    direction?: MessageScrollerButtonDirection;
    behavior?: ScrollBehavior;
    variant?: string;
    size?: string;
  }
>;
