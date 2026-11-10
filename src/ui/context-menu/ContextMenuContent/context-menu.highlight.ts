import { createEffect, createMemo, createSignal, type Accessor } from "solid-js";
import type { ContextMenuItemEntry } from "../context-menu.types";

/**
 * 浮层内的高亮导航。
 *
 * 单一职责：维护"当前高亮的是哪一项"，并提供移动/跳到首尾的能力。
 * 交互模型是焦点留在 popup 上、高亮用 `aria-activedescendant` 表达，
 * 因此这里只改 id，不碰真实焦点。
 */

/** 键盘移动时的目标下标（-1 表示列表为空，调用方据此放弃） */
export function resolveNextActiveIndex(
  currentIndex: number,
  delta: 1 | -1,
  length: number,
  loopFocus: boolean,
): number {
  if (length <= 0) return -1;
  if (currentIndex === -1) return delta > 0 ? 0 : length - 1;

  const next = currentIndex + delta;
  if (next < 0) return loopFocus ? length - 1 : 0;
  if (next >= length) return loopFocus ? 0 : length - 1;
  return next;
}

export interface CreateHighlightOptions {
  /** 全部菜单项（按 DOM 顺序），用于反查当前高亮项 */
  orderedItems: Accessor<ContextMenuItemEntry[]>;
  /** 可参与导航的菜单项（排除 disabled） */
  enabledItems: Accessor<ContextMenuItemEntry[]>;
  loopFocus: () => boolean;
  /** 当前展开的子菜单项 id（没有展开时为 undefined） */
  openPopupId: Accessor<string | undefined>;
  /** 关闭已展开的子菜单 */
  closeOpenPopup: () => void;
  /** 本浮层是否打开；关闭时清空高亮，保证下次从第一项开始 */
  open: Accessor<boolean>;
}

export interface HighlightNavigation {
  activeId: Accessor<string | undefined>;
  activeEntry: Accessor<ContextMenuItemEntry | undefined>;
  setActiveId: (id: string | undefined) => void;
  moveActive: (delta: 1 | -1) => void;
  focusFirst: () => void;
  focusLast: () => void;
}

export function createHighlight(options: CreateHighlightOptions): HighlightNavigation {
  const [activeId, setActiveIdInternal] = createSignal<string | undefined>();

  /** 高亮某项；若高亮切到了别的项，顺手关掉已展开的子菜单 */
  const setActiveId = (id: string | undefined) => {
    setActiveIdInternal(id);
    const openId = options.openPopupId();
    if (openId !== undefined && openId !== id) options.closeOpenPopup();
  };

  const activeEntry = createMemo(() =>
    options.orderedItems().find((item) => item.id === activeId()),
  );

  // 高亮项变化时滚动到可见区域
  createEffect(() => {
    const entry = activeEntry();
    if (!entry) return;
    entry.element.scrollIntoView({ block: "nearest" });
  });

  // 关闭时清空高亮与子菜单状态
  createEffect(() => {
    if (options.open()) return;
    setActiveIdInternal(undefined);
    options.closeOpenPopup();
  });

  return {
    activeId,
    activeEntry,
    setActiveId,
    moveActive(delta) {
      const list = options.enabledItems();
      const currentIndex = list.findIndex((item) => item.id === activeId());
      const nextIndex = resolveNextActiveIndex(
        currentIndex,
        delta,
        list.length,
        options.loopFocus(),
      );
      if (nextIndex === -1) return;
      setActiveId(list[nextIndex]!.id);
    },
    focusFirst() {
      const list = options.enabledItems();
      setActiveId(list.length > 0 ? list[0]!.id : undefined);
    },
    focusLast() {
      const list = options.enabledItems();
      setActiveId(list.length > 0 ? list[list.length - 1]!.id : undefined);
    },
  };
}
