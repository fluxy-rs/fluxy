import type { ValidComponent } from "solid-js";
import type { PolymorphicProps } from "../../types";

/**
 * ContextMenuTrigger props,对齐 Base UI `ContextMenu.Trigger`。
 * 与 TooltipTrigger / Button 一致支持多态渲染:通过 `component` 指定真实标签或
 * 组件,默认渲染 `<div>`(与 shadcn 的用法一致)。
 *
 * @example
 * ```tsx
 * <ContextMenuTrigger>右键区域</ContextMenuTrigger>
 * <ContextMenuTrigger component="button">按钮触发器</ContextMenuTrigger>
 * <ContextMenuTrigger component={A} href="/docs">链接触发器</ContextMenuTrigger>
 * ```
 */
export type ContextMenuTriggerProps<T extends ValidComponent = "div"> = PolymorphicProps<T>;
