import { splitProps, type ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { clsx, mergeRefs } from "../../utils";
import { useContextMenuTrigger } from "./useContextMenuTrigger";
import type { ContextMenuTriggerProps } from "./ContextMenuTrigger.types";

/**
 * ContextMenu 的触发区域,默认渲染 `<div>`,支持通过 `component` 多态渲染成
 * 任意标签或组件(与 TooltipTrigger 一致)。通过以下方式打开菜单:
 * - 右键(contextmenu 事件),锚定到鼠标坐标
 * - 触摸/手写笔长按 500ms,锚定到按下坐标
 * - 键盘 Shift+F10 或菜单键,锚定到触发器中心
 *
 * 事件逻辑在 `useContextMenuTrigger` 里用 `addEventListener` 绑定,因此调用方
 * 自己传入的 `onClick` / `onContextMenu` 不会被覆盖。
 */
export const ContextMenuTrigger = <T extends ValidComponent = "div">(
  props: ContextMenuTriggerProps<T>,
) => {
  const { ctx, attachListeners } = useContextMenuTrigger();

  // class / classList 必须显式摘出来并一起传给 Dynamic：
  // `{...props}` 之后再写显式 `class=` 会用 node.className 覆盖掉 classList
  const [local, others] = splitProps(props as ContextMenuTriggerProps, [
    "component",
    "class",
    "classList",
    "ref",
    "tabIndex",
  ]);

  return (
    <Dynamic
      {...others}
      component={(local.component as ValidComponent) ?? "div"}
      ref={mergeRefs(
        (el) => ctx.setTrigger(el as HTMLElement),
        local.ref as Element | ((el: Element) => void) | undefined,
        attachListeners,
      )}
      data-slot="context-menu-trigger"
      data-popup-open={ctx.open() ? "" : undefined}
      data-pressed={ctx.open() ? "" : undefined}
      tabIndex={local.tabIndex ?? 0}
      class={clsx("select-none", local.class)}
      classList={local.classList}
    />
  );
};
