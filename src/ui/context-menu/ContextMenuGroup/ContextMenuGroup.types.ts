import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

/**
 * ContextMenuGroup props,对齐 Base UI `ContextMenu.Group`。
 * 内部通过 context 把 label 的 id 关联到 `aria-labelledby`。
 */
export interface ContextMenuGroupProps extends BaseProps, ParentProps {
  [key: string]: any;
}
