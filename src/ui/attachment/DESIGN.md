# Attachment 组件设计方案

> 状态:**已实现**
> 移植目标:**shadcn Base UI 版 `attachment`**(`ui.shadcn.com/docs/components/base/attachment`)
> 关联代码:`src/components/attachment/`

## 1. 概述

用于展示文件/图片附件:媒体(图标或图片)、名称、元信息、上传状态与操作。
常见于聊天输入框、消息流、上传列表。

结构与 shadcn 一致(root + parts,每个部件一个目录):

```
Attachment
├── AttachmentMedia        # 图标 / 图片预览
├── AttachmentContent
│   ├── AttachmentTitle
│   └── AttachmentDescription
├── AttachmentActions
│   └── AttachmentAction   # 复用 Button
└── AttachmentTrigger      # 覆盖整卡的触发器(多态)
AttachmentGroup            # 横向滚动、吸附的附件行
```

## 2. 状态 / 尺寸 / 方向

- `state`:`idle` | `uploading` | `processing` | `error` | `done`(默认 `done`)。
  `uploading`/`processing` 对标题应用 `shimmer`;`error` 切换 destructive 处理。
- `size`:`default` | `sm` | `xs`。
- `orientation`:`horizontal`(默认) | `vertical`。
- 全部通过根上的 `data-state` / `data-size` / `data-orientation` + 具名 `group/attachment`
  驱动子部件样式(与 shadcn 一致)。

## 3. 复用

| 需要               | 复用                                                                            |
| ------------------ | ------------------------------------------------------------------------------- |
| 操作按钮           | `Button`(默认 `ghost` + `xs`,仅图标时要求 `aria-label`)                         |
| 多态触发器         | `~/types` 的 `PolymorphicProps` + `Dynamic`(默认 `button`,可换 `a` 等)          |
| 圆角/卡片色/状态色 | 现有主题 token(`bg-card`、`text-card-foreground`、`bg-muted`、`destructive` 等) |

## 4. 两个自实现工具类(`src/styles/effects.css`)

shadcn 的 `shimmer` / `scroll-fade-x` 随其 `shadcn` 包提供,本仓库没有,因此自实现:

- **`shimmer`**:`background-clip: text` + 横向移动的高光带(`currentColor` 为底色,
  `color-mix` 出高光),`@keyframes tw-shimmer`。上传/处理中标题使用。
- **`scroll-fade-x`**:横向滚动容器两端 `mask-image` 渐隐。支持
  `animation-timeline: scroll()` 的浏览器里随滚动进度显现/收起；不支持时退化为静态双边缘渐隐
  （`src/styles/effects.css` 的 `@supports` 分支）。

## 5. 无障碍

- `AttachmentAction` 通常是纯图标,需 `aria-label` 说明动作与目标。
- `AttachmentTrigger` 覆盖整卡、自身无文本,需要 `aria-label`;它位于操作区(z-20)之下的
  z-10,两者都能独立聚焦/点击。
- `AttachmentGroup` 横向滚动:附件可交互时键盘用 Tab 到达屏外项;纯展示时由使用方给
  group 加 `tabIndex`/`role`/`aria-label`。
- `error` 状态除颜色外,在 `AttachmentDescription` 里保留失败原因(不只靠颜色表达)。

## 6. 后续待实现

- `shimmer` 的完整参数(`shimmer-color` / `-duration` / `-spread` / `-angle` / `-reverse` /
  `-once` / `-none`)与 RTL 细化（`prefers-reduced-motion` 已有兜底）。
- 纯展示场景的可聚焦 group 辅助(可选封装)。
- 真机交互/a11y 走查（`tests/components/attachment/` 已覆盖全部部件与组合，dev 示例也已存在）。

## 7. 使用示例

```tsx
<Attachment state="uploading">
  <AttachmentMedia>
    <FileText />
  </AttachmentMedia>
  <AttachmentContent>
    <AttachmentTitle>sales-dashboard.pdf</AttachmentTitle>
    <AttachmentDescription>PDF · 2.4 MB</AttachmentDescription>
  </AttachmentContent>
  <AttachmentActions>
    <AttachmentAction aria-label="Remove sales-dashboard.pdf">
      <X />
    </AttachmentAction>
  </AttachmentActions>
</Attachment>;
```
