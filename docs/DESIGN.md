---
title: "Fluxy 项目设计文档"
subtitle: "局域网跨平台即时通信 · 文件传输 · 共享剪贴板"
author: "项目设计草案"
date: "2026-10-21"
lang: zh-CN
---

# Fluxy 项目设计文档

> **版本**：v0.2.0（架构草案）  
> **更新日期**：2026-10-21  
> **状态**：技术方案讨论稿，供 MVP 开发与设计评审使用  
> **目标平台**：Windows / macOS / Linux / Android / iOS

## 1. 项目概述

### 1.1 产品定位

**Fluxy** 是一款面向同一局域网设备的、以本地优先（local-first）为原则的跨平台实时数据交换应用。它将 **即时文字聊天、文件/文件夹传输、共享剪贴板** 视为三个平级的核心业务。应用默认不要求注册账号、不依赖公共服务器，在网络允许的情况下由设备直接建立加密通信连接。

Fluxy 既不是“附带聊天的文件工具”，也不是“只会发文件的聊天软件”。其核心体验是：**打开应用 → 发现并配对设备 → 发送文字、复制内容或传输文件 → 在目标设备上立即使用。**

### 1.2 设计目标

1. **跨平台一致**：一套 Rust 业务引擎覆盖桌面和移动端，界面与原生权限按平台适配。
2. **局域网直连**：自动发现、手动地址和二维码配对等方式兼容不同网络条件；不承诺穿透被隔离的网络。
3. **即时通信优先**：大文件传输时，聊天消息、剪贴板和控制消息仍要保持可用与低延迟。
4. **可靠传输**：支持分块校验、暂停、取消、并发调度、网络中断后续传；跨重启续传通过最小检查点实现。
5. **隐私最小化**：基础版不持久化聊天内容；只有配置、设备信任关系与必要传输恢复元数据需要落盘。
6. **渐进扩展**：为未来企业定制的消息留存、审计、策略和设备管理预留边界，但不提前引入沉重服务端或数据库。

### 1.3 非目标

首个正式版本**不包含**互联网穿透与云中继、中心化账户体系、企业组织架构、群聊、全平台后台自动剪贴板同步、跨设备离线消息存储、聊天内容检索、端到端聊天记录云备份。它们不应阻塞基础版交付。

## 2. 功能范围与版本规划

| 功能 | MVP（桌面优先） | 移动适配阶段 | 后续 / 企业版 |
|---|---|---|---|
| 局域网发现与手动直连 | 是 | 平台原生发现与权限 | 跨网发现或组织策略 |
| 首次配对、可信设备 | 是 | 是 | 批量部署与集中管理 |
| 一对一实时文本消息 | 是，内存会话 | 是，前台可用优先 | 留存、检索、审计 |
| 接收文字自动复制 | 用户按设备授权后启用 | 前台及系统允许时 | 组织策略配置 |
| 手动发送当前剪贴板 | 是 | 是（按系统能力） | 多种内容格式 |
| 自动监听剪贴板 | 桌面端可选 | 能力受限，不承诺后台 | 更多策略与同步模式 |
| 单/多文件发送与接收 | 是 | 是 | 管理型批量传输 |
| 分块校验与网络断线续传 | 是 | 是 | 续传调度策略优化 |
| 并发文件/分块传输 | 基础并发 | 按设备能力限制 | 自适应调度优化 |
| 退出重启后恢复传输 | 设计与最小实现 | 需恢复文件授权 | 更多跨设备恢复策略 |
| 文件夹传输 | 桌面优先（安全路径处理） | 依平台文件授权能力 | 文件差异同步 |
| 聊天记录持久化 | **否** | **否** | 可选企业扩展 |

**说明**：“MVP（桌面优先）”指优先使桌面 Rust Core 和 GUI 具备可用能力；Android/iOS 是明确的产品目标，而不是所有移动平台限制都能被统一封装消除。

## 3. 关键用户场景

### 3.1 文字快速传送

