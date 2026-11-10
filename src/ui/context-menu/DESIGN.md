# ContextMenu 组件设计方案

> 状态:**已实现**(右键/长按触发、子菜单、checkbox/radio、typeahead、模态锁滚动)
> 移植目标:Base UI `ContextMenu` + shadcn 的样式与 `data-slot`
> 关联代码:`src/components/context-menu/`

## 1. 结构

```
ContextMenu                 # 不渲染 DOM:开合、虚拟锚点、菜单元素集合、锁滚动
├── ContextMenuTrigger      # 默认 div;右键/长按 → openAt(clientX, clientY)
└── ContextMenuPortal
    └── ContextMenuContent  # 两层:外层 translate 定位,内层 role=menu + 动画
        ├── ContextMenuItem / CheckboxItem / RadioGroup / RadioItem
        ├── ContextMenuGroup / Label / Separator / Shortcut
        └── ContextMenuSub
            ├── ContextMenuSubTrigger
            └── ContextMenuSubContent
```

根菜单与每个子菜单各自有 `ContextMenuPopupContextValue`；`ContextMenuSubmenuContextValue`
负责父子联动。

## 2. SRP 分工

| 关注点                                         | 文件                                                                                                         |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| 类型契约                                       | `context-menu.types.ts`                                                                                      |
| Root / Popup / Submenu / Radio / Group context | `ContextMenu/ContextMenu.context.ts`、`context-menu.context.ts`                                              |
| Root 状态(开合/锚点/锁滚动)                    | `ContextMenu/ContextMenu.tsx`                                                                                |
| 触发(右键/长按/键盘)                           | `ContextMenuTrigger/useContextMenuTrigger.ts`、`context-menu.long-press.ts`、`context-menu.trigger.utils.ts` |
| 浮层运行时                                     | `ContextMenuContent/useContextMenuContent.ts`                                                                |
| 定位                                           | `ContextMenuContent/context-menu.positioner.ts`（positioner + size）                                         |
| 展示骨架                                       | `ContextMenuContent/ContextMenuPopupSurface.tsx`                                                             |
| 菜单项集合                                     | `ContextMenuContent/context-menu.popup-items.ts`（DOM 顺序、可用项）                                         |
| 高亮导航                                       | `ContextMenuContent/context-menu.highlight.ts`                                                               |
| 菜单项注册/高亮                                | `context-menu.entry.ts`（`useContextMenuEntry`）                                                             |
| 键盘映射(纯)                                   | `context-menu.keyboard.ts`                                                                                   |
| 字符导航(纯)                                   | `context-menu.typeahead.ts`                                                                                  |
| 子菜单状态                                     | `ContextMenuSub/ContextMenuSub.tsx`                                                                          |
| 事件详情/定位工具                              | `context-menu.utils.ts`                                                                                      |

## 3. 交互模型

- **锚点**：右键坐标用 `createVirtualElement({ x, y, contextElement: trigger })`；子菜单锚点是
  `SubTrigger` 元素。
- **焦点**：焦点落在 popup（`tabindex=-1`），高亮项通过 `aria-activedescendant` +
  `data-highlighted` 表达；不把焦点在菜单项之间搬。
- **开合 reason**：`trigger-press`、`outside-press`、`escape-key`、`item-press`、
  `sibling-open`、`cancel-open` 等；`details.cancel()` 阻止关闭。
- **方向键**：纵向菜单 ↑↓ 移动、→ 打开子菜单、← 退回父菜单；横向菜单左右移动。
  Home/End 首尾，loopFocus 控制环绕。
- **typeahead**：500ms 窗口内累积前缀，大小写不敏感；命中后不清空缓冲（可继续输入更长前缀）。
- **子菜单**：悬停延迟打开（默认 100ms）、移出延迟关闭（默认 0，另有 100ms 穿越容忍）；
  `closeParentOnEsc` 控制 Escape 是否连父菜单一起关。
- **模态**：`modal` 默认 true，只锁页面滚动；菜单内部（`context-menu-content` /
  `context-menu-sub-content`）仍可滚动。焦点陷阱/背景 inert 尚未实现。

## 4. 无障碍

- Content：`role="menu"`、`tabindex="-1"`、`aria-activedescendant`。
- Item：`role="menuitem"` / `menuitemcheckbox` / `menuitemradio`、`aria-disabled`、
  `data-highlighted`；纯图标项需要 `label` 或 `aria-label` 供 typeahead/读屏使用。
- Trigger：默认 `tabIndex=0`、`data-slot="context-menu-trigger"`、`data-popup-open`、
  `data-pressed`；**不设置** `aria-haspopup` / `aria-expanded` / `aria-controls`（这是 ContextMenu
  与 DropdownMenu 的一处差异，后者由 Trigger 提供这三项）。

## 5. 与 DropdownMenu 的关系

DropdownMenu 复用本组件的浮层运行时、菜单项注册表、键盘与子菜单状态，只替换：
锚点（trigger 元素而非鼠标坐标）、`data-slot`、部分视觉类名。详见
[DropdownMenu DESIGN](../dropdown-menu/DESIGN.md)。

## 6. 后续

- 抽出共享 `menu/` 核心，消除 DropdownMenu 对 `context-menu/*` 内部文件的依赖。
- 焦点陷阱与关闭后的焦点还原策略补齐（与 Dialog 一起做）。
- 虚拟化/超长菜单、`indeterminate` checkbox 等细节。
