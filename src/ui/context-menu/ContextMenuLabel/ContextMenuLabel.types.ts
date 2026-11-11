import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

/** ContextMenuLabel props,对齐 Base UI `ContextMenu.GroupLabel` + shadcn 的 inset */
export interface ContextMenuLabelProps extends BaseProps, ParentProps {
  /** 左侧对齐到带图标项的文字位置 */
  inset?: boolean;
  [key: string]: any;
}
