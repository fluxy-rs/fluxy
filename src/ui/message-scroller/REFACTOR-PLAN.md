# useMessageScrollerEngine 拆分设计（待评审）

> 目标：把 797 行的滚动引擎按 SRP 拆成「保留编排 + 三块独立职责」，同时**先把测试网补起来**。
> 状态：**已执行完毕（2026-10）**。§2 的三条决策按评审意见落地：
> `mode` 只暴露语义化迁移、`dom-measure` 独立成文件、搬迁期间不改行为。
> 执行结果：引擎 797 → 245 行；新增 `dom-measure` / `scroll-state` / `commands` /
> `visibility` / `anchoring` 五个模块；测试从 86 条（引擎 39.93% 分支）增至 220 条
> （引擎与五个模块语句/函数/行 100%，两条不可达分支按 §8 登记）。
> 本文件保留作为"当时为什么这么切"的记录，最新的分工表见 `DESIGN.md` §2。

## 0. 现状与判据

### 0.1 规模与覆盖率（实测）

| 文件                                                    | 行数 | 语句  | 分支      | 函数  | 行    |
| ------------------------------------------------------- | ---- | ----- | --------- | ----- | ----- |
| `useMessageScrollerEngine.ts`                           | 797  | 57.08 | **39.93** | 58.62 | 59.86 |
| `message-scroller.anchors.ts`                           | 48   | 100   | 100       | 100   | 100   |
| `message-scroller.measure.ts`                           | 44   | 100   | 100       | 100   | 100   |
| `message-scroller.scroll-target.ts`                     | 82   | 100   | 100       | 100   | 100   |
| `useMessageScroller.ts` / `…Scrollable` / `…Visibility` | —    | 100   | 100       | 100   | 100   |
| `MessageScroller*.tsx`（六个部件）                      | —    | **0** | 0         | 0     | 0     |

2026-09 那一轮已经把「CSS 解析 / 锚点查找 / 对齐算法」三块纯计算拆出去了（见 DESIGN.md §2），
所以**引擎里剩下的全是"读 DOM + 驱动状态"的脏逻辑**，而恰恰是这部分几乎没有测试。

### 0.2 未覆盖区间（按关注点归类，实测 lcov）

| 关注点        | 未覆盖行                        | 说明                                                                                                      |
| ------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------- |
| DOM 几何测量  | 112-114, 123, 131, 147, 154-159 | `itemOffsetTop` / `itemTopInViewport` 的部分守卫、`contentBottom` 循环、`maxScrollTop`、`setSpacerHeight` |
| 滚动状态机    | 187-193, 219, 224-226, 228, 232 | anchored/settling 不被抢回、visibility 帧、`applyAutoscrolling` 定时器                                    |
| 命令与 spacer | 249, 289-291, 304-319           | `setScrollTop` 容差分支、`targetTopFor`、`scrollToElement` 主体（spacer + prependAnchor + mode）          |
| 重锚定        | 323-325                         | `reanchorToAnchoredMessage`                                                                               |
| 初始定位      | 333-358                         | `applyDefaultScrollPosition` 三条路径全未覆盖                                                             |
| 排队          | 367-404                         | `flushPendingScrollToMessage` / `schedulePendingFlush` / `scrollToMessage` 已挂载分支                     |
| prepend 保位  | 415-434                         | `capturePrependAnchor` / `restorePrependAnchor`                                                           |
| 内容变化编排  | 439-530                         | `handleContentChange`（93 行）几乎所有分支                                                                |
| resize        | 533-545                         | `handleResize` 四条路径                                                                                   |
| 让位          | 556-562                         | `releaseFollow`                                                                                           |
| 可见性        | 567-653                         | `computeVisibility` + 观察器 start/stop/subscribe                                                         |
| 注册          | 662-681                         | `registerMessage` / `registerItem`                                                                        |
| effect 接线   | 705, 717-718, 740, 751-752      | anchorMutation / resize 帧 / keydown                                                                      |

**结论：在把代码搬走之前，必须先补测试网。** 否则搬 400 行未覆盖逻辑，等于把风险从"看得见的大文件"变成"看不见的四个小文件"。

