import { createEffect, splitProps } from "solid-js";
import { useContextMenuContext } from "../ContextMenu/ContextMenu.context";
import { ContextMenuPopupSurface } from "./ContextMenuPopupSurface";
import type { ContextMenuContentProps } from "./ContextMenuContent.types";
import { createContextMenuPopupRuntime } from "./useContextMenuContent";

/**
 * ContextMenu 的浮层内容:Portal + Positioner + Popup 三者的组合,
 * 对应 shadcn 的 `<ContextMenuContent>`。
 *
 * 锚点是根组件根据鼠标坐标生成的虚拟元素,所以定位方式是 position:fixed
 * (Portal 到 body 后不受祖先定位影响)。
 */
export function ContextMenuContent(props: ContextMenuContentProps) {
  const root = useContextMenuContext("ContextMenuContent");
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
  ]);

  const side = () => local.side ?? "right";
  const align = () => local.align ?? "start";

  const runtime = createContextMenuPopupRuntime({
    root,
    open: root.open,
    isSubmenu: false,
    reference: root.anchor,
    side,
    align,
    sideOffset: () => local.sideOffset ?? 0,
    alignOffset: () => local.alignOffset ?? 4,
    collisionPadding: () => local.collisionPadding ?? 5,
    dir: () => local.dir,
  });

  // Popup 的 finalFocus 是浮层级配置,但焦点回收由根组件统一执行,
  // 因此这里把值同步给根状态。
  createEffect(() => root.setFinalFocus(local.finalFocus !== false));

  return (
    <ContextMenuPopupSurface
      runtime={runtime}
      dataSlot="context-menu-content"
      contentId={root.contentId}
      open={root.open}
      side={side}
      dir={() => local.dir}
      registerMenuElement={root.registerMenuElement}
      class={local.class}
      style={local.style}
      onKeyDown={local.onKeyDown}
      rest={rest}
    >
      {local.children}
    </ContextMenuPopupSurface>
  );
}
