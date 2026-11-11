# ScrollArrows 设计说明

> 状态:P1(悬停滚动默认开启,但由**滚动容器**按指针位置驱动)
> 关联代码:`src/components/scroll-arrows/`、`src/components/scroll-arrows/useHoverScroll.ts`、`src/hooks/useScrollEdges.ts`

## 1. 背景

Select / Combobox / ContextMenu / TimePicker 把原生滚动条隐藏后用上下箭头提示
"该方向还有内容"。箭头是绝对定位浮层,贴在滚动容器的上/下沿。

## 2. 生产事故:箭头可交互导致列表持续滚动

早期实现(把箭头做成可交互浮层)在 TimePicker 里出现"无限滚动、分钟选不中":

1. 箭头是**满宽、可命中**的浮层(`absolute inset-x-0 h-6 z-10`,pointer events auto),
   与列表首/尾选项在边缘处重叠,于是**抢走了列表项的指针事件**——想点边缘的分钟,
   实际命中的是箭头。
2. `pointerenter` 触发 rAF **持续滚动**。指针停在箭头上不动时,列表会一直滚,
   指针下方的选项每帧都在变 → "数字不停变动、点不中"。
3. 滚到边界时 `useScrollEdges` 把箭头置为不可见,`<Show>` 卸载箭头;
   浏览器在 DOM 变化后会重算 hover 并把 `pointerenter` 派发给新的目标;
   加上原来的 1px 阈值没有迟滞,`canScroll*` 会在边界反复翻转,箭头反复出现/消失,
   于是"显示 → pointerenter → 滚动 → 消失 → 再显示"形成**反馈循环**。
   同时旧的 rAF 循环没有检查箭头是否可见,可能在上一个循环里继续滚动。

## 3. 现在的做法

上一版把悬停滚动关掉了(`interactive` 默认 `false`),于是库内**没有任何调用方**
开启它——能力变成死代码,使用者看到的就是"箭头悬浮时不再滚动"。但直接恢复旧实现
会把"抢点击"一起带回来。真正的解法是**把悬停检测从箭头搬到滚动容器上**:

- **箭头永远是装饰层**:`pointer-events-none`(恒定,不再有开关)、`aria-hidden`,
  只画视觉提示。它不再是滚动逻辑的一部分,于是**结构上**不存在
  "箭头显隐 ↔ pointerenter 互相触发"的反馈循环。
- **悬停滚动在容器上驱动**(`useHoverScroll.ts`):监听容器的 `pointermove`,
  按指针坐标判断是否停在**上/下 24px 的边缘带**内(`h-6`,与箭头等高)。
  因为箭头不参与命中,边缘的列表项照常收到点击——这正是旧实现做不到的。
- **停留 150ms 才滚动**:带内移动会重置计时,掠过不会误触发。
- **随时可被夺回控制权**:`pointerdown`(按下立刻停,避免按下期间选项还在动)、
  `touchstart`、`wheel`、`keydown`、`pointerleave`、到边界(`scrollTop` 不再变化)
  都会停止。已排队但"取消不及"的帧到达时会因方向已清空而直接返回。
- **与箭头显隐同源**:带内是否可滚由 `useScrollEdges` 的 `canScrollUp/Down` 判定,
  所以"箭头没显示"时那个方向也不会滚。
- **迟滞阈值**:`useScrollEdges` 进入阈值 4px、退出阈值 1px,消除边界处每帧抖动。

代价(需要知道的取舍):悬停滚动期间,列表内容会在静止的指针下方移动,因此
**基于 hover 高亮的组件**(Select / Combobox 用 `mouseenter` 高亮)会看到高亮
跟着指针下方的项走。这是"不抢指针"的必然结果——若某个调用方不接受,传
`hoverScroll={false}` 即可。ContextMenu 不受影响:它的高亮挂的是 `pointermove`,
内容移动不会触发。

## 4. 使用

```tsx
<ScrollArrows target={listElement} />                  // 悬停滚动(默认)
<ScrollArrows target={listElement} hoverScroll={false} /> // 纯装饰
```

> 迁移提示:旧版是 `interactive`(默认 `false`),已改名为 `hoverScroll`(默认 `true`);
> 语义也变了——它不再让箭头参与指针命中,只控制容器上的悬停滚动。