---

## 1. 边界划分

保留一个"编排核心"，抽出三块独立职责（外加一块共享的几何测量）：

| 新模块                                     | 关注点            | 输入                                 | 输出（副作用）                                        | **不做什么**                           |
| ------------------------------------------ | ----------------- | ------------------------------------ | ----------------------------------------------------- | -------------------------------------- |
| `message-scroller.dom-measure.ts`          | DOM 几何测量      | viewport / content / spacer 元素     | 无（只读）                                            | 不读 options、不改任何状态             |
| `message-scroller.visibility.ts`（块 D）   | 可见性与阅读锚点  | 上述测量、`options`、IO              | `visibleMessageIds` / `currentAnchorId`；按需建/断 IO | 不改 scrollTop、不决定锚定             |
| `message-scroller.scroll-state.ts`（块 A） | 模式状态机        | `options`、测量、滚动事件            | `scrollableStart/End`、`autoscrolling`、`mode`        | 不直接写 scrollTop（只提供 mode 迁移） |
| `message-scroller.commands.ts`（块 B）     | 滚动命令与 spacer | 命令参数、测量、状态机               | 写 `scrollTop`、写 spacer、迁移 mode                  | 不监听事件、不决定"何时滚"             |
| `message-scroller.anchoring.ts`（块 C）    | 锚定与保位        | content 子元素变化、resize、排队请求 | 调用块 B 的命令                                       | 不碰 IO、不拼 ctx                      |

依赖方向（无环）：

```
dom-measure ──┬──> scroll-state ──> commands ──> anchoring
              └──> visibility
                     ↑（引擎把 registerElement/unregisterElement 注入）
引擎：signals + 5 个模块 + 注册表 + 3 个 effect + 返回 ctx
```

保留在引擎里的东西（**刻意不放进去**）：
- 六个元素/状态信号与 `options` 透出；
- `messageElements: Map`（行注册表）与 `registerItem`：它同时被 anchoring（查 id）与 visibility（observe）需要，放引擎避免循环依赖；
- 三个 `createEffect`（content / viewport / 卸载清理）与 ctx 组装。

---

## 2. 需要评审拍板的边界决策

| #   | 决策                                      | 建议                                                                                                                     | 理由                                                                                                                      |
| --- | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| D1  | `mode` 的归属                             | 唯一住在 `scroll-state`，只暴露语义化迁移：`setFollowing()` / `setFree()` / `settleJump()` / `anchorTo(el)`              | 现状 `mode` 被三处直接赋值（状态机、命令、`releaseFollow`），是"看不见的耦合"；收敛后块 B/C 无法越权改模式                |
| D2  | `spacerHeight` / `spacerGap`              | 住在 `commands`，对外读 `spacerHeight()`                                                                                 | anchoring 的 `handleResize` 需要比较"重锚定前后 spacer 是否归零"                                                          |
| D3  | `pendingScroll` / `defaultApplied`        | 住在 `anchoring`                                                                                                         | 它们只服务"首次定位 + 排队"，且 `pendingScroll` 要透出给 ctx                                                              |
| D4  | 行注册表 `messageElements`                | 留在引擎                                                                                                                 | 避免 `anchoring ↔ visibility` 直接互相依赖（注册时要"若在排队该 id 则 flush"+"observe"）                                  |
| D5  | 注册表与可见性的桥                        | 引擎在 `registerItem` 里显式调用 `anchoring.notifyMessageAppeared(id)` 与 `visibility.registerElement/unregisterElement` | 让依赖方向单向、可见                                                                                                      |
| D6  | DOM 几何测量放新文件还是并入 `measure.ts` | **新文件** `message-scroller.dom-measure.ts`                                                                             | `measure.ts` 的文档定位是"CSS 尺寸测量（纯）"，而这块要读 viewport/content 元素；合在一起会让"纯模块 100% 覆盖"的保证变脆 |
| D7  | 帧调度（rAF/timeout）                     | 每个模块持有自己的句柄，引擎在卸载时逐个 `dispose()`                                                                     | 现状四个 `*Frame` + 一个 `timer` 全散在引擎作用域，卸载清理容易漏                                                         |
| D8  | 每步是否允许顺手改行为                    | **不允许**。发现行为问题就单独开提交："先写失败用例 → 修 → 保留回归用例"（TESTING.md §7）                                | 搬迁与修 bug 混在一起会让"覆盖率变化"无法解释                                                                             |