电脑 A 选择已配对手机 B，发送一段文字。手机 B 前台接收到消息后显示通知，并依据 B 本地已启用的“自动复制来自 A 的文字”策略，写入剪贴板。若系统不允许自动写入，则显示一键复制入口。**发送方不能单方面强制覆盖接收方的剪贴板。**

### 3.2 共享剪贴板

用户在设备 A 选择“发送当前剪贴板”，把纯文本传到 B；桌面端可另行开启自动检测与同步。在自动同步模式下，通过消息 ID、来源设备 ID、内容摘要与短期去重缓存抑制回声；对敏感应用或内容的过滤仅作辅助，不能代替用户明确授权。

### 3.3 大文件可靠传送

用户从桌面向移动端发送大文件。系统分块发送并校验，允许同时运行多个有限数量的数据流；连接中断后，接收方保留已完成块和恢复元数据。恢复前再次验证设备身份、源文件一致性和文件访问权限，仅重新请求缺失块。

### 3.4 边传文件边聊天

在数十 GB 文件传输期间，用户仍能及时发送聊天消息与剪贴板文本。控制消息使用高优先级流，文件任务受队列长度与带宽/缓冲预算约束。

## 4. 技术选型

| 层次 | 推荐技术 | 选择理由及注意事项 |
|---|---|---|
| 应用容器 | Tauri 2 | 跨桌面/移动端、原生插件与 Rust 集成 |
| 前端框架 | SolidJS + TypeScript | 细粒度响应式，适合列表和实时任务状态 |
| UI 组件 | **Neut** | 自有组件库，服务于本应用通用 UI 需求 |
| 前端构建 | Vite | 轻量、与 SolidJS 集成直接 |
| Rust 异步运行时 | Tokio | 网络、调度、取消与任务管理 |
| 可靠传输 | Quinn（QUIC/TLS） | 加密、多路复用与流量/拥塞控制 |
| 设备发现 | mDNS/DNS-SD + 手动地址 | 适合局域网；平台实现需适配 |
| 序列化 | Serde + 结构化二进制协议（推荐 Protobuf） | 易于版本演进；数据流不经过 JS |
| 校验 | BLAKE3 | 块级与全文件内容校验 |
| 配置 | 小型本地配置文件 | 不引入聊天数据库 |
| 传输恢复 | 原子更新的 checkpoint/journal + .part 文件 | 仅保存断点续传必要状态 |
| 可信设备/密钥 | OS 安全存储或严格权限的本地密钥文件 | 持续设备身份与可信关系 |

**技术原则**：Rust Core 不依赖 Tauri；Neut 仅承载通用组件，不承载 Fluxy 特定业务；消息内容和文件二进制数据不通过 WebView 中转。Tauri Commands 承载用户操作，Channels / 有界事件通道承载状态与进度更新。

## 5. 总体架构

```text
┌─────────────────────────────────────────────────┐
│ Fluxy UI: Tauri 2 + SolidJS + Neut              │
│ 设备 / 会话 / 聊天 / 剪贴板 / 传输任务 / 设置    │
└───────────────────┬─────────────────────────────┘
                    │ Commands / Channels
┌───────────────────▼─────────────────────────────┐
│ Tauri Adapter + Platform Plugins                │
│ 通知、文件选择、系统剪贴板、移动端生命周期      │
└───────────────────┬─────────────────────────────┘
                    │ 抽象接口
┌───────────────────▼─────────────────────────────┐
│ Rust Core                                       │
│ DeviceManager / SessionManager / EventBus       │
│ Messaging / Clipboard / TransferManager         │
│ FileAccess / TransferCheckpoint                 │
└───────────────────┬─────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────┐
│ Tokio + Quinn + mDNS                            │
│ 身份认证 / QUIC 连接 / 控制流 / 文件数据流      │
└─────────────────────────────────────────────────┘
```

### 5.1 模块职责

