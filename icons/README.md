# Fluxy 方案 A · 跨平台图标资源

统一品牌标识：沿用「极简流环」原版 A 的无限曲线，不增加端点、小箭头或装饰性元素。

## 视觉分发策略

- **macOS**：圆角底板版，`macos/fluxy-macos.svg`，提供 PNG 16—1024 与 `fluxy.icns`。这是 Apple 风格的 app tile 处理。
- **Windows 10/11**：无底板自由轮廓版，`windows/fluxy-windows.svg`，以深色薄边缘增强在浅色壁纸的可读性；`windows/fluxy.ico` 包含 16/24/32/48/64/128/256 多尺寸图像。另有深色背景用 `fluxy-windows-dark.svg`。
- **Linux**：简洁自由轮廓版，`linux/fluxy-linux.svg`；提供 `linux/hicolor` 图标主题树与 `fluxy-symbolic.svg` 纯色标识。Linux 发行版/桌面环境的图标风格各不相同，并不存在必须使用某种边框的规则。

## 集成说明

- Tauri 打包时为各目标系统分别指定合适的 icon 文件；Windows 可直接使用 `fluxy.ico`，macOS 可使用 `fluxy.icns`，Linux 桌面可使用 hicolor 目录里的 `fluxy.png` / `fluxy.svg`。
- Linux `.desktop` 文件中请使用 `Icon=fluxy`，并在目标环境安装 `hicolor/{size}x{size}/apps/fluxy.png` 或 `hicolor/scalable/apps/fluxy.svg`。
- 对于不同桌面主题，高对比度等特殊场景，提供 dark 和 symbolic 变体，不要自动将 macOS 深色方形底板强加到 Linux/Windows。
- 这些资源为通用跨平台品牌视觉提案，不代表所有 Windows/Linux 应用必须采用透明图标；仍可根据应用商店和软件包格式要求生成特殊尺寸与版本。

SVG 均为独立可编辑矢量路径，无外部素材与字体依赖。`icons_preview.png` 是视觉对比稿，主资产请从各平台目录中选用。
