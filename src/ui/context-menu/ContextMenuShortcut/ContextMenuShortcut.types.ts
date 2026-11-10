import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";

/** ContextMenuShortcut props;本质是一个带样式的 `<span>` */
export interface ContextMenuShortcutProps extends BaseProps, ParentProps {
  [key: string]: any;
}