- `protocol`：版本协商、消息封装、消息尺寸限制、错误码与能力声明。
- `identity`：设备身份密钥、首次配对、可信设备与证书验证。
- `discovery`：mDNS/DNS-SD、手动直连、地址变化处理、在线状态。
- `transport`：Quinn 连接、握手、断线重连、流优先级与接收限流。
- `messaging`：文字消息、接收确认、内存中的短期会话、去重。
- `clipboard`：主动推送、自动同步策略、接收端授权、回声抑制。
- `transfer`：Manifest、ChunkTracker、Scheduler、校验、恢复与取消。
- `file-access`：路径/URI/授权抽象，暴露流式读写和随机访问能力。
- `checkpoint`：传输状态落盘与异常恢复，**不处理聊天持久化**。
- `app-adapter`：Tauri Commands/Channels、配置与界面状态投影。

### 5.2 数据与生命周期边界

**权威业务状态保存在 Rust**；SolidJS 仅保留渲染快照和交互态。GUI 页面关闭或切换不应让文件传输引擎自动退出。所有长生命周期任务由 Rust App State 管理，并支持显式取消。

基础版的文字消息采用**有界内存缓存**；应用退出即丢失。传输恢复元数据只包含文件描述信息、块位图、任务和设备关联信息，不存储聊天内容。操作系统通知与诊断日志也必须避免意外包含私密消息正文。

## 6. 设备发现、配对与连接

### 6.1 发现策略

1. 首选 mDNS/DNS-SD 广播服务实例，包含产品协议标识、版本和最少必要的连接信息；**广播内容不作为身份信任依据**。
2. 为不同子网、组播禁用、AP 隔离环境提供手动 IP/端口及二维码配对。
3. 对发现结果设置 TTL、去重与在线/离线超时；IP 地址改变时使用稳定设备 ID 识别设备。
4. 若网络阻止两端通信，明确提示“不在可直连网络中”，而不是无休止重试。

### 6.2 身份与首次配对

每台安装实例生成稳定的设备密钥对和 `DeviceId`。首次连接时双方通过用户确认的配对码、二维码或指纹确认彼此身份；后续在 QUIC/TLS 握手中绑定并验证已信任的设备公钥或证书。**不得通过跳过 TLS 证书校验来实现“局域网免证书”。** 支持取消信任、重新配对和密钥轮换。

### 6.3 会话策略

- 每对在线设备优先维护**一条可复用的 QUIC 连接**，在连接上开多个流。
- 控制/聊天/剪贴板逻辑通道与文件数据流分离；大文件不可阻塞控制消息。
- 连接重试采用退避与抖动，并在重新连接后重新校验身份。
- 应用层确认 `MessageAck` 的意义是接收端业务层已处理消息，不能等同于 QUIC 的网络 ACK。

## 7. 通信协议设计

### 7.1 消息封装（建议）

```text
Envelope {
  protocol_version: u32
  message_id: UUID
  source_device_id: DeviceId
  session_id: UUID
  created_at_ms: i64
  kind: MessageKind
  payload: bytes
}
```

`created_at_ms` 只用于显示和诊断，不作为可信的时序/去重凭据。接收方必须对消息总大小、字段长度、字符串编码和关联权限做校验。客户端连接后先协商协议版本、支持的能力、分块大小上限、并发上限等。

### 7.2 控制消息类型

| 类别 | 消息 | 语义 |
|---|---|---|
| 基础 | `Hello`, `Capabilities`, `Ping`, `Error` | 协商、心跳与错误 |
| 聊天 | `ChatText`, `MessageAck` | 文本、应用层确认 |
| 剪贴板 | `ClipboardText`, `ClipboardAck` | 推送文本及处理结果 |
| 文件 | `TransferOffer`, `TransferAccept`, `TransferReject` | 任务协商 |
| 文件 | `ChunkRequest`, `ChunkAck`, `TransferResume` | 缺失块请求与验证确认 |
| 文件 | `TransferPause`, `TransferCancel`, `TransferComplete` | 状态和生命周期 |

