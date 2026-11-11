import { mergeProps, splitProps } from "solid-js";
import { Separator, type SeparatorProps } from "../../separator";
import { clsx } from "../../utils";
import type { ContextMenuSeparatorProps } from "./ContextMenuSeparator.types";

/**
 * 菜单项之间的分隔线,复用项目已有的 `Separator`(与 SelectSeparator /
 * SidebarSeparator / ButtonGroupSeparator 是同一套写法)。
 *
 * 这里额外补上 `role="separator"` 与 `aria-orientation`:Base UI 的
 * `ContextMenu.Separator` 会暴露这个角色,而通用 Separator 组件本身不设置。
 */
export function ContextMenuSeparator(props: ContextMenuSeparatorProps) {
  const merged = mergeProps({ orientation: "horizontal" } as const, props);
  const [local, rest] = splitProps(merged, ["class", "orientation"]);

  return (
    <Separator
      {...(rest as SeparatorProps)}
      data-slot="context-menu-separator"
      orientation={local.orientation}
      role="separator"
      aria-orientation={local.orientation}
      class={clsx("-mx-1 my-1", local.class)}
    />
  );
}
