import { createMemo, createSignal, splitProps } from "solid-js";
import { ContextMenuGroupContext, ContextMenuRadioGroupContext } from "../context-menu.context";
import type {
  ContextMenuGroupContextValue,
  ContextMenuRadioGroupContextValue,
} from "../context-menu.types";
import { createChangeEventDetails } from "../context-menu.utils";
import type { ContextMenuRadioGroupProps } from "./ContextMenuRadioGroup.types";

/**
 * 一组互斥的单选项。与 Base UI 一致:选中某项默认**不**关闭菜单。
 * 选中值通过 context 共享给内部 `<ContextMenuRadioItem>`。
 *
 * RadioGroup 自身就是一个 `role="group"` 容器,因此这里同时提供
 * `ContextMenuGroupContext`:这样 shadcn 示例里把 `<ContextMenuLabel>` 直接
 * 放在 `<ContextMenuRadioGroup>` 内部也能正常工作(标签 id 会写到该组的
 * `aria-labelledby` 上)。组件里若再套一层 `<ContextMenuGroup>`,内层 Label
 * 会关联到最近的组,行为同样正确。
 */
export function ContextMenuRadioGroup(props: ContextMenuRadioGroupProps) {
  const [local, rest] = splitProps(props, [
    "class",
    "children",
    "value",
    "defaultValue",
    "onValueChange",
    "disabled",
  ]);

  const [internalValue, setInternalValue] = createSignal(props.defaultValue);
  const value = createMemo(() => (props.value !== undefined ? props.value : internalValue()));

  const [labelId, setLabelId] = createSignal<string>();
  const groupCtx: ContextMenuGroupContextValue = { labelId, setLabelId };

  const ctx: ContextMenuRadioGroupContextValue = {
    value,
    setValue: (next, event) => {
      if (props.value === undefined) setInternalValue(() => next);
      props.onValueChange?.(next, createChangeEventDetails("item-press", event));
    },
    disabled: () => !!local.disabled,
  };

  return (
    <ContextMenuRadioGroupContext.Provider value={ctx}>
      <ContextMenuGroupContext.Provider value={groupCtx}>
        <div
          role="group"
          aria-labelledby={labelId()}
          data-slot="context-menu-radio-group"
          class={local.class}
          {...rest}
        >
          {local.children}
        </div>
      </ContextMenuGroupContext.Provider>
    </ContextMenuRadioGroupContext.Provider>
  );
}
