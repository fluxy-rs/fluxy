import type { Accessor, ParentProps } from "solid-js";

export interface PopoverProps extends ParentProps {
  /** 受控打开状态；不传则内部自管理（非受控模式） */
  open?: boolean;
  /** 非受控模式下的初始打开状态 */
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  disabled?: boolean;
  /** 是否启用 modal 模式（当前仅透传，不实现焦点陷阱） */
  modal?: boolean;
  /**
   * 打开时是否锁定页面滚动，默认 true：锁住文档滚动并拦截浮层之外的
   * 滚轮/触摸滚动，避免浮层锚点随页面滚动而"飘移"；浮层内容自身仍可滚动。
   */
  lockScroll?: boolean;
}

export interface PopoverContextValue {
  open: Accessor<boolean>;
  disabled: Accessor<boolean>;
  modal: Accessor<boolean>;
  contentId: string;
  reference: Accessor<Element | undefined>;
  setReference: (el: Element | undefined) => void;
  floating: Accessor<HTMLElement | undefined>;
  setFloating: (el: HTMLElement | undefined) => void;
  openPopover: () => void;
  closePopover: () => void;
  togglePopover: () => void;
}