文件块的原始字节在数据流中发送，由小型帧头携带 `transfer_id / file_id / chunk_index / offset / length / hash`；不放入 JSON，也不经由 WebView。接收方对未知消息类型按版本兼容策略拒绝或忽略，不得误解释。

### 7.3 幂等、排序与可靠性

- 同一会话的文字消息默认按消息序列号呈现；重连后拒绝重复 `message_id`。
- 采用短期有界 ID 缓存进行去重。基础版没有离线投递，接收端未在线的消息不得显示为“已送达”。
- 文件数据块可乱序完成；只有接收端写入并校验成功后才 `ChunkAck`。
- 异常断线时所有尚未确认的块视为待核对，不能仅凭发送端已写入套接字判断成功。

## 8. 文件传输系统

### 8.1 任务模型与状态机

一次传输任务 `TransferId` 包含发起者、接收者、文件清单、目标目录策略、任务状态、统计信息和 checkpoint 引用。

```text
Created → Offered → AwaitingApproval → Preparing
                                   └→ Rejected
Preparing → Transferring ↔ Paused
                  │
                  ├→ Interrupted → Resuming → Transferring
                  ├→ Verifying → Completed
                  ├→ Failed
                  └→ Cancelled
```

状态转移由 Rust Core 统一执行，GUI 不能直接把任务“改成完成”。完成必须满足：所有块校验通过、全文件校验通过、文件安全落地/最终重命名完成。

### 8.2 分块、校验与并发

- **建议初始值**：分块 4 MiB；单文件并发 4 个块；同时传输文件数 2（均为可配置的测试起点，不是永久固定值）。
- Chunk 大小必须写入 Manifest，支持协商；每个分块带 BLAKE3 校验，最终再核对全文件摘要。
- 采用有界 Semaphore 和有限数据缓冲，异步偏移量读写；**不要把整个分块队列或大文件整体加载到内存**。
- `TransferScheduler` 同时管理“并发文件数”和“单文件数据流数”，按设备和全局资源预算限流。
- 对控制流使用较高优先级，并在应用层限制文件数据流写入窗口；优先级是调度提示，不是严格延迟保证。
- 文件夹传输在 Manifest 中使用**相对路径**，拒绝路径穿越、绝对路径、非法名称及未授权符号链接。

### 8.3 断点续传与异常恢复

接收端保存 `.part` 临时文件、Manifest 和完成块位图；重新连接时通过 `TransferResume` 协商缺失块，并对可疑或未持久确认的块重新验证。发送端需确认源文件仍是同一内容版本；可以结合大小、修改时间、块摘要和最终全文件 BLAKE3 校验。**仅依赖文件名/修改时间不够安全。**

建议采用原子替换的 checkpoint 文件或附带校验的 journal，先确保块数据满足约定的持久性，再标记 checkpoint 成功；异常退出后允许重新校验边界块。恢复失败可降级为重新发送，而不能拼接不同内容版本的文件。

```text
app-data/
  checkpoints/<transfer-id>/manifest.bin
  checkpoints/<transfer-id>/progress.bin
  received/<transfer-id>.part
```

文件完成后进行完整性验证，再移至最终文件名；遇到同名文件、磁盘空间不足、无权限时必须提示并进入可恢复状态。任务元数据是基础功能必要技术数据，**不属于聊天记录商业化存储能力**。

### 8.4 非功能目标（待基准测试确认）

- 单文件支持大于 10 GiB 的端到端测试，不出现整文件内存加载。
- 中断网络后重新连接，只重新传输未确认或校验失败的块。
- 并发文件传输中，聊天消息在基准千兆局域网中保持可交互；具体 p95 延迟由测试确定。
- 文件传输完成后，源文件与目标文件的最终摘要一致。
- 高并发情况下，队列、内存与打开文件描述符数量均有上限。

## 9. 共享剪贴板与消息体验

### 9.1 两种工作模式

**模式 A：主动共享（优先实现）。** 发送当前剪贴板文本到指定设备；或聊天时附带“接收后复制”的意图。**最终是否写入剪贴板由接收端已配置的信任策略决定**。

