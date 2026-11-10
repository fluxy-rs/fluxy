import { splitProps, type ParentProps } from "solid-js";
import { Portal } from "solid-js/web";

/**
 * ContextMenuPortal props,对齐 Base UI `ContextMenu.Portal`。
 * Solid 的 `<Portal>` 本身不渲染包裹元素,因此 `className` / `style` 之类的
 * 透传属性在这里没有意义,只保留 `container`。
 */
export interface ContextMenuPortalProps extends ParentProps {
  /** 把浮层挂载到哪个父节点,默认 document.body */
  container?: Node;
}

/**
 * 把子节点渲染到 `container`(默认 body)。
 * 注意:`ContextMenuContent` 内部已经自带 Portal,通常不需要再手动包一层;
 * 该组件保留是为了与 shadcn/Base UI 的导出集合保持一致。
 */
export function ContextMenuPortal(props: ContextMenuPortalProps) {
  const [local] = splitProps(props, ["container", "children"]);
  return <Portal mount={local.container}>{local.children}</Portal>;
}