---

## 3. 迁移顺序（每步一个提交，2..6 步都不改行为）

| 步骤    | 动作                                                                                                                                         | 引擎行数（预估） | 该步必须新增的测试                          | 验收信号                                                       |
| ------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | ------------------------------------------- | -------------------------------------------------------------- |
| **0**   | 补测试网 + 抽测试脚手架                                                                                                                      | 不变（797）      | §4 全部（≈46 条），拆成 4-5 个小提交        | 引擎分支 ≥ 80%；`tests/components/message-scroller` 全绿       |
| **0.5** | 把 `measured/setScrollMetrics/setupDom/addRow/addRowAtOffset/renderEngine/flushState` 搬到 `tests/components/message-scroller/test-utils.ts` | 不变             | 无（纯搬迁）                                | 用例数与断言一字不改、全绿                                     |
| **1**   | 抽 `dom-measure`                                                                                                                             | −50              | 测量 4 组直接单测（§4 a）                   | 新模块四项 100%                                                |
| **2**   | 抽 `visibility`（块 D）                                                                                                                      | −120             | §4 e 全部（12 条）                          | 新模块四项 100%；`useMessageScrollerVisibility` 仍 100%        |
| **3**   | 抽 `scroll-state`（块 A）                                                                                                                    | −90              | §4 b 全部（9 条）+ 迁移"可滚动状态"既有用例 | 新模块四项 100%                                                |
| **4**   | 抽 `commands`（块 B）                                                                                                                        | −110             | §4 c 全部（8 条）+ 迁移"滚动方法"既有用例   | 新模块四项 100%                                                |
| **5**   | 抽 `anchoring`（块 C）                                                                                                                       | −250             | §4 d 全部（13 条）                          | 新模块四项 100%；引擎 ≤ 200 行                                 |
| **6**   | 文档收尾                                                                                                                                     | —                | 无                                          | DESIGN.md §2 表格 + "2026-10 重构"说明；TESTING.md §6 新增一行 |

### 每步统一的回归策略

1. 只搬代码、不改逻辑；搬完先跑 `bunx tsc --noEmit` 与 `biome check --write src tests`；
2. `bunx vitest run tests/components/message-scroller`（定向）→ 必须全绿；
3. 用 `--coverage.include` 单独量新模块，确认**四项 100%**——这是"搬迁没丢路径"的最强证据；
4. 记录引擎的**行数与四项覆盖率前后值**写进提交信息，便于事后核对；
5. 每步结束跑一次全量 `bun run test:coverage`（全局门槛是 100%，虽然当前仓库整体未达标，但不能让本模块退步）；
6. 任一步跑不绿 → `git checkout` 回滚，不留半成品（AGENTS.md 重构纪律）。

---

## 4. Step 0 的测试网清单

需要的桩（都比 `vitest.setup.ts` 里的 no-op 桩更可控）：

- **记录型 `IntersectionObserver`**：`vi.stubGlobal` 一个类，捕获实例、可手动投递 `entries`、可断言 `observe/unobserve/disconnect` 调用；
- **无 IO 环境**：`vi.stubGlobal("IntersectionObserver", undefined)` 覆盖 577/605 的退化分支（setup 默认提供了 no-op 桩，必须显式抹掉）；
- **rAF**：沿用现有 `flushState()`（`vi.advanceTimersByTimeAsync(0)`）；
- **测量**：沿用 `measured` / `setScrollMetrics` / `addRowAtOffset` 的可控坐标系。

### a. DOM 几何测量（4 条）

1. `items()` 排除 spacer、过滤非 `HTMLElement` 子节点；
2. `itemOffsetTop` / `itemTopInViewport` 在无 viewport 时返回 0；
3. `contentBottom` 无 viewport/content 时返回 0；含 padding；多行时取最大底边；
4. `maxScrollTop` 无 viewport 返回 0、负值归零。

