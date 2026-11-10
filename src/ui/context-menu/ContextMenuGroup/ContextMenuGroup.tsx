import { createSignal, splitProps } from "solid-js";
import { ContextMenuGroupContext } from "../context-menu.context";
import type { ContextMenuGroupContextValue } from "../context-menu.types";
import type { ContextMenuGroupProps } from "./ContextMenuGroup.types";

/**
 * 把相关菜单项分成一组,配合 `<ContextMenuLabel>` 使用。
 * 组与标签之间通过 `aria-labelledby` 建立无障碍关联。
 */
export function ContextMenuGroup(props: ContextMenuGroupProps) {
  const [local, rest] = splitProps(props, ["class", "children"]);

  const [labelId, setLabelId] = createSignal<string>();
  const ctx: ContextMenuGroupContextValue = { labelId, setLabelId };

  return (
    <ContextMenuGroupContext.Provider value={ctx}>
      <div
        role="group"
        aria-labelledby={labelId()}
        data-slot="context-menu-group"
        class={local.class}
        {...rest}
      >
        {local.children}
      </div>
    </ContextMenuGroupContext.Provider>
  );
}
