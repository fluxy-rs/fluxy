import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

/** ContextMenuSeparator props,对齐 Base UI `ContextMenu.Separator` */
export interface ContextMenuSeparatorProps extends BaseProps, ParentProps {
  /** 分隔线方向,默认 'horizontal'(shadcn 仅内置横向样式) */
  orientation?: "horizontal" | "vertical";
  [key: string]: any;
}
