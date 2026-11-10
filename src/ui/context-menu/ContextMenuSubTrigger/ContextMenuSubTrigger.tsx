import { createEffect, onCleanup, onMount, splitProps } from "solid-js";
import { ChevronRight } from "lucide-solid";
import { clsx } from "../../utils";
import { useContextMenuSubmenu } from "../context-menu.context";
import { useContextMenuEntry } from "../context-menu.entry";
import { contextMenuSubTriggerClass } from "../context-menu.styles";
import type { ContextMenuSubTriggerProps } from "./ContextMenuSubTrigger.types";

/**
 * 打开子菜单的菜单项。内置一个右向箭头(和 shadcn 一致),
 * 悬停 `delay` 毫秒后打开,点击切换,右方向键由父浮层的键盘处理逻辑打开。
 */
export function ContextMenuSubTrigger(props: ContextMenuSubTriggerProps) {
  const submenu = useContextMenuSubmenu("ContextMenuSubTrigger");
  const [local, rest] = splitProps(props, [
    "class",
    "children",
    "inset",
    "disabled",
    "label",
    "openOnHover",
    "delay",
    "closeDelay",
    "onClick",
  ]);

  const disabled = () => !!local.disabled || submenu.disabled();

  let element: HTMLDivElement | undefined;

  const entry = useContextMenuEntry({
    component: "ContextMenuSubTrigger",
    element: () => element,
    disabled,
    label: () => local.label,
    hasPopup: () => true,
    openPopup: (reason, event) => submenu.openSubmenu(reason, event),
  });

  onMount(() => {
    submenu.setItemId(entry.id);
    onCleanup(() => submenu.setItemId(undefined));
  });

  // 父浮层里"当前打开的子菜单"一旦不是自己(比如高亮移到了别的项),
  // 就同步关掉自己,保证同一时间只有一个子菜单。
  // `openedViaParent` 用来区分"由本触发器经父浮层打开的"与"受控模式直接打开的",
  // 后者不会被这里的同步逻辑误关。
  let openedViaParent = false;
  createEffect(() => {
    const state = submenu.parentPopup.openPopupState();
    if (state?.id === entry.id) {
      openedViaParent = true;
      return;
    }
    if (openedViaParent && submenu.open()) {
      openedViaParent = false;
      submenu.setOpen(false, state ? "sibling-open" : "list-navigation");
      return;
    }
    if (!submenu.open()) openedViaParent = false;
  });

  let openTimer: number | undefined;
  const clearOpenTimer = () => {
    if (openTimer !== undefined) {
      window.clearTimeout(openTimer);
      openTimer = undefined;
    }
  };
  onCleanup(clearOpenTimer);

  // 子菜单一旦（以任何方式）打开就撤销还在等待的悬停定时器，
  // 否则它会在之后触发 openSubmenu，把刚关掉的子菜单又打开。
  createEffect(() => {
    if (submenu.open()) clearOpenTimer();
  });

  const handlePointerEnter = () => {
    entry.highlight();
    submenu.cancelClose();
    if (disabled()) return;
    if (local.openOnHover === false || submenu.open()) return;
    clearOpenTimer();
    openTimer = window.setTimeout(() => {
      openTimer = undefined;
      submenu.openSubmenu("trigger-hover");
    }, local.delay ?? 100);
  };

  const handlePointerLeave = () => {
    clearOpenTimer();
    if (submenu.open()) submenu.scheduleClose(local.closeDelay ?? 0);
  };

  const handleClick = (e: MouseEvent) => {
    if (disabled()) return;
    local.onClick?.(e);
    if (e.defaultPrevented) return;

    if (submenu.open()) {
      submenu.closeSubmenu("trigger-press", e, false);
    } else {
      submenu.openSubmenu("trigger-press", e);
    }
  };

  return (
    <div
      ref={(el) => {
        element = el;
        // 子菜单以触发器元素为定位锚点,必须登记到 SubmenuContext,
        // 否则 SubContent 的 positioner 拿不到 reference,浮层会停在
        // 未定位状态(opacity:0)或落到错误坐标。
        submenu.setTrigger(el);
        onCleanup(() => submenu.setTrigger(undefined));
      }}
      id={entry.id}
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={submenu.open() ? "true" : "false"}
      tabIndex={-1}
      data-slot="context-menu-sub-trigger"
      data-inset={local.inset ? "" : undefined}
      data-open={submenu.open() ? "" : undefined}
      data-highlighted={entry.isActive() ? "" : undefined}
      data-disabled={disabled() ? "" : undefined}
      aria-disabled={disabled() ? "true" : undefined}
      onClick={handleClick}
      onPointerEnter={handlePointerEnter}
      onPointerLeave={handlePointerLeave}
      onPointerMove={() => entry.highlight()}
      class={clsx(contextMenuSubTriggerClass, local.class)}
      {...rest}
    >
      {local.children}
      <ChevronRight class="ml-auto" />
    </div>
  );
}
