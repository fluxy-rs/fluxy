# MessageScroller 组件设计方案

> 状态:**P1 完成**(结构 + 滚动引擎 + 公开 API/行为对齐 + 可见性/锚定/spacer)
> 移植目标:shadcn Base UI 版 `message-scroller`(行为来自 `@shadcn/react` 的 headless 包)
> 关联代码:`src/components/message-scroller/`

## 1. 概述

会话滚动容器。shadcn 的样式层包装的是 `@shadcn/react` 的 headless 原语,本仓库不能依赖它,
因此在 `useMessageScrollerEngine.ts` 里自研滚动引擎(行为按上游 dist 实现逐条移植)。它只负责
**滚动视口**,不拥有消息/AI 状态/持久化。

```
MessageScrollerProvider          # headless 根:滚动状态机 + context(不渲染 DOM)
└── MessageScroller              # 有样式的框架
    ├── MessageScrollerViewport  # 可滚动元素(scroll/意图事件、保位、ARIA region、转发 ref)
    │   └── MessageScrollerContent   # role="log" 的会话容器 + 内部 spacer
    │       └── MessageScrollerItem  # 行边界(id/anchor/可见性)
    └── MessageScrollerButton    # 滚到开始/结束的控件
```

## 2. SRP 分工

| 关注点                   | 文件                                                                                                                                |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| 类型契约                 | `message-scroller.types.ts`                                                                                                         |
| context                  | `message-scroller.context.ts`                                                                                                       |
| 滚动引擎(装配)           | `useMessageScrollerEngine.ts`(信号、行注册表、内容/视口两个 effect、ctx 组装)                                                       |
| DOM 几何测量             | `message-scroller.dom-measure.ts`(`items` / `itemOffsetTop` / `itemTopInViewport` / `contentBottom` / `maxScrollTop`)               |
| 滚动状态机               | `message-scroller.scroll-state.ts`(四种模式与**唯一**迁移入口、可滚动状态、autoscrolling、让位)                                     |
| 滚动命令与 spacer        | `message-scroller.commands.ts`(`setScrollTop` / `scrollToStart` / `scrollToEnd` / `scrollToElement` / `targetTopFor` / spacer 三态) |
| 可见性与阅读锚点         | `message-scroller.visibility.ts`(IO 按需订阅、无 IO 退化、`currentAnchorId` / `visibleMessageIds`)                                  |
| 锚定与保位               | `message-scroller.anchoring.ts`(初次定位、prepend 保位、新回合锚定、重锚定、排队补滚、resize)                                       |
| CSS 尺寸测量(纯)         | `message-scroller.measure.ts`(`parsePx` / `paddingBox` / `rowGap`)                                                                  |
| 锚点查找(纯)             | `message-scroller.anchors.ts`(`firstAnchorFrom` / `firstUnhandledAnchor` / `hasMultipleAnchorsFrom`)                                |
| 滚动目标计算(纯)         | `message-scroller.scroll-target.ts`(`computeTargetTop` / `targetTopFor`,四种 `align`)                                               |
| data 属性工具 / 共享常量 | `message-scroller.utils.ts`(`data-scrollable` 的 token 拼接、`AT_EDGE_TOLERANCE`)                                                   |
| 消费 hooks               | `useMessageScroller.ts`、`useMessageScrollerScrollable.ts`、`useMessageScrollerVisibility.ts`                                       |
| 各部件的渲染/ARIA        | `MessageScroller*/` 各自目录                                                                                                        |
| 滚动渐隐工具             | `src/styles/effects.css`(`scroll-fade` 家族,与 shadcn 同源)                                                                         |

