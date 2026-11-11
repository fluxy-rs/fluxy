import { createEffect, on, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { ContextMenuPopupSurface } from "../ContextMenuContent/ContextMenuPopupSurface";
import { createContextMenuPopupRuntime } from "../ContextMenuContent/useContextMenuContent";
import { useContextMenuSubmenu } from "../context-menu.context";
import { cancelSubmenuCloseChain } from "../context-menu.utils";
import type { ContextMenuSubContentProps } from "./ContextMenuSubContent.types";

/**
 * 子菜单的浮层内容。定位锚点是子菜单触发器,而不是鼠标坐标;
 * 其余(键盘导航、高亮、关闭行为)与根菜单共用同一套运行时。
 */
export function ContextMenuSubContent(props: ContextMenuSubContentProps) {
  const submenu = useContextMenuSubmenu("ContextMenuSubContent");
  const [local, rest] = splitProps(props, [
    "class",
    "style",
    "children",
    "side",
    "align",
    "sideOffset",
    "alignOffset",
    "collisionPadding",
    "dir",
    "finalFocus",
    "onKeyDown",
    "onPointerEnter",
    "onPointerLeave",
  ]);

  const side = () => local.side ?? "right";
  const align = () => local.align ?? "start";

  const runtime = createContextMenuPopupRuntime({
    root: submenu.root,
    open: submenu.open,
    parent: submenu.parentPopup,
    isSubmenu: true,
    reference: submenu.trigger,
    submenu,
    loopFocus: submenu.loopFocus,
    orientation: submenu.orientation,
    highlightItemOnHover: submenu.highlightItemOnHover,
    side,
    align,
    sideOffset: () => local.sideOffset ?? 0,
    alignOffset: () => local.alignOffset ?? 4,
    collisionPadding: () => local.collisionPadding ?? 5,
    dir: () => local.dir,
  });

  // 子菜单关闭后把焦点还给父浮层,并重新高亮触发器。
  // (通过鼠标移出/高亮切换关闭时不会走 closeSubmenu 的 focusTrigger 分支)
  createEffect(
    on(
      submenu.open,
      (isOpen, previous) => {
        if (!previous || isOpen) return;
        if (!submenu.root.open()) return;
        const id = submenu.itemId();
        if (!id) return;
        submenu.parentPopup.setActiveId(id);
        submenu.parentPopup.popup()?.focus({ preventScroll: true });
      },
      { defer: true },
    ),
  );

  return (
    <ContextMenuPopupSurface
      runtime={runtime}
      dataSlot="context-menu-sub-content"
      contentId={runtime.popupCtx.menuId}
      open={submenu.open}
      side={side}
      dir={() => local.dir}
      registerMenuElement={submenu.root.registerMenuElement}
      class={clsx("shadow-lg", local.class)}
      style={local.style}
      onKeyDown={local.onKeyDown}
      onPointerEnter={() => {
        cancelSubmenuCloseChain(submenu);
        local.onPointerEnter?.();
      }}
      onPointerLeave={() => {
        submenu.scheduleClose();
        local.onPointerLeave?.();
      }}
      rest={rest}
    >
      {local.children}
    </ContextMenuPopupSurface>
  );
}
