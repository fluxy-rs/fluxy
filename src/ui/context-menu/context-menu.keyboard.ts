import type {
  ContextMenuChangeEventReason,
  ContextMenuItemEntry,
  ContextMenuSubmenuContextValue,
} from "./context-menu.types";

/** 键盘处理需要的能力（全部以函数注入，便于脱离 Solid 单独测试） */
export interface MenuKeyboardContext {
  /** 横向菜单：左右方向键用于移动高亮，而不是进入/退出子菜单 */
  horizontal: boolean;
  /** 当前高亮项 */
  activeEntry: () => ContextMenuItemEntry | undefined;
  moveActive: (delta: 1 | -1) => void;
  focusFirst: () => void;
  focusLast: () => void;
  /** 字符导航（前缀匹配） */
  typeahead: (char: string) => void;
  /** 子菜单场景：ArrowLeft / Escape 只关当前子菜单 */
  submenu?: ContextMenuSubmenuContextValue;
  closeAll: (reason: ContextMenuChangeEventReason, event: Event) => void;
}

/**
 * 菜单的按键映射：一个纯派发器——只把按键翻译成"菜单动作"，
 * 不持有状态、不读 DOM（高亮项与子菜单关系都由 `ctx` 提供）。
 *
 * 分工：
 * - 上下方向键 / Home / End → 移动或跳转高亮；
 * - 左右方向键 → 横向菜单移动高亮，纵向菜单进入子菜单 / 退回父菜单；
 * - Enter / Space → 激活高亮项（禁用项忽略）；
 * - Escape → 关闭当前子菜单或整个菜单（子菜单可用 `closeParentOnEsc` 覆盖）；
 * - Tab → 关闭菜单并把焦点交还触发器（浏览器随后继续 Tab）；
 * - 其它可打印字符 → 交给 typeahead（带修饰键的组合键不参与）。
 */
export function handleMenuKeyDown(event: KeyboardEvent, ctx: MenuKeyboardContext): void {
  const { submenu, horizontal } = ctx;

  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      ctx.moveActive(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      ctx.moveActive(-1);
      break;
    case "ArrowRight": {
      if (horizontal) {
        event.preventDefault();
        ctx.moveActive(1);
        break;
      }
      const entry = ctx.activeEntry();
      if (entry?.hasPopup()) {
        event.preventDefault();
        entry.openPopup?.("list-navigation", event);
      }
      break;
    }
    case "ArrowLeft": {
      if (horizontal) {
        event.preventDefault();
        ctx.moveActive(-1);
        break;
      }
      if (submenu) {
        event.preventDefault();
        // 阻止冒泡，避免父级浮层也处理这次按键
        event.stopPropagation();
        submenu.closeSubmenu("list-navigation", event, true);
      }
      break;
    }
    case "Home":
      event.preventDefault();
      ctx.focusFirst();
      break;
    case "End":
      event.preventDefault();
      ctx.focusLast();
      break;
    case "Enter":
    case " ": {
      event.preventDefault();
      const entry = ctx.activeEntry();
      if (entry && !entry.disabled()) entry.activate();
      break;
    }
    case "Escape": {
      event.preventDefault();
      event.stopPropagation();
      if (submenu && !submenu.closeParentOnEsc()) {
        submenu.closeSubmenu("escape-key", event, true);
      } else {
        ctx.closeAll("escape-key", event);
      }
      break;
    }
    case "Tab": {
      // 关闭菜单并把焦点交还触发器，浏览器随后的 Tab 会从触发器之后继续
      event.preventDefault();
      ctx.closeAll("focus-out", event);
      break;
    }
    default: {
      if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
        ctx.typeahead(event.key);
      }
    }
  }
}