**模式 B：自动同步（按平台逐步实现）。** 桌面端检测用户复制的文本并发往选定可信设备；移动端根据系统权限和生命周期降级为前台同步、手动发送或“点击复制”。不承诺 iOS / Android 上持续后台读取剪贴板。

### 9.2 保护策略

1. 自动监听默认关闭；自动写入按设备明确选择，提供“一键关闭”。
2. 同步范围默认只含纯文本，限制单条大小（例如初始 1 MiB，可配置），文件/图片剪贴板另行设计。
3. 通过来源 ID、消息 ID、有限时间窗口的内容摘要抑制循环；本地程序写入应当被标记以避免再广播。
4. 接收到含密码、一次性验证码等文本时无法保证可靠识别敏感性，必须让用户控制自动同步。
5. 手机端无法自动写入时，保留消息并显示明确的“复制”操作，不静默声称成功。

### 9.3 前端交互建议

桌面主界面采用“设备列表 / 当前会话 / 传输队列”的布局，提供拖放文件、设备右键菜单“发送当前剪贴板”、聊天气泡复制、传输进度/速度与暂停恢复。移动端聚焦设备选择、文字发送和文件选择，避免强行复刻桌面三栏布局。

## 10. 跨平台文件访问

### 10.1 核心原则

**不要让 `transfer` crate 接收平台原始路径或 URI。** GUI 或系统选择器返回的内容先注册到 `FileAccessManager`，得到仅供本地使用的 `FileHandleId`。随后 Rust Core 通过 `FileSource` / `FileSink` 抽象读取与写入，按能力协商选择随机访问或顺序流方式。

```rust
struct FileCapabilities {
    random_read: bool,
    random_write: bool,
    persistent_access: bool,
    size_known: bool,
}

// 伪接口，具体错误与生命周期通过设计评审确定
trait FileSource {
    async fn metadata(&self) -> Result<FileMeta, FileError>;
    async fn read_at(&self, offset: u64, len: usize)
        -> Result<Vec<u8>, FileError>;
}
```

随机访问能力只应由实际文件句柄验证得到，不应因为 URI 后缀或操作系统类型而推断。对只能顺序读取的源文件使用单流传输，必要时征求用户同意后复制到应用沙盒；特别注意大文件复制的磁盘空间成本。

### 10.2 Android 方案

使用系统 Storage Access Framework 选择文件，获得 `content://` URI；由 Kotlin 原生适配层调用 `ContentResolver`，为 Rust 提供可用句柄或流。部分提供者返回的描述符不可 seek，应检测并回退。跨重启使用系统支持的持久 URI 授权；权限失效、原文件被移动/删除时进入 `WaitingForFileAccess` 并请求重新授权。

### 10.3 iOS / macOS 方案

通过文档选择器与 Security-scoped URL 访问应用沙盒外的文件；根据需要采用 iOS `fileAccessMode: 'scoped'`，注意它适用于文档选择器而非全部媒体选择情形。自定义 Rust I/O 必须确保原生授权生命周期在读写期间有效；必要时使用文件协调和 Bookmark 恢复机制。不要把一次性 `file://` 字符串当成永久可用路径。

### 10.4 接收文件策略

MVP 移动端优先把块写入应用沙盒 `.part`，完成后通过系统分享/导出到用户目标位置；这样简化并发随机写与恢复。代价是导出时可能额外占用存储，后续再增加直接写入受授权目录的能力。

## 11. 数据存储与商业扩展边界

| 数据种类 | 基础版策略 | 保留期限 / 备注 |
|---|---|---|
| 聊天消息正文 | 内存、有界缓存 | 应用退出即消失 |
| 当前设备在线状态 | 内存 | 超时移除 |
| 消息去重与同步回声缓存 | 内存 | TTL / 有界容量 |
| 用户偏好与允许的设备 | 本地配置 | 由用户管理 |
| 设备身份密钥 | 本地安全存储 | 直到重置身份 |
| 未完成文件与 checkpoint | 本地文件 | 任务完成/取消后按策略清理 |
| 完成后的接收文件 | 用户指定位置 | 由用户管理 |
| 详细日志 | 默认不记录敏感内容 | 日志级别与保留受控 |

