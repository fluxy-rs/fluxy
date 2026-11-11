import { createMemo, createSignal, onCleanup, type JSX } from "solid-js";
import { useContextMenuContext } from "../ContextMenu/ContextMenu.context";
import { ContextMenuSubmenuContext, useContextMenuPopup } from "../context-menu.context";
import type {
  ContextMenuChangeEventReason,
  ContextMenuSubmenuContextValue,
} from "../context-menu.types";
import { createChangeEventDetails } from "../context-menu.utils";
import type { ContextMenuSubProps } from "./ContextMenuSub.types";

/** 鼠标穿越触发器与子菜单之间的间隙时的防抖下限(毫秒) */
const HOVER_CLOSE_GRACE = 100;

/**
 * 子菜单根组件:不渲染 DOM,维护子菜单的开关状态、定位锚点(触发器)与
 * 与父浮层之间的联动(同一时间只允许一个子菜单打开)。
 */
export function ContextMenuSub(props: ContextMenuSubProps): JSX.Element {
  const root = useContextMenuContext("ContextMenuSub");
  const parentPopup = useContextMenuPopup("ContextMenuSub");

  const [internalOpen, setInternalOpen] = createSignal(props.defaultOpen ?? false);
  const open = createMemo(() => (props.open !== undefined ? props.open : internalOpen()));
  const disabled = createMemo(() => !!props.disabled);
  const closeParentOnEsc = createMemo(() => !!props.closeParentOnEsc);
  const loopFocus = createMemo(() => props.loopFocus ?? root.loopFocus());
  const orientation = createMemo(() => props.orientation ?? root.orientation());
  const highlightItemOnHover = createMemo(
    () => props.highlightItemOnHover ?? root.highlightItemOnHover(),
  );

  const [trigger, setTrigger] = createSignal<HTMLElement>();
  const [itemId, setItemId] = createSignal<string>();

  let closeTimer: number | undefined;
  const cancelClose = () => {
    if (closeTimer !== undefined) {
      window.clearTimeout(closeTimer);
      closeTimer = undefined;
    }
  };
  onCleanup(cancelClose);

  const setOpen = (next: boolean, reason: ContextMenuChangeEventReason, event?: Event) => {
    const details = createChangeEventDetails(reason, event, trigger());
    // 同根组件：先回调，cancel() 时不落状态
    props.onOpenChange?.(next, details);
    if (details.isCanceled) return;
    if (props.open === undefined) setInternalOpen(next);
  };

  const closeSubmenu = (
    reason: ContextMenuChangeEventReason,
    event?: Event,
    focusTrigger = false,
  ) => {
    const id = itemId();
    cancelClose();
    // 先关自己再清父浮层的登记:写信号会同步刷新相关 effect,若反过来,
    // 子菜单触发器的同步 effect 会抢先以 "list-navigation" 的 reason 关掉它,
    // 覆盖掉这里想上报的真实关闭原因。
    if (open()) setOpen(false, reason, event);
    if (id && parentPopup.openPopupState()?.id === id) {
      parentPopup.closeOpenPopup();
    }

    if (focusTrigger && id) {
      // 焦点还给父浮层,并重新高亮触发器,键盘可以继续在父菜单里导航
      parentPopup.setActiveId(id);
      parentPopup.popup()?.focus({ preventScroll: true });
    }
  };

  const openSubmenu = (reason: ContextMenuChangeEventReason, event?: Event) => {
    if (disabled()) return;
    const id = itemId();
    if (!id) return;
    cancelClose();
    if (open()) return;
    // 先登记到父浮层,父浮层据此保证"同时只有一个子菜单打开"
    parentPopup.openPopup(id, reason, event);
    setOpen(true, reason, event);
  };

  const scheduleClose = (delay = HOVER_CLOSE_GRACE) => {
    cancelClose();
    closeTimer = window.setTimeout(
      () => {
        closeTimer = undefined;
        closeSubmenu("trigger-hover");
      },
      Math.max(delay, HOVER_CLOSE_GRACE),
    );
  };

  const ctx: ContextMenuSubmenuContextValue = {
    root,
    parentPopup,
    open,
    setOpen,
    disabled,
    closeParentOnEsc,
    loopFocus,
    orientation,
    highlightItemOnHover,
    trigger,
    setTrigger,
    itemId,
    setItemId,
    openSubmenu,
    closeSubmenu,
    scheduleClose,
    cancelClose,
  };

  return (
    <ContextMenuSubmenuContext.Provider value={ctx}>
      {props.children}
    </ContextMenuSubmenuContext.Provider>
  );
}