### b. 滚动状态机（9 条）

5. `autoScroll: false` 初始为 `free-scrolling`（`scrollToStart` 后仍是 free）；
6. `autoScroll + !end + 非 settling/anchored` → `following-bottom`；
7. `settling-jump` / `anchored-to-message` 不被 `autoScroll` 抢回 following；
8. `following + end + 上移 + 非 autoscrolling` → `free-scrolling`；
9. `autoscrolling` 期间上移**不**转 free；
10. `following` 时对外 `end` 强制为 `false`（按钮隐藏），回实时边缘后恢复；
11. `scrollEdgeThreshold` 边界：恰好等于阈值不算可滚；
12. `applyAutoscrolling(true)` 到期自动复位；到期前再次调用会重置定时器（`AUTO_SCROLLING_TIMEOUT_MS`）；
13. `releaseFollow` 对 `following/anchored/settling` 三种模式都转 free 并关 autoscrolling；已是 free 则不动。

### c. 命令与 spacer（8 条）

14. `setScrollTop` 差值 ≤ `AT_EDGE_TOLERANCE` 时**直接赋值 + 立即提交**（不分帧）；
15. 差值更大时走 `scrollTo`，并在下一帧提交；
16. `auto: true` 时开启 `autoscrolling`；
17. `setSpacerHeight`：0 → `hidden=true` 且清 `marginTop`；正值 → `height` 且 `marginTop = -gap`；同值重复调用不重写 DOM；
18. `scrollToElement`：`content` 不包含目标 → 返回 false；
19. `scrollToElement` 的 spacer 高度 = `max(0, target + clientHeight - contentBottom)`；
20. `scrollToElement` 记录 `prependAnchor`；`keepPreviousPeek` 时 mode=anchored 且记 `streamingTurn`，否则 settling；
21. `scrollToStart` / `scrollToEnd`：清零 spacer 与 `streamingTurn`、设置 mode、无 viewport 时返回 false；`reanchorToAnchoredMessage` 仅在 anchored 且元素仍 connected 时生效。

### d. 锚定与保位（13 条）

22. 空会话时 `pendingScroll` 置 false（不隐藏视口）；
23. 初次定位三条路径：`start` → `scrollToStart`；`end` → `scrollToEnd`；`last-anchor` 有锚点且该回合放得下 → `scrollToEnd`；
24. `last-anchor` 放不下 → `scrollToElement(align:start, keepPreviousPeek)`；无锚点 → 回退 `end`；
25. 初次定位会把已存在的锚点标记为"已处理"，避免后续属性变化误触发重锚定；
26. `previousCount === 0` 且未应用默认位置：`autoScroll` 时 `scrollToEnd`，否则只提交状态；
27. prepend 保位：`previousFirst` 下标 > 0 且 `preserveScrollOnPrepend` → 按视口差值补偿并重记锚点；
28. 新回合单锚点：`scrollToElement(align:start, keepPreviousPeek)` 并加入 handled；
29. 同批多锚点 + `following` + `autoScroll` → 直接 `scrollToEnd`（不在多个锚点间跳）；
30. 行数未变但出现未处理锚点（给已有行打开 `scrollAnchor`）→ 锚定到它；
31. `following + autoScroll` 的普通追加 → `scrollToEnd`；否则提交状态 + 同步可见性；
32. `handleResize` 四条：following → end；anchored 重锚定成功 → 若 spacer 从 >0 归零且 autoScroll 则再 end；否则分帧提交；
33. `scrollToMessage` 四种：已挂载且成功 → true；已挂载但滚动失败 → 排队 true；未挂载且 `itemCount===0` → 排队 true 且 `pendingScroll=false`；已挂载但 id 缺失 → false；
34. 排队 flush：目标元素注册后下一帧 flush 成功并重置 `pendingScroll`。

### e. 可见性（12 条）