> 2026-09 重构：把原先内联在 `useMessageScrollerEngine.ts`（852 行）里的
> 「CSS 解析 / 锚点查找 / 对齐算法」三块**纯计算**拆成独立模块。
> 它们不读信号、不写 DOM，因此四种 `align` 的每个分支都能脱离 jsdom 布局限制直接断言
> （这三个模块已做到语句/分支/函数三项 100% 覆盖）。
> 引擎只保留状态机与编排，通过 `message-scroller.scroll-target.ts` 的
> `targetTopFor(...)` 适配器调用纯算法。
>
> 2026-10 重构：引擎从 797 行降到 245 行，按 `REFACTOR-PLAN.md` 的边界拆出
> 「DOM 几何测量 / 滚动状态机 / 滚动命令与 spacer / 可见性 / 锚定与保位」五块，
> 引擎只剩信号、行注册表、两个 effect 与 ctx 组装。两条设计约束值得记住：
> **`mode` 只有一个写入者**（`scroll-state` 的 `setFollowing` / `setFree` /
> `anchorTo` / `settleJump`，其它模块不再直接赋值）；**行注册表留在引擎**
> （它同时被 anchoring 与 visibility 需要，搬走会形成循环依赖，注册时由引擎
> 转调 `visibility.observe` 与 `anchoring.notifyMessageRegistered`）。
> 拆分前先把引擎的分支覆盖率从 39.93% 补到 92.7%（220 条用例），
> 再逐块搬迁；五个新模块的语句/函数/行均为 100%，残留的两条复合条件短路侧
> 已按 `TESTING.md` §8 以变异测试确认不可达。

## 3. 已实现行为

- **模式状态机**:`following-bottom` / `free-scrolling` / `anchored-to-message` /
  `settling-jump`。滚轮、触摸、键盘滚动键(Arrow/Page/Home/End/Space)会**立刻放弃跟随**。
- **可滚动状态**:`data-scrollable`(空格分隔 token)与 `useMessageScrollerScrollable()`;
  距边缘多少像素算「在边缘」由 `scrollEdgeThreshold`(默认 8)控制;内容底部**不含 spacer**。
  跟随输出时对外把 `end` 强制为 `false`(按钮隐藏),回到实时边缘后恢复。
- **命令**:`scrollToStart` / `scrollToEnd` / `scrollToMessage(id, options)`;支持
  `align`(start/center/end/nearest)、`behavior`(auto/smooth)、`scrollMargin`。返回 boolean;
  `scrollToMessage` 在会话未挂载任何行时可**排队**,已挂载但 id 缺失则返回 false。
- **尾部 spacer**:`scrollToElement` 时按目标位置计算所需空间,设置隐藏 spacer 的高度
  (并用负 `margin-top` 抵消 content 的 gap),让靠近结尾的锚定行也能滚到顶部;滚到 start/end
  时清零。Content 通过 `spacerClassName` 暴露其类名。
- **新回合锚定**:新增 `scrollAnchor` 行且读者在实时边缘时,用 `scrollToElement(keepPreviousPeek)`
  把它放到顶部并保留 `scrollPreviousItemPeek`;同时记为 `streamingTurn`。回复生长时
  `reanchorToAnchoredMessage` 原地重锚定,直到读者滚动离开。同批多个新锚点则直接跟随底部。
- **`defaultScrollPosition`**:`start` / `end`(默认) / `last-anchor`;`last-anchor` 无锚点或
  该回合放得下时回退 `end`;`end`/`last-anchor` 在应用前用 `data-pending-scroll` 隐藏视口,
  空会话跳过。
- **`preserveScrollOnPrepend`**(Viewport prop,默认 true):记录首个可见行及其视口位置,
  上方插入历史后按差值补偿 `scrollTop`,而不是靠高度差猜测。
- **`autoScroll`**:仅在 `following-bottom` 模式跟随;`scrollToEnd`/按钮重新接管。
- **可见性**:`useMessageScrollerVisibility()` 提供 `currentAnchorId`(阅读行 = 视口顶部 +
  `scrollMargin` + `scrollPreviousItemPeek`,取之上最后一个锚点)/ `visibleMessageIds`;
  **按需订阅**:首个订阅者建立 `IntersectionObserver`(root = Viewport,rootMargin 同上),
  最后一个移除时断开;不支持 IO 时退化为逐帧测量。
- **无障碍**:Viewport 是 `role="region"` + `aria-label` + `tabindex=0` 的键盘可达区域;
  Content 是 `role="log"` + `aria-relevant="additions"` + `aria-busy`;Button 无内容可滚时
  `inert`、`tabindex=-1`、`data-active="false"`;spacer 为 `aria-hidden`。
- **性能**:Item 带 `content-visibility: auto` 与 `contain-intrinsic-size`;滚动热路径只更新
  少量信号与 data 属性,不重渲染行。

## 4. 示例(dev/examples/)

