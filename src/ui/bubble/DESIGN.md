# Bubble 组件设计方案

> 状态:**已实现**
> 移植目标:shadcn Base UI 版 `bubble`(`ui.shadcn.com/docs/components/base/bubble`)
> 关联代码:`src/components/bubble/`

## 1. 概述

对话气泡的展示面。无状态、无浮层,纯样式 + 多态内容。四个部件:

```
BubbleGroup            # 同一发送者的连续气泡分组
└── Bubble             # 根:variant + align
    ├── BubbleContent  # 内容,多态(div / button / a)
    └── BubbleReactions# 贴在气泡边缘的反应行
```

## 2. SRP 分工

| 关注点        | 文件                                                                         |
| ------------- | ---------------------------------------------------------------------------- |
| 变体/对齐样式 | `Bubble/Bubble.styles.ts`(cva,7 个 variant)                                  |
| 根容器        | `Bubble/Bubble.tsx`(输出 `data-variant` / `data-align`)                      |
| 内容样式      | `BubbleContent/BubbleContent.styles.ts`                                      |
| 多态内容      | `BubbleContent/BubbleContent.tsx`(`PolymorphicProps` + `Dynamic`,默认 `div`) |
| 反应样式      | `BubbleReactions/BubbleReactions.styles.ts`(cva,side/align)                  |
| 反应容器      | `BubbleReactions/BubbleReactions.tsx`                                        |
| 分组          | `BubbleGroup/BubbleGroup.tsx`                                                |

## 3. 关键点

- **BubbleContent 必须是 Bubble 的【直接子级】**:变体样式走 `*:data-[slot=bubble-content]:*`
  与 `[&>[data-slot=bubble-content]:is(button,a):hover]:*`,依赖直接子选择器。
- **多态**:`BubbleContent` 用 `component` 换成 `button`/`a`,自动获得 `[button,a]` 的
  transition / focus ring 与 hover 底色;交互元素的无障碍名来自气泡文本。
- **对齐**:`align="start" | "end"`,通过 `data-[align=end]:self-end` 作用在 flex 列容器里。
- **变体**:`default / secondary / muted / tinted / outline / ghost / destructive`;
  `ghost` 取消 `max-w-[80%]` 以便整行宽度。

## 4. 无障碍

- 展示型反应:给 `BubbleReactions` 加 `role="img"` + 描述性 `aria-label`(整行只播报一次)。
- 交互型反应:渲染 `Button` 并给纯图标按钮 `aria-label`。
- 可点击气泡:用 `BubbleContent component={...}` 渲染真正的 `button`/`a`,不要只加 onClick。

## 5. 后续

- 与 `Message` 组件组合(头像/名字/时间戳/消息级操作)不在本组件范围。
- Collapsible / Tooltip / Popover 组合已在 dev 示例中演示(见 dev/examples/bubble.tsx)。