未来企业版可以通过 `EventSink`、`PolicyProvider`、`DeviceDirectory` 等扩展点增加**经明确授权的**消息留存、组织策略与审计。避免现在把企业版数据库对象侵入基础通信协议或 UI。企业存储的合规要求、许可模式和审计范围需单独产品评审。

## 12. UI 与 Tauri API 边界

### 12.1 Commands（示意）

```text
list_devices() → DeviceSummary[]
request_pairing(device_id) → PairingChallenge
confirm_pairing(challenge_id, decision) → Result
send_text(device_id, text, copy_hint) → MessageId
send_clipboard_text(device_id) → MessageId
register_file(file_reference) → FileHandleId
start_transfer(device_id, file_ids[]) → TransferId
pause_transfer(transfer_id) / resume_transfer(transfer_id)
cancel_transfer(transfer_id)
get_state_snapshot() → AppSnapshot
```

Tauri Command 需限制被允许的窗口、参数及路径访问范围；前端的 `FileHandleId` 不是跨设备通行证，Rust 服务端应再次检查权限和所有权。

### 12.2 Events（示意）

```text
DeviceOnline / DeviceOffline
PairingRequested / PairingCompleted
MessageReceived / MessageDeliveryChanged
ClipboardReceived / ClipboardApplied / ClipboardApplyFailed
TransferProgress / TransferStateChanged
FileAccessRequired / AppError
```

前端首次订阅时采用“**快照 + 增量事件**”机制以避免漏事件；事件附带递增序号和版本，发现断档时重新抓取快照。文件进度按固定节流周期发送（例如 100–250 ms），而非每个分块读写都触发 UI 更新。

## 13. 工程目录建议

```text
fluxy/
├── Cargo.toml                    # Rust workspace
├── crates/
│   ├── fluxy-protocol/
│   ├── fluxy-identity/
│   ├── fluxy-discovery/
│   ├── fluxy-transport/
│   ├── fluxy-core/
│   ├── fluxy-messaging/
│   ├── fluxy-clipboard/
│   ├── fluxy-transfer/
│   ├── fluxy-file-access/
│   └── fluxy-checkpoint/
├── apps/
│   ├── cli/                      # CLI/协议与性能验证
│   └── gui/
│       ├── src/                  # SolidJS + Neut
│       └── src-tauri/            # Tauri 2 适配
├── plugins/
│   └── file-access/             # 需要时再建设的移动原生插件
├── docs/
│   ├── architecture.md
│   ├── protocol.md
│   └── test-plan.md
└── .github/workflows/
```

**落地原则**：这是逻辑边界的目标结构，不要求第一天就拆出所有 crate。可以先从 `protocol / core / transport / transfer / app` 五个工程单元开始，接口稳定后再拆分，避免抽象过度。

## 14. 测试、质量与安全要求

### 14.1 功能验收

- 同一局域网的两台设备能够发现、经用户确认配对、撤销信任并重新配对。
- 一对一文字发送/接收和消息确认可用；重发不产生重复展示。
- 接收端开启自动复制后，收到文字可以写入剪贴板；拒绝或失败时给予明确状态提示。
- 文件传输支持暂停、取消、网络中断恢复；重连后只补传缺失或未确认的块。
- 10 GiB 及以上文件完成后 BLAKE3 摘要一致，不出现整文件内存驻留。
- 传输进行时，聊天与控制操作可持续使用。
- 关闭应用后不留下可检索的基础版聊天记录；允许存在明确列出的配置和传输检查点。

### 14.2 故障注入

断网、Wi-Fi 切换、强制退出、源文件修改、目标磁盘空间不足、只读目录、失效的移动端授权、同名文件、非法相对路径、异常协议帧、慢速接收方、大量小文件与并行任务压力测试。