按上游 `apps/v4/examples/base/message-scroller-*.tsx` 逐一移植,结构/文案/数据对齐:
Chat、Anchoring Turns、Group Chat、Keeping Context Visible、Following the Live Edge、
Opening Position、Loading Earlier Messages、Animating New Messages、Jumping to Messages、
Tracking the Reader's Position、Reading Scroll State、Scroll State、Virtualization。

- `message-scroller.tsx` 只做组合与 UI,复用仓库组件(Card/Empty/ToggleGroup/Slider/
  Tabs/Select/Tooltip/DropdownMenu/HoverCard 等);Tracking 的悬浮大纲已改用
  HoverCard;受限于本仓库没有 motion,动画改用 CSS 关键帧。
- `message-scroller-support.tsx`:本地 `createChat()` + `useScriptedChat` 模拟
  `@shadcn/helpers/ai-sdk` 的「提交 → 流式输出」;`MessageAnimated` 对齐上游结构
  (用户行入场动画、段落切分、muted/ghost 变体),预设与 `@/lib/message-animations` 同名。
- `message-scroller.css`:七个动画预设的 CSS keyframes(含 prefers-reduced-motion 兜底)。
- Virtualization 为自研极简窗口化(仓库不引入 TanStack),用 `MessageScrollerViewport`
  作为滚动元素。

## 5. 与 shadcn 示例的差异及原因

dev 示例已按上游 `apps/v4/examples/base/message-scroller-*.tsx` 逐条移植结构与数据,
但**无法逐像素/逐行为复现**,原因分两类:本仓库刻意不引入的上游运行时依赖,以及两套组件
库的 API 约定差异。下面把每一项写成「上游用什么 / 本仓库现状 / 差异 / 未来实现」,供后续补齐。

### 5.1 缺少运行时依赖导致的行为差异

| 上游依赖                                                                       | 用在哪                                                            | 本仓库现状                                                                         | 差异                                                                                                                      | 未来实现                                                                                                                          |
| ------------------------------------------------------------------------------ | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `motion/react`(Motion)                                                         | `components/message-animated.tsx`、`Animating New Messages`       | 自写 `MessageAnimated` + `dev/examples/message-scroller.css` 的 7 个 CSS keyframes | 只有入场动画,无 exit/AnimatePresence、无真 spring 物理、无 layout 动画;切换预设不会重放已挂载行(已在代码里固定挂载时预设) | 增加可选的 motion 适配层:用 CSS `linear()` 近似 spring,或允许使用方注入 motion;补齐 exit 与 `useReducedMotion`                    |
| `@ai-sdk/react` + `@shadcn/helpers/ai-sdk`(`createChat`/`useChat`/`transport`) | Chat、Following the Live Edge、Keeping Context Visible、Animating | 本地 `createChat()` + `useScriptedChat`(定时器模拟提交→流式输出)                   | 无真实 transport/abort/regenerate;无 UIMessage `parts`(reasoning/tool/attachment);`status` 语义为近似                     | 提供 `@neut/ui` 的 chat/streaming 适配(hook + transport 接口),或保留脚本驱动并把接口抽象出来                                      |
| `@tanstack/react-virtual`                                                      | Virtualization                                                    | 自研固定行高窗口化                                                                 | 行高固定、无动态测量/overscan 自适应/scrollMargin 集成                                                                    | 保持「虚拟化在 primitive 之外」的定位,补一个 headless 窗口化 hook 示例,并在 `MessageScrollerViewport` 上确认 `ref` 可透传(已支持) |
| `sonner`(toast)                                                                | Loading Earlier Messages 的「History loaded」提示                 | 未接(dev 未挂载 Toaster)                                                           | 少了加载历史后的 toast 反馈                                                                                               | 在 dev 布局挂载本仓库 `Toaster`,加载完成后调用 toast                                                                              |
| ~~`HoverCard`~~                                                                | Tracking 的悬浮大纲                                               | 已实现 `hover-card` 并接入                                                         | 已消除                                                                                                                    | —                                                                                                                                 |
| `lucide-react`                                                                 | 全部示例图标                                                      | `lucide-solid`                                                                     | 图标名/导出形式不同(`ArrowUpIcon`→`ArrowUp`、`IconPlaceholder`→直接组件)                                                  | 无需处理,属框架差异                                                                                                               |

### 5.2 组件库 API 约定差异(刻意为之,不是缺陷)