35. 无 `IntersectionObserver`：退化为按矩形逐项判定；
36. 有 IO：用 `visibleIds` 集合判定，锚点仍按矩形算；
37. `currentAnchorId` = 阅读线（`scrollMargin + scrollPreviousItemPeek`）之上最后一个锚点，含容差边界；
38. 无 viewport/content → 两个信号都清空；
39. 全部不可见且无锚点 → 两个信号都清空；
40. IO 回调：`isIntersecting` 增删 id 后分帧同步；
41. `subscribeVisibility` 计数：首个订阅建 observer 并 observe 已注册行；最后一个撤销时断开并清空；
42. 无 IO 时订阅走"逐帧同步"路径；
43. `registerItem` 挂载：写入注册表、`observe`、分帧同步；若正在排队该 id → 触发 pending flush；
44. `registerItem` 卸载：`unobserve`、从 `visibleIds` 删除、分帧同步；
45. 卸载清理：取消所有帧、断开 observer、清空 `visibleIds`；
46. 重新订阅时 observer 会重建（`stop` 后再 `subscribe`）。

### f. 接线（复用既有 8 条）

现有 `基础接线` 用例已覆盖 signals/透出/初始 pendingScroll；Step 0 只需补：`setSpacer` 触发 `spacerGap` 计算、`preserveScrollOnPrepend` 的 setter、以及 ctx 键集合与 `MessageScrollerContextValue` 一致（用显式键名列表断言，不用快照）。

---

## 5. 风险与对策

| 风险                                    | 对策                                                                                                                                             |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 测试网本身写错（把当前 bug 固化成期望） | 每条断言前先确认 DESIGN.md §3 的"已实现行为"是权威；发现实现与文档不符 → 当 bug 处理（单独提交 + 文档同步），不写"迎合实现"的断言                |
| `mode` 语义在搬迁中被改掉               | Step 3 之前先补 §4 b 的模式迁移矩阵；Step 3 只做"把赋值换成语义化方法"                                                                           |
| 循环依赖导致 import 混乱                | 按 §1 的依赖方向；循环苗头（anchoring ↔ visibility）一律通过引擎注入回调处理                                                                     |
| jsdom 无布局导致"测了个寂寞"            | 继续用可控坐标系（`measured` + `setScrollMetrics`）；不引入真实浏览器依赖                                                                        |
| 四块新模块"都 100% 但组合起来坏了"      | 引擎层的组合用例（§4 d/f）保留在集成测试文件里，不迁走；每步跑定向 + 全量                                                                        |
| 拆完行数没降（接口变多）                | 以"模块可独立 100% 覆盖"为收益判据，不以行数为唯一指标；若某块拆完 < 60 行且无独立语义，就不拆（评估后 `dom-measure` 是边界情况，已按 §D6 决策） |

---

## 6. 收尾（Step 6）

- DESIGN.md §2 的 SRP 表格补上四个新文件与各自关注点；
- §2 末尾的"2026-09 重构"说明后追加一段"2026-10 重构"：拆出 `dom-measure` / `visibility` / `scroll-state` / `commands` / `anchoring`，引擎回到纯编排，并记录各模块覆盖率；
- TESTING.md §6 增加 message-scroller 行：已覆盖的块（纯模块 + 引擎接线 + 本次补的锚定/可见性用例）与仍为 0% 的部分（六个 `MessageScroller*.tsx` 部件与整机行为），后者是下一轮的独立任务；
- 若 §8 需要登记不可达守卫（`!vp` / `typeof IntersectionObserver === "undefined"` 等防御分支在搬迁后可能无法全部命中），逐条以变异方式确认后登记。

---

## 7. 不建议做的事

- **不要**为了减少文件数把四块合并回引擎，或把本轮新块并入已经 100% 的 `measure.ts` / `anchors.ts`；
- **不要**在搬迁同一提交里顺手修 bug、改命名、调阈值；
- **不要**引入通用的 "scroll manager" 抽象层或第三方滚动库；
- **不要**为了覆盖率写"复制实现公式"的断言（TESTING.md §4.2）：spacer 高度、阅读线这类几何断言要用**具体的输入/输出数字**表达；
- **不要**在部件（`.tsx`）为 0% 的情况下宣称 message-scroller 已有回归网——本轮只解决引擎。