### 14.3 安全清单

- 真实认证的 QUIC/TLS；配对信任不可仅凭 IP 或 mDNS 字段。
- 接收文件需经授权；默认不自动执行、打开或运行接收的文件。
- 限制控制消息、文本消息、并发流和任务数量，抵御资源耗尽。
- 接收目录的路径正规化、穿越防御、符号链接检查和同名处理。
- UI/WebView 最小权限，严格 Tauri capabilities，不把敏感密钥或文件内容暴露给前端。
- 默认无剪贴板自动监听，按目标设备允许自动覆盖；提供快速禁用和清除可信设备。
- 诊断日志默认对消息正文、剪贴板文本及完整文件路径脱敏。

## 15. 开发计划与里程碑

| 阶段 | 交付目标 | 验收标志 |
|---|---|---|
| M0：协议与核心 PoC | CLI 双机发现/配对、QUIC 文字消息 | 无 GUI 可靠直连与认证 |
| M1：桌面通信 MVP | SolidJS+Neut 界面、设备列表、聊天、主动剪贴板 | 三大平台中至少两平台互通 |
| M2：可靠传输 | Manifest、分块校验、基本并发、断点续传 | 10 GiB、断网及强退恢复测试 |
| M3：桌面完整度 | 拖拽/文件夹、任务队列、通知、自动剪贴板可选 | 完整桌面交互与安全测试 |
| M4：移动支持 | Android/iOS 前台通信、URI/授权适配、接收与导出 | 桌面↔手机双向互通 |
| M5：优化与发布 | 并发自适应、性能调优、安装包、签名与文档 | 跨平台回归与发布标准达成 |

**建议开发顺序**：优先实现 Rust CLI 双端通信与文件续传，再做复杂 GUI；移动端文件访问尽早做小型 PoC，以验证“可随机读/可持久授权”而不是最后才发现平台障碍。每个阶段独立验收后再展开下一阶段。

## 16. 待确认的关键决策（ADR）

1. **ADR-001 协议编码**：Protobuf 还是另一个跨语言二进制编码；明确字段兼容规则。
2. **ADR-002 设备身份**：密钥类型、配对 UX、首次确认和恢复流程。
3. **ADR-003 文件身份**：Manifest 的版本指纹、最终摘要计算时机及源文件变更策略。
4. **ADR-004 传输持久性**：何时 fsync、检查点原子更新策略与崩溃恢复开销。
5. **ADR-005 移动端文件句柄**：对不可 seek 的 URI 采取顺序发送还是复制暂存；两者何时切换。
6. **ADR-006 剪贴板权限**：每设备默认关闭、一次允许、长期允许的交互与自动复制失败回退。
7. **ADR-007 版本范围**：首个对外发布版本是否要求所有五端，还是分阶段公开预览。

## 17. 参考资料（官方/一手资料）

- Tauri 2 — Calling Rust from the Frontend：<https://v2.tauri.app/develop/calling-rust/>
- Tauri 2 — Mobile Plugin Development：<https://v2.tauri.app/develop/plugins/develop-mobile/>
- Tauri 2 — Dialog API（`fileAccessMode`）：<https://v2.tauri.app/reference/javascript/dialog/>
- Android — Storage Access Framework：<https://developer.android.com/training/data-storage/shared/documents-files>
- Apple — Document Picker：<https://developer.apple.com/documentation/UIKit/UIDocumentPickerViewController>
- Quinn — SendStream priority：<https://docs.rs/quinn/latest/quinn/struct.SendStream.html>
- Neut 组件库：<https://github.com/thep0y/neut>

---

**结论**：Fluxy 应首先完成可独立运行和测试的 Rust 局域网通信内核，再通过 Tauri 2 + SolidJS + Neut 交付跨平台界面。聊天、剪贴板与文件传输平级；基础版不留存聊天记录，文件续传检查点独立存在；通过平台文件访问抽象将移动端权限限制隔离于传输引擎之外。