上游 Base UI 版示例与本仓库的约定不同,移植时做了等价改写,**视觉与交互应一致**;后续不要
把这些「改写」当成 bug 去改回:

| 上游写法                                         | 本仓库写法                                            | 说明                                                           |
| ------------------------------------------------ | ----------------------------------------------------- | -------------------------------------------------------------- |
| `render={<Button/>}` / `UseRenderComponentProps` | `component={Button}`(多态 `PolymorphicProps`)         | 本仓库统一用 `component` 表达 render 目标                      |
| `IconPlaceholder`                                | 直接 `lucide-solid` 图标                              | 上游为多图标集预览服务                                         |
| `SelectContent side=/align=`                     | `SelectContent placement="top-start"`                 | 本仓库 Select 采用「选中项对齐」定位模型,没有独立 side         |
| `<ToggleGroup value={[role]}>`                   | `<ToggleGroup value={role}>`                          | 本仓库单选模式用标量(`multiple` 才用数组),视觉一致             |
| `<Slider value={[peek]}>`                        | `<Slider value={peek}>`                               | 同上,标量/数组二选一                                           |
| `TooltipTrigger render={<Button/>}`              | `<TooltipTrigger variant size icon aria-label/>`      | 触发器等默认渲染 `Button`                                      |
| 主题类 `cn-message-scroller*`、`cn-button` 等    | 组件内联 Tailwind 类                                  | 上游 `shadcn/tailwind.css` 的主题 token 层未移植               |
| Content 默认间距来自主题                         | `MessageScrollerContent` 内置 `gap-6`(示例再按需覆盖) | 上游 `cn-message-scroller-content` 提供间距,本仓库用固定值近似 |

### 5.3 主题 token 层

上游 `shadcn` 包在全局 CSS 里提供 `@import "shadcn/tailwind.css"`,其中 `cn-*` 类把
组件样式抽成语义 token,可按 theme(base-rhea / base-luma 等)切换;本仓库把样式直接写在
组件里,因此**不同 theme 下的圆角/间距/颜色细节会有出入**。若要完全对齐,需要先引入
一层主题 token(或在 `src/styles` 里维护多套变量),再让组件引用 token。

### 5.4 过程限制

当前环境没有可用的桌面浏览器(浏览器工具报未连接),所有示例只做了 `tsc` + 构建验证,
**滚动/吸附/动画/手势的最终观感未经真机确认**。后续实现时请以 `bun run dev` 手动逐节核对,
尤其是:新回合锚定、spacer 让最后一行顶到顶部、prepend 保位、autoScroll 让位、IO 可见性高亮。

> 2026-10 补充:引擎、五个模块与六个部件现已全部有测试(含把 Provider + 五部件
> 拼起来的整机用例),jsdom 下的行为契约已经固定;但**"看起来对不对"仍然只能靠真机**——
> 上面这份手动核对清单依然有效。部件测试还发现并修掉了一个真实缺陷:
> spacer 的行间距原先在 ref 回调里立即读取,而那时元素可能尚未插入文档
> (jsdom 会抛错,浏览器只会把行间距当成 0);现已改为写高度时惰性读取。

## 6. 未纳入(组件本身)

- **SSR 防闪烁脚本**:`data-pending-scroll` 已实现;把它接到内联脚本属于使用方页面代码
  (`end` 时先滚到底再移除属性),组件不内置 `<script>`。
- `scroll-fade` 依赖 CSS scroll-driven animation,不支持该特性的浏览器退化为静态渐隐
  (上游同款兜底)。

## 7. 未来实现清单(建议优先级)

1. **motion 适配层 / 更强 CSS 动画**:补 exit 与 spring,消掉 `Animating` 与上游的最大差异。
2. ~~**HoverCard 组件**~~:已实现 `hover-card`,`Tracking the Reader's Position` 已改用
   上游的悬浮大纲(`HoverCardTrigger` + `HoverCardContent` + 列表)。
3. **toast(sonner 对应物)**:dev 挂载 `Toaster`,补 `Loading Earlier Messages` 的提示。
4. **chat/streaming 适配接口**:把 `useScriptedChat` 的接口抽成 transport,未来可接真实后端。
5. **主题 token 层**:若要支持多 theme,把组件内联样式迁到语义 token。
6. **虚拟化参考实现**:可选,保持 primitive 中立。
