import type { ParentProps } from "solid-js";
import type { BaseProps } from "../../types";
import type { ContextMenuChangeEventDetails } from "../context-menu.types";

/** ContextMenuRadioGroup props,对齐 Base UI `ContextMenu.RadioGroup` */
export interface ContextMenuRadioGroupProps extends BaseProps, ParentProps {
  /** 受控选中值;不传则内部自管理 */
  value?: any;
  /** 非受控模式下的初始选中值 */
  defaultValue?: any;
  onValueChange?: (value: any, eventDetails: ContextMenuChangeEventDetails) => void;
  /** 整组禁用,内部 RadioItem 一起失效 */
  disabled?: boolean;
  [key: string]: any;
}
