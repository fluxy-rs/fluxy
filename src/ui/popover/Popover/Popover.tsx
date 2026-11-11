import { createMemo, createSignal, createUniqueId, type JSX } from "solid-js";
import { useScrollLock } from "../../hooks";
import { PopoverContext } from "./Popover.context";
import type { PopoverContextValue, PopoverProps } from "./Popover.types";

/**
 * Popover 根组件：不渲染任何 DOM，只负责状态管理 + 提供 context。
 * 真正的展示交给 <PopoverTrigger>/<PopoverContent>。
 *
 * 这是 shadcn Base UI 版本 Popover 的 SolidJS 移植：Base UI 的 Root 同样不渲染
 * 自己的 HTML 元素，只作为状态容器使用。
 *
 * @example
 * ```tsx
 * <Popover>
 *   <PopoverTrigger>打开</PopoverTrigger>
 *   <PopoverContent>内容</PopoverContent>
 * </Popover>
 * ```
 */
export function Popover(props: PopoverProps): JSX.Element {
  const [internalOpen, setInternalOpen] = createSignal(props.defaultOpen ?? false);
  const open = createMemo(() => (props.open !== undefined ? props.open : internalOpen()));
  const disabled = createMemo(() => !!props.disabled);
  const modal = createMemo(() => !!props.modal);
  const lockScroll = createMemo(() => props.lockScroll ?? true);

  // 打开期间默认锁定页面滚动:文档滚动被锁、浮层之外的滚轮/触摸被拦截,
  // 但浮层自身仍可滚动。传 lockScroll={false} 可关闭该行为。
  useScrollLock(() => open() && lockScroll(), {
    allowedSelector: '[data-slot="popover-content"]',
  });

  const [reference, setReference] = createSignal<Element>();
  const [floating, setFloating] = createSignal<HTMLElement>();

  const commit = (next: boolean) => {
    if (props.open === undefined) setInternalOpen(next);
    props.onOpenChange?.(next);
  };

  const openPopover = () => {
    if (disabled()) return;
    commit(true);
  };

  const closePopover = () => {
    commit(false);
  };

  const togglePopover = () => {
    if (disabled()) return;
    commit(!open());
  };

  const contentId = `popover-${createUniqueId()}`;

  const ctx: PopoverContextValue = {
    open,
    disabled,
    modal,
    contentId,
    reference,
    setReference,
    floating,
    setFloating,
    openPopover,
    closePopover,
    togglePopover,
  };

  return <PopoverContext.Provider value={ctx}>{props.children}</PopoverContext.Provider>;
}
