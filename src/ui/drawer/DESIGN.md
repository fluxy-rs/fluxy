# Drawer 组件设计方案

> 状态:**P1 已实现**(受控/非受控、modal+锁滚动、四方向、进出场、拖拽关闭、全部部件)
> 移植目标:shadcn Base UI 版 Drawer(底层是 Base UI `Drawer`,原为 Vaul)
> 关联代码:`src/components/drawer/`

## 1. 结构(每个部件一个目录)

```
Drawer（Root）
├── DrawerTrigger / DrawerClose     # 多态，默认 Button
├── DrawerOverlay                   # 遮罩
├── DrawerSwipeHandle               # 拖拽把手
└── DrawerContent                   # Portal + Overlay + Viewport + Popup + Content
    ├── DrawerHeader / DrawerFooter
    └── DrawerTitle / DrawerDescription
```
低层部件 `DrawerPortal` / `DrawerOverlay` / `DrawerSwipeHandle` 也单独导出。

## 2. 状态与交互

- 受控/非受控:`open` / `defaultOpen` / `onOpenChange(open, details)`;`details` 对齐仓库
  既有 `ChangeEventDetails`(`reason` / `cancel()` / `isCanceled`)。
- `swipeDirection`: `down`(默认) / `up` / `left` / `right`,驱动面板定位与进出场方向。
- 面板默认四周留白(边距由 `--drawer-inset` 控制,默认 `0.75rem`)、四角圆角 + 完整描边;
  传 `class="[--drawer-inset:0px]"` 可回到 shadcn 默认的贴边形态。
- `modal`(默认 true):显示遮罩 + **锁定页面滚动** + 点击外部关闭;
  `disablePointerDismissal` 可禁止点击遮罩/外部关闭(仍可 Esc/Close)。
- 打开时:`useScrollLock` 锁文档滚动并拦截浮层外滚轮/触摸,`[data-slot="drawer-popup"]` 内可滚动。
- 进出场:面板用 `transform` 过渡(按方向移出视口);打开时先以关闭位置挂载、rAF 后切到打开位置。
  关闭时保留挂载直到 `transitionend` 再卸载。
- 拖拽关闭:pointer 起手 → 沿轴位移 → 抬手超过阈值(面板尺寸 30% 或 80px)关闭,否则回弹;
  从可交互元素起手、或内容还能沿拖拽方向继续滚动时不触发。
- 触摸设备:拖拽期间用非 passive 的 `touchmove` 阻止原生滚动,否则浏览器会在第一次 move 后
  接管滚动并派发 `pointercancel`;只拦截沿关闭方向的位移,反方向仍交给内容原生滚动。

## 3. 无障碍

- Popup:`role="dialog"`、`aria-modal`(modal 时)、`aria-labelledby`/`aria-describedby`
  (由 Title/Description 挂载时注册)、`tabindex="-1"`;打开后聚焦面板,关闭后焦点还给触发器。
- Trigger:`aria-haspopup="dialog"`、`aria-expanded`、`aria-controls`、`data-state`。
- 遮罩 `aria-hidden`;Esc 关闭。

## 4. 与 Base UI / Vaul 的差异

- 不依赖 `@base-ui/react`;用仓库的 `useScrollLock` 实现 modal 锁滚动。
- 组件名与 data-slot 对齐 shadcn(`drawer` / `drawer-trigger` / `drawer-content` /
  `drawer-popup` / `drawer-overlay` / `drawer-swipe-handle` 等),`swipeDirection` 取值一致。

## 5. 后续待实现

- **snapPoints / snapPoint / onSnapPointChange / snapToSequentialPoints**:多段吸附。
- **嵌套抽屉与堆叠**(`data-nested-drawer-open` / 父级缩放与 peek)。
- `modal="trap-focus"`;完整的拖拽物理(速度、超调 bleed、`data-expanded`)。
- `DrawerContent` 的 `initialFocus` 等 Base UI 细粒度 props;RTL 细节。
- dev 示例补齐(Snap Points / Nested)与交互走查。
