# Sheet 组件设计方案

> 状态:**P1 完成**(side / showCloseButton / 复用 Dialog 行为)
> 移植目标:shadcn Base UI 版 `sheet`(上游 = `@base-ui/react/dialog` + 贴边样式)
> 关联代码:`src/components/sheet/`
> 依赖声明:见文末「第三方依赖」——上游基于 Base UI Dialog,本仓库**不引入**该运行时,改为复用本仓库自研 Dialog。

## 1. 概述

Sheet 在语义上就是「按屏幕边缘贴靠的 Dialog」。上游 `sheet.tsx` 只是给
`@base-ui/react/dialog` 的 Root/Trigger/Close/Backdrop/Popup/Title/Description
换一套贴边样式与滑入滑出动画,并没有独立的状态机。

本仓库已经有自研 Dialog(受控/非受控、滚动锁定、Overlay 点击关闭、挂载/退场动画、
`aria-labelledby`/`describedby` 注入),因此 Sheet **不重复实现 Dialog**,而是像
`AlertDialog` 那样复用 `DialogSurface` / Dialog context / `useDialogTrigger`,
只补贴边样式与 `data-slot`:

```
Sheet                # = Dialog(open/defaultOpen/onOpenChange/lockScroll)
├── SheetTrigger     # = DialogTrigger 逻辑 + data-slot="sheet-trigger"
└── SheetContent     # = DialogSurface(Portal/Overlay/滚动锁定/动画) + side 样式
    ├── SheetHeader
    │   ├── SheetTitle        # 通过 DialogContentContext 注入 aria-labelledby
    │   └── SheetDescription  # 注入 aria-describedby
    ├── SheetFooter
    └── SheetClose            # 右上角关闭(可关掉) + 作为普通按钮使用
```

## 2. SRP 分工

| 关注点               | 文件                                                                           |
| -------------------- | ------------------------------------------------------------------------------ |
| 类型契约             | `sheet.types.ts`                                                               |
| 根 / Trigger / Close | `Sheet/`、`SheetTrigger/`、`SheetClose/`                                       |
| 贴边面板             | `SheetContent/`(`.styles.ts` 负责 side 与动画类)                               |
| 结构部件             | `SheetHeader/`、`SheetFooter/`                                                 |
| 标题/描述            | `SheetTitle/`、`SheetDescription/`(复用 `DialogContentContext`)                |
| 行为外壳             | `~/components/dialog/DialogSurface`(Portal、Overlay、滚动锁定、卸载动画、ARIA) |

## 3. 已实现行为

- **贴边**:`SheetContent side="top" | "right" | "bottom" | "left"`(默认 right),
  用 `data-side` + 方向类控制定位/边框/宽度(`w-3/4 sm:max-w-sm` 等)。
- **关闭**:右上角关闭按钮默认显示,`showCloseButton={false}` 关闭;`SheetClose` 也可
  带 children 当普通按钮(如 footer 的 Cancel)。点击 Overlay 关闭(沿用 Dialog)。
- **滚动锁定**:沿用 Dialog 的 `useScrollLock`(锁文档滚动 + 拦截浮层外滚轮/触摸,
  内容区自身可滚)。
- **动画**:打开 `animate-in` + `fade-in-0` + `slide-in-from-<side>`;关闭
  `animate-out` + `fade-out-0` + `slide-out-to-<side>`;由 `DialogSurface` 的
  `onAnimationEnd` 在退场结束后卸载。
- **ARIA**:`role="dialog"`,`SheetTitle`/`SheetDescription` 通过
  `DialogContentContext` 注入 id,接到 `aria-labelledby`/`aria-describedby`。
- **data-slot**:`sheet` / `sheet-trigger` / `sheet-close` / `sheet-content` /
  `sheet-header` / `sheet-footer` / `sheet-title` / `sheet-description`。
- **性能**:无自有状态机,复用 Dialog 的信号;SheetContent 只在 `side`/`showCloseButton`
  变化时重渲染,面板内容不因父子组件重绘。

## 4. 与上游的差异

- 上游用 Base UI 的 `data-starting-style` / `data-ending-style` 做进出场;本仓库统一用
  既有 `animate-in` / `animate-out` 工具类 + `onAnimationEnd` 卸载。
- 上游 `cn-sheet-*` 主题 token 未移植,颜色沿用本仓库 Dialog 的
  `bg-white dark:bg-neutral-900` / `text-neutral-*` 与 `border`。
- 上游把 `SheetOverlay`/`SheetPortal` 作为内部实现(未导出);本仓库由 `DialogSurface`
  内部承担,同样不单独导出。

## 5. 第三方依赖

上游 Sheet 的全部行为来自 **`@base-ui/react/dialog`**。本仓库按约定不引入上游运行时,
改为复用自研 Dialog。因此 Sheet 的能力边界 = 本仓库 Dialog 的能力边界,以下
**Base UI Dialog 提供、但本仓库 Dialog/Sheet 目前未实现**的能力,需要第三方或后续补齐
(补齐时应同时惠及 Dialog / AlertDialog / Drawer):

| 能力                                                           | 上游来源       | 现状                                                                                   | 未来实现                                                     |
| -------------------------------------------------------------- | -------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| 焦点陷阱与最终焦点(`initialFocus`/`finalFocus`)                | Base UI Dialog | 打开时 `DialogSurface` 用 rAF 把焦点移入面板；**尚未实现焦点陷阱与关闭后还原触发元素** | 在 DialogSurface 内实现焦点循环 + 记录触发元素并在关闭时还原 |
| Escape 关闭(`closeOnEscape`)                                   | Base UI Dialog | **已实现**：`Dialog` 的 `dismissOnEscape`（默认 true），Sheet 继承                     | —                                                            |
| 背景惰性化(其余内容 `inert` / `aria-hidden`)                   | Base UI Dialog | `aria-modal="true"` + 滚动锁定；**未把背景内容设为 inert**                             | 打开时给根内容加 `inert`/`aria-hidden`,关闭时还原            |
| `onOpenChangeComplete`、`disablePointerDismissal` 等细粒度 API | Base UI Dialog | 仅 `open`/`defaultOpen`/`onOpenChange`/`lockScroll`                                    | 按需在 Dialog 上扩展                                         |
| 嵌套 Dialog 栈、Portal 容器、`keepMounted`                     | Base UI Dialog | 未特殊处理                                                                             | 引入 dialog 栈与可配置 portal 容器                           |

> 结论:Sheet 的贴边、关闭按钮、Escape 关闭、滚动锁定、进出场动画、ARIA 已在本仓库内实现,
> **未新增任何运行时依赖**;上表剩余的焦点陷阱/焦点还原/背景惰性化属于本仓库 Dialog 的既有缺口,
> 未来优先在 Dialog 层补齐。

## 6. 示例

`dev/examples/sheet.tsx`:Demo(右侧表单)、Side(top/right/bottom/left)、
No Close Button(`showCloseButton={false}`)。
