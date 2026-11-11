/**
 * 各子组件的类名常量。直接对齐 shadcn base 版 `context-menu.tsx`,
 * 唯一的系统性改动是把 `focus:` 系列变体换成 `data-highlighted:`
 * ——本实现的焦点始终在 popup 上,高亮项通过 `aria-activedescendant` +
 * `data-highlighted` 表达,而不是真实聚焦到每一项。
 */

/** 普通菜单项 */
export const contextMenuItemClass =
  "group/context-menu-item relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-inset:pl-8 data-[variant=destructive]:text-destructive data-[variant=destructive]:data-highlighted:bg-destructive/10 data-[variant=destructive]:data-highlighted:text-destructive dark:data-[variant=destructive]:data-highlighted:bg-destructive/20 data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 data-highlighted:bg-accent data-highlighted:text-accent-foreground data-highlighted:*:[svg]:text-accent-foreground data-[variant=destructive]:*:[svg]:text-destructive";

/** 子菜单触发器(带一个自带的 ChevronRight 图标) */
export const contextMenuSubTriggerClass =
  "flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-inset:pl-8 data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-accent data-highlighted:text-accent-foreground data-open:bg-accent data-open:text-accent-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

/** 可勾选项(CheckboxItem / RadioItem 共用),右侧预留指示器位置 */
export const contextMenuCheckableItemClass =
  "relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-8 pl-2 text-sm outline-hidden select-none data-inset:pl-8 data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 data-highlighted:bg-accent data-highlighted:text-accent-foreground";

/** GroupLabel */
export const contextMenuLabelClass =
  "px-2 py-1.5 text-xs font-medium text-muted-foreground data-inset:pl-8";

/** 快捷键提示 */
export const contextMenuShortcutClass =
  "ml-auto text-xs tracking-widest text-muted-foreground group-data-[highlighted]/context-menu-item:text-accent-foreground";

/** 勾选/单选指示器的定位容器 */
export const contextMenuIndicatorClass =
  "pointer-events-none absolute right-2 flex items-center justify-center";
