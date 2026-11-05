# Message 组件设计方案

> 状态:**已实现**
> 移植目标:shadcn Base UI 版 `message`(`ui.shadcn.com/docs/components/base/message`)
> 关联代码:`src/components/message/`

## 1. 概述

一条消息的行布局：负责头像、对齐、header、footer，**不负责消息外观**——
可见的消息面用 `Bubble` 渲染，滚动容器用 `MessageScroller`（已实现）。

```
MessageGroup            # 同一发送者的连续消息
└── Message             # 行:align(start/end) + 头像 + 内容
    ├── MessageAvatar   # 头像槽(贴底,有 footer 时上移)
    └── MessageContent
        ├── MessageHeader
        ├── Bubble      # 使用方自行放置
        └── MessageFooter
```

## 2. SRP 分工

每个部件一个目录（`src/components/message/MessageXxx/`：`MessageXxx.tsx` + `.types.ts` + `index.ts`），
共享类型在 `Message/Message.types.ts`。本组件无状态、无浮层，全部是纯展示的 div + `data-*` 状态钩子。

## 3. 关键点

- `align` 通过 `data-[align=end]:flex-row-reverse` 反转整行(头像随之到另一侧);
  `MessageContent` 的 `group-data-[align=end]/message:*:data-slot:self-end` 让其中的气泡等
  贴到末端。
- **头像贴底**:`self-end`;当消息含 `MessageFooter` 时,用
  `group-has-data-[slot=message-footer]/message:-translate-y-8` 上移,避免对着 footer。
- **footer 随消息一侧**:`group-data-[align=end]/message:justify-end`。
- **ghost 气泡**:header/footer 通过 `group-has-data-[variant=ghost]/message:px-0` 去掉左右内边距
  ——依赖 `Bubble` 的 `data-variant="ghost"`。

## 4. 无障碍

`Message` 是展示型布局,语义由内部内容决定:纯图标操作按钮要有 `aria-label`;
进行中的消息建议用 `Marker role="status"` + `Spinner`。

## 5. 后续

- 已完成：`MessageScroller`（会话滚动容器）位于 `src/components/message-scroller/`，
  其设计见 [MessageScroller DESIGN](../message-scroller/DESIGN.md)。
