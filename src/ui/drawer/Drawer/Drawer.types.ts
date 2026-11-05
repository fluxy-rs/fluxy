import type { Accessor, ParentProps } from "solid-js";
import type { BaseProps, PolymorphicProps } from "../../types";

export type DrawerSwipeDirection = "up" | "down" | "left" | "right";

export type DrawerChangeReason =
  | "trigger-press"
  | "close-press"
  | "outside-press"
  | "escape-key"
  | "swipe"
  | "none";

export interface DrawerChangeEventDetails {
  reason: DrawerChangeReason;
  event?: Event;
  trigger?: Element;
  cancel: () => void;
  allowPropagation: () => void;
  readonly isCanceled: boolean;
  readonly isPropagationAllowed: boolean;
}

export interface DrawerProps extends ParentProps {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean, eventDetails: DrawerChangeEventDetails) => void;
  modal?: boolean;
  swipeDirection?: DrawerSwipeDirection;
  showSwipeHandle?: boolean;
  disablePointerDismissal?: boolean;
}

export interface DrawerContextValue {
  open: Accessor<boolean>;
  setOpen: (open: boolean, reason: DrawerChangeReason, event?: Event) => void;
  show: Accessor<boolean>;
  setShow: (show: boolean) => void;
  modal: Accessor<boolean>;
  swipeDirection: Accessor<DrawerSwipeDirection>;
  showSwipeHandle: Accessor<boolean>;
  disablePointerDismissal: Accessor<boolean>;
  trigger: Accessor<HTMLElement | undefined>;
  setTrigger: (el: HTMLElement | undefined) => void;
  popup: Accessor<HTMLElement | undefined>;
  setPopup: (el: HTMLElement | undefined) => void;
  contentId: string;
  titleId: Accessor<string | undefined>;
  setTitleId: (id: string | undefined) => void;
  descriptionId: Accessor<string | undefined>;
  setDescriptionId: (id: string | undefined) => void;
  restoreFocus: () => void;
}

export type DrawerTriggerProps<T extends import("solid-js").ValidComponent = "button"> =
  PolymorphicProps<T, BaseProps>;
export type DrawerCloseProps<T extends import("solid-js").ValidComponent = "button"> =
  PolymorphicProps<T, BaseProps>;
