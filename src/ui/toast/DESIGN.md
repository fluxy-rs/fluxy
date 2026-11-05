# Toast 组件设计方案

> 状态:**P1 完成**(命令式 API、promise、自动/滑动关闭、多视口)
> 移植目标:Sonner 的 API 与交互(样式为本仓库自实现)
> 关联代码:`src/components/toast/`

## 1. 结构

```
toast (state/toast.ts)      # 命令式入口:toast()/success/error/promise/custom...
├── state/store.ts          # 唯一数据源:createToast / dismissToast / removeToast
├── state/promise-toast.ts  # loading → success/error 编排
└── Toaster (Toaster/)      # Portal + 按位置分视口
    └── ToastViewport       # 展开/收起、热键、可见条数
        └── Toast           # 单条:生命周期、滑动、关闭、图标/操作
```

## 2. SRP 分工

| 关注点                      | 文件                                                                |
| --------------------------- | ------------------------------------------------------------------- |
| 类型契约                    | `Toast/Toast.types.ts`、`Toaster/Toaster.types.ts`                  |
| 数据源与增删                | `state/store.ts`                                                    |
| 命令式 API                  | `state/toast.ts`                                                    |
| promise 编排                | `state/promise-toast.ts`                                            |
| 视口选择/展开/热键          | `Toaster/useToaster.ts`、`ToastViewport.tsx`                        |
| 单条生命周期(自动关闭/退场) | `Toast/useToastLifecycle.ts`                                        |
| 滑动关闭                    | `Toast/Toast.tsx` + `Toast.utils.ts`                                |
| 展示部件                    | `ToastContent/`、`ToastIcon/`、`ToastActions/`、`ToastCloseButton/` |

## 3. 状态模型

- 全局只有一个 `createStore<ToastT[]>`；`createToast` 是唯一写入入口。
- 同 id 再次调用：`Object.assign` 原地更新（`message` 缺省时保留原标题），不新增。
- 新 toast `unshift` 到队首，列表顺序天然为“新 → 旧”。
- `dismissToast` 只标记 `delete = true`；真正 `removeToast` 在退场动画结束后。
- `promise`：有 `loading` 时先建 loading；settle 后用同一 id 更新为 success/error；
  resolver 返回 `undefined` 表示“不留提示”，收起 loading。

## 4. 渲染与布局

- `Toaster` 默认 `bottom-right`，按 `position` 把 toast 分到不同视口；同一视口内只渲染
  `visibleToasts`（默认 3）条。
- `gap` 默认 14；`offset/mobileOffset` 控制视口边距；`expand` 初始展开。
- 热键默认 `Alt+T` 切换展开。
- `icons` 可覆盖图标；`closeButton` 显示关闭按钮。
- `theme` 已弃用：主题由 `<html data-theme>` 决定。

## 4.1 视觉配方（2026-11 调整）

旧实现用 `border` + 原生调色板（`emerald-500/60`、`blue-500/60`、`amber-500/60`、`red-500/60`）
标状态，和仓库其余浮层不是一套：

| 维度     | 旧 Toast                                       | Popover / 菜单              | 调整后                                            |
| -------- | ---------------------------------------------- | --------------------------- | ------------------------------------------------- |
| 描边     | 1px `border` + 彩色                            | `ring-1 ring-foreground/10` | 统一用中性灰 ring；彩色边框由使用方覆盖           |
| 内边距   | `p-4`                                          | `p-2.5`                     | `px-3 py-2.5`                                     |
| 状态色   | 原生调色板，且只染边框                         | 用 `--destructive` 等 token | 图标用 `--success/--warning/--info/--destructive` |
| 图标     | `size-5` 容器但 lucide 默认 24px 溢出          | `[&_svg]:size-4`            | 容器与图标都 16px                                 |
| 圆角     | 关闭按钮 `rounded-full`、操作按钮 `rounded-md` | `rounded-lg` / `rounded-md` | 关闭按钮 `rounded-md`、操作按钮按 Button xs       |
| 表面样式 | 只有 `richColors`，**没有任何样式**            | —                           | `variant`: popover / semantic / colored           |

- 语义 token 加在 `src/styles/index.css`：`:root` 与 `[data-theme="dark"]` 各一份
  `--success` / `--warning` / `--info`，并在 `@theme inline` 映射为
  `--color-success` / `--color-warning` / `--color-info`，因此 `text-success`、
  `bg-warning/10` 这类类名在暗色模式下自动换值。
- **三种表面样式**（`ToastVariant`）：
  - `popover`（默认）：shadcn 默认长相，`bg-popover` + `text-popover-foreground`，
    只有图标使用语义色；
  - `semantic`：10% 状态色混入 `--popover` 的实色浅底 + 语义色标题；
  - `colored`：实色状态底（`bg-success` 等）+ 对应的 `*-foreground` 文字，
    `default` / `loading` 用 `bg-primary` + `text-primary-foreground`。
    样式在 `toastVariants` 的 `compoundVariants` 里按 `variant + type` 组合，
    `variant` / `type` 变体本身不产出类名。
- **变体优先级**：单条 `toast.variant` > `toastOptions.variant` > `Toaster.variant` >
  旧 `richColors`（`true` 等价 `semantic`）> 默认 `popover`。`richColors` 标为
  deprecated，保留只为兼容 Sonner 风格的调用。
- **semantic 的底色必须不透明**：写成 `bg-success/10` 会让表面只有 10% 不透明度，
  折叠堆叠时后面的 toast 会透出来（曾经的缺陷）。改用
  `bg-[color-mix(in_oklab,var(--popover),var(--success)_10%)]`，并且跟随
  `data-theme="dark"`。
- **必须用 oklab 而不是 oklch**：`--popover` 是消色（chroma = 0、hue = 0），
  oklch 的极坐标插值会把 90% 白色的 hue 0 与 10% 状态色的 hue 混在一起，
  蓝色/绿色都会被拉成偏红；oklab 是直角坐标，只按 a/b 轴混合，能保住状态色色相。
  （实测：oklch 下 info/success 的背景都落在 hue ≈ 0–27 的红色区间。）
- **描边不随状态变色**：所有 variant 都是中性的 `ring-1 ring-foreground/10`
  （或使用方用 `class` 覆盖成 `ring-0` / `ring-destructive/40`）。状态色只出现在
  图标、semantic 的浅底、colored 的实色底与文字上。
- 图标与关闭按钮的尺寸按仓库图标约定取 16px（`size-4` / `size-3.5`），
  避免 lucide 默认 24px 撑破容器、比正文还大。

## 5. 无障碍

- 每个位置一个 `<ol>` 视口（`data-slot="toaster-viewport"`），带 `aria-live="polite"`、
  `aria-relevant="additions text"`、`aria-atomic="false"`、`aria-label`（`containerAriaLabel` /
  `customAriaLabel`，默认 "Notifications"）；单条 Toast 是 `<li>`，没有额外的 `role="status"`。
- 关闭按钮有 `closeButtonAriaLabel`；纯图标操作必须由使用方提供 `aria-label`。
- 自动关闭时长可通过 `duration` 覆盖；`loading` 类 toast 不应自动关闭。

## 6. 后续

- 把 `Toaster` 的样式 token 化（当前含少量中性色硬编码）。
- 触摸滑动的方向/阈值可配置化（目前 `swipeDirections` 只控制方向集合）。
- dev 示例补全**滑动关闭**场景（promise / custom 已有示例；部件与状态测试见
  `tests/components/toast/`）。
