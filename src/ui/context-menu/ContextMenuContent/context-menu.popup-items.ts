import { createMemo, createSignal, type Accessor } from "solid-js";
import type { ContextMenuItemEntry } from "../context-menu.types";

/**
 * 浮层内的菜单项集合。
 *
 * 单一职责：维护"这一层里注册了哪些菜单项"，并给出两种派生视图——
 * 按 DOM 顺序排好的全部项、以及其中未禁用的项。浮层之间的父子关系、
 * 高亮与定位都不在这里。
 */

/** 按 DOM 顺序比较两个菜单项：A 在 B 之前返回 -1，之后返回 1，同一元素返回 0 */
export function compareItemElements(a: ContextMenuItemEntry, b: ContextMenuItemEntry): number {
  if (a.element === b.element) return 0;
  const relation = a.element.compareDocumentPosition(b.element);
  return relation & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1;
}

export interface ItemCollection {
  /** 按注册时机排列的原始列表（`orderedItems` 之前的状态） */
  items: Accessor<ContextMenuItemEntry[]>;
  registerItem: (entry: ContextMenuItemEntry) => () => void;
  /**
   * 按 DOM 顺序排序后的列表。
   * 调用方常用 `<For>` 动态生成菜单项，注册顺序与实际渲染顺序可能不同，
   * 而"上一项/下一项"必须按用户看到的顺序走。
   */
  orderedItems: Accessor<ContextMenuItemEntry[]>;
  /** 可参与键盘导航的项（排除 disabled） */
  enabledItems: Accessor<ContextMenuItemEntry[]>;
}

export function createItemCollection(): ItemCollection {
  const [items, setItems] = createSignal<ContextMenuItemEntry[]>([]);

  const orderedItems = createMemo(() => [...items()].sort(compareItemElements));

  return {
    items,
    orderedItems,
    enabledItems: createMemo(() => orderedItems().filter((item) => !item.disabled())),
    registerItem(entry) {
      setItems((list) => [...list, entry]);
      return () => setItems((list) => list.filter((it) => it.id !== entry.id));
    },
  };
}
