# Fluxy v0.2.0 开发计划（含 Android v0.3.0 并行轨道）

| 项 | 内容 |
| --- | --- |
| 依据 | docs/DESIGN.md（v0.2.0） |
| 编制日期 | 2026-10-21 |
| 适用分支 | 桌面 0.2.X；移动 0.3.X |
| 计划周期 | 桌面 20 周（2 名桌面工程师 + 0.5 QA）；Android 并行至 W21（1 名 Android 工程师） |
| 状态 | Draft，待确认第 15 节的产品决策 |

---

## 0. 阅读指南

本计划把 DESIGN.md 的 M0–M5 拆成可直接建 Issue、直接排期、直接验收的任务。每个任务都有唯一 ID、产出物、人日估算、依赖和完成判据。

阅读顺序建议：

1. 第 1–2 节确认范围与当前起点。
2. 第 3–8 节确认架构、协议、数据与 UI 方案。
3. 第 9–10 节是排期与工作分解，直接用于建里程碑。
4. 第 11–12 节是验收与工程规范。
5. 第 14 节是明天就可以开始执行的十天计划。
6. 移动端专项方案、fd 桥、权限与测试矩阵见 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md)。

本计划的核心判断：**Fluxy 的第一交付物是可独立运行、可测试的 Rust 引擎，GUI 是它的第一个消费者。** 因此所有阶段性演示都先以 CLI 跑通，再接入 Tauri。

### 0.1 一页结论

- 桌面 20 周、2 名桌面工程师 + 1 名 Android 工程师 + 0.5 QA；桌面轨道 133 人日，移动轨道 38 人日（总 171），W20 为集中缓冲。
- 前 5 周只做 Rust CLI：发现、配对、加密连接、聊天 ACK、断网重连；协议在 W5 冻结。
- GUI 从第 2 周起用 mock 事件并行开发，Tauri 只做薄适配，业务状态全部在 Engine。
- 文件传输采用接收方拉取 + 4 MiB 分块 + 两端恢复元数据；10 GiB、断网、杀进程是 M2 硬门槛；桌面文件夹按批次复用单文件续传。
- 控制帧采用 Protobuf Envelope，配置与检查点仍用 JSON；数据块保持二进制定长头。
- Android 走独立 v0.3.0 列车：minSdk 33、SAF fd、NsdManager + 热点网关发现、dataSync 前台服务、LNP 权限；只发 GitHub APK，桌面 v0.2.0 不被阻塞，详见 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md)。
- 最大风险是 Android LNP/SAF/前台服务、热点下 mDNS 失效与 QUIC 控制流被大文件挤占；热点由网关探测兜底，前者 P0 预研，后者 M2 压测。
- 明天从 P0-01/P0-02 开始，第 10 个工作日必须能演示“发现→配对→聊天”。

本次按 2026-10-21 的 DESIGN 与产品决策修订：新增桌面文件夹批次（+5）、Protobuf Envelope（+1）、身份密钥轮换（+1）、热点网关发现回退（+2）；桌面从 124 增至 133 人日，M1 增至 24、M3 延至 W15、M5 排 W16–19，v0.2.0 仍在 W20；Android 改为 GitHub APK、W21 发布，移动轨道 38 人日不变。

---

## 1. 范围、假设与交付定义

### 1.1 v1 必须交付

| 能力 | v1 边界 |
| --- | --- |
| 设备发现 | 同一局域网/手机热点内 mDNS/NSD 自动发现 + 热点默认网关探测；桌面保留手动 host:port 诊断，移动端仅自动发现、不用 QR |
| 设备配对 | 首次配对展示 6 位 SAS，双方确认后写入信任列表；之后证书钉扎重连 |
| 即时聊天 | 纯文本，单条 ≤ 32 KiB，仅内存，应用层 ACK，不持久化 |
| 共享剪贴板 | 纯文本，≤ 1 MiB；主动共享为主；桌面端自动监听发送为可选开关，接收按设备策略 |
| 文件传输 | 多文件与文件夹（桌面 v0.2.0；移动 v0.3.0 仅多文件）、分块、校验、并发、暂停、取消、断线重连、跨重启续传 |
| 传输可靠性 | 4 MiB 分块、单文件 4 路并发、BLAKE3 完整性校验、缺失块重取 |
| 平台 | Windows / macOS / Linux 为 v0.2.0；Android 为第一移动平台，独立 v0.3.0 beta；iOS 随后 |
| 网络接口 | 桌面支持以太网、Wi-Fi、热点和开发用回环；自动选择默认路由/首选私网地址，可手动指定网卡/IP，一次一个活动地址。移动端固定只用 Wi-Fi/热点，网络类型不可切换 |
| 安全 | 设备身份、加密连接、接收授权、路径检查、自动剪贴板默认关闭 |

### 1.2 明确不做（v1 非目标）

- 账号体系、云端中继、NAT 穿透、跨公网连接。
- 聊天记录持久化、全文检索、消息撤回、已读回执、群聊。
- 移动端文件夹传输、文件夹差异同步/同步盘语义；桌面文件夹递归传输属于 v0.2.0。
- v0.2.0 不同时绑定多个 LAN 地址；多网卡通过手动选择单一活动地址，不提供多端点并发。回环仅用于开发/测试，不属于产品网络模式。
- 剪贴板图片、富文本、文件对象。
- 离线消息队列（对端离线时消息直接失败，用户手动重试）。
- 自动更新、遥测、崩溃上报（默认全部关闭或不做）。
- 网络切换时的 QUIC 连接迁移（Wi-Fi 与有线切换按重连处理）。

这些非目标会写进 README 和发布说明，避免范围蔓延。

### 1.3 团队与工期假设

- 2 名桌面工程师：A 偏 Rust/网络，B 偏 SolidJS/Tauri/交互。
- 1 名 Android/Kotlin 工程师 W1 入职并参与 P0-06 预研，W9 起全职到 W21；0.5 名 QA 从 M1 开始介入，W18 后兼顾移动矩阵。
- 桌面 v0.2.0 在 W20 发布；Android beta W20 出 GitHub APK，v0.3.0 W21 发布，不上架 Play。
- Android 前置：release keystore、两台 Android 13+ 手机（验证热点主机/客户端）、Android 16 真机/模拟器；不需要 Play 开发者账号。
- iOS 前置：Apple Developer 账号与 macOS runner；没有则 iOS 顺延，不影响 Android 与桌面。
- 若桌面只有 1 人，按 30–34 周估算；网络层与 UI 的联调窗口无法压缩。

### 1.4 完成定义（DoD）

一个任务只有同时满足以下条件才算完成：

1. 代码合并到 0.2.X，CI 全绿（fmt、clippy、test、biome、tsc）。
2. 新增行为有单元测试或集成测试；协议字段有 round-trip 测试。
3. 对外命令、事件、配置项、错误码有对应文档或 Rust doc。
4. 在至少一台真实设备上手动验证过主路径。
5. 没有新增 TODO/FIXME 落在主路径；临时降级必须建 Issue 并标注。
6. 涉及 UI 的，必须有空状态、加载态、错误态三种界面。

---

## 2. 现状盘点

当前仓库是 Tauri 2 + SolidJS + TypeScript 官方模板，尚未开始业务开发。

| 项目 | 现状 | 差距 |
| --- | --- | --- |
| 前端 | Solid 1.9 + Vite 8 + TS 6，模板 App.tsx | 需替换为 Neut + Tailwind 4 + 路由 |
| Tauri | Tauri 2，greet 示例命令 | 需改为引擎适配层 |
| Rust | 单 crate src-tauri，仅有 greet | 需建立 workspace 与五个 crate |
| 协议 | 无 | 需从零设计并冻结 v1 |
| 测试 | 无 | 需建立单测、集成、故障注入、基准 |
| CI | 无 | 需建立三平台矩阵 |
| 版本 | package.json 0.2.0，Cargo/tauri.conf 0.1.0 | 需统一为 0.2.0 |
| 仓库 | 分支 0.2.X 尚无任何提交 | 需建立首个基线提交 |
| 文档 | DESIGN、DEVELOPMENT_PLAN、MOBILE_PLAN | 缺 PROTOCOL、TESTING、ADR、RELEASE |
| 依赖 | 未安装 Neut、通知、剪贴板、路由 | 需在 P0 一次性锁定版本 |
| 移动端 | 未初始化 | Android 优先，独立轨道；P0 预研，W9 起并行，详见 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md) |

### 2.1 必须立即修正的问题

1. 版本三处不一致（0.2.0 / 0.1.0 / 0.1.0），统一到 0.2.0。
2. 删除 greet 示例，避免模板代码进入后续提交历史。
3. 建立 Cargo workspace，否则 M0 的 CLI 无法复用核心库。
4. 增加 LICENSE（MIT，与 package.json 一致）、README、.editorconfig。
5. package.json 增加 packageManager 字段，锁定 bun 1.4.3。
6. tauri.conf.json 的 productName 改为 Fluxy，窗口设置最小尺寸。
7. 补 .gitignore：target、dist、node_modules、.DS_Store；提交 src-tauri/gen/android 以保留 manifest/Gradle 定制。
8. 建立第一条提交：chore: bootstrap Fluxy 0.2.0 workspace。

---

## 3. 目标架构

### 3.1 仓库结构

~~~text
fluxy/
├─ Cargo.toml                     # workspace，统一版本与依赖
├─ crates/
│  ├─ fluxy-proto/                # 纯协议：帧、消息、错误码、版本、限制
│  ├─ fluxy-core/                 # 领域模型 + 端口 trait + 聊天/剪贴板/传输状态机
│  ├─ fluxy-net/                  # quinn/rustls 传输、mDNS、配对密码学
│  ├─ fluxy-app/                  # Engine 装配、配置、信任库、检查点存放、平台实现
│  └─ fluxy-cli/                  # M0 原型、压测与运维工具
├─ src-tauri/                     # Tauri 适配：commands、channels、插件注册
├─ plugins/
│  └─ tauri-plugin-fluxy-mobile/  # Kotlin/Swift：SAF fd、NsdManager、FGS、Keychain
├─ src/                           # SolidJS UI
├─ tools/
│  └─ udp-chaos/                  # UDP 故障注入代理（丢包/延迟/断连）
├─ docs/
│  ├─ DESIGN.md
│  ├─ DEVELOPMENT_PLAN.md
│  ├─ MOBILE_PLAN.md
│  ├─ PROTOCOL.md
│  ├─ TESTING.md
│  ├─ SECURITY.md
│  ├─ RELEASE.md
│  └─ adr/
└─ .github/workflows/
~~~

### 3.2 依赖方向

~~~text
fluxy-proto  ←  fluxy-core  ←  fluxy-net  ←  fluxy-app  ←  fluxy-cli
                                             ↑
                                          src-tauri  ←  src (TS)
~~~

硬性规则：

- fluxy-proto 不依赖 tokio、quinn、tauri，保持纯数据与编解码。
- fluxy-core 不依赖 tauri，不引用任何 GUI 类型；文件、剪贴板、时钟、传输、发现都通过 trait 注入。
- fluxy-net 实现 core 定义的 Transport 与 Discovery 端口，不反向依赖 app。
- fluxy-app 负责把 net、core、平台实现装配成 Engine，并对 CLI 与 Tauri 暴露同一套命令/事件。
- src-tauri 只做参数转换、权限与生命周期桥接，不写业务逻辑。
- 移动端平台能力只通过 plugins/tauri-plugin-fluxy-mobile 注入；core 不引用 Android/iOS 类型。
- 前端不直接理解 Rust 结构体，全部使用 tauri-specta 生成的 bindings。

### 3.3 运行时模型

采用单 Engine Actor + 每连接任务 + 每文件任务的结构：

~~~text
Tauri commands ─┐
CLI commands  ──┼─→ mpsc<Command> ─→ Engine actor（唯一可变状态）
                │                         │
                │                         ├─→ broadcast<Event> ─→ Tauri Channel / CLI 输出
                │                         ├─→ 连接任务（每 peer 1 个）
                │                         └─→ 传输任务（每文件 1 个，内部 4 个读取 worker）
~~~

约定：

- Engine actor 独占 PeerTable、TrustStore 内存视图、聊天会话、传输注册表。
- 网络任务只解析帧并把 NetworkEvent 送回 actor，不直接改共享状态。
- 文件读写走 spawn_blocking，使用有界通道，避免阻塞 Tokio 工作线程。
- 事件总线使用 tokio::sync::broadcast，慢消费者只丢进度事件，不丢终态事件。
- Transport 的 bind 地址由 NetworkSelection 注入；Engine 不直接读系统网卡，便于测试与平台适配。
- 所有对外命令幂等或带明确错误码；Engine 停止时先取消任务再落检查点。

### 3.4 模块职责

| 模块 | 职责 | 不做什么 |
| --- | --- | --- |
| proto | Envelope/Protobuf schema、帧格式、错误码、版本协商、常量上限 | 不建立连接、不读写文件 |
| core::identity | 设备 ID、密钥对、签名与验签、SAS 计算 | 不直接落盘 |
| core::trust | 信任记录模型、策略判定 | 不决定 UI 文案 |
| core::chat | 会话内存状态、发送状态、ACK 超时 | 不持久化 |
| core::clipboard | 共享/接收状态、摘要去重、循环防护 | 不直接调用系统剪贴板 |
| core::transfer | Manifest、文件夹批次、分块位图、调度、检查点模型、状态机 | 不依赖具体路径 API |
| core::ports | Transport、Discovery、ClipboardPort、FileAccess、Clock、EventSink | 不含实现 |
| net | QUIC 连接、mTLS 校验、帧读写、mDNS、热点默认网关探测（GatewayProbe）、配对握手 | 不关心 UI 与传输落盘 |
| app | Engine 装配、JSON 存储、检查点原子写、桌面平台实现 | 不含平台 UI 逻辑 |
| tauri | 命令/事件映射、窗口与通知、插件权限 | 不实现协议 |
| mobile plugin | SAF fd、NsdManager、local_network_info（网关/接口）、FGS、Keystore/Keychain、分享入口 | 不实现协议、不解析文件内容 |

### 3.5 多网卡与本地地址选择

Windows/macOS/Linux 常同时存在以太网、Wi-Fi、VPN、Hyper-V/Docker 虚拟网卡。mDNS 如果广播了错误地址，对端会“看得到设备但连不上”。v0.2.0 把“选哪块网卡/哪个 IP”作为一等设置，而不是让用户猜。

模型：

| 字段 | 说明 |
| --- | --- |
| mode | auto / manual |
| interfaceId | 平台接口标识（名称 + 索引）；只在本机使用 |
| address | 选定的 IPv4/IPv6 地址；链路本地 IPv6 带 scope id |
| port | 监听端口；0 表示系统分配 |
| devLoopback | 仅开发/测试：显式允许 127.0.0.1/::1 与不同端口；默认关闭 |

规则：

- auto：枚举 up、非 loopback、非 point-to-point 的接口；优先默认路由接口，其次首选私网 IPv4；避开常见 VPN/虚拟网卡名称；选一个主地址绑定。没有默认路由时选第一个有私网地址的接口，并在 UI 标注“非默认路由”。Ethernet 与 Wi-Fi 同等参与排序，不因接口类型直接排除。
- 开发回环：仅当 CLI 传 `--dev-loopback` 或配置 `dev.allowLoopback=true` 时，允许绑定 127.0.0.1/::1，用两个实例、不同端口直连测试；回环不参与 mDNS 广播、不进入自动选择，也不降低 SAS/证书校验。
- manual：用户选接口与地址；启动/切换时校验仍存在，失败返回 2003；回环地址只在 devLoopback 开启时接受。
- 一次只绑定一个本地地址；v0.2.0 不同时服务多个 LAN。需要多 LAN 时用手动切换，后续版本再评估多端点。
- QUIC endpoint 绑定该地址；mDNS 只广播该地址；手动 host:port 连接也从该地址发起。
- 仅绑定源地址不保证绕过 OS 路由；M2-11 验证 Windows IP_UNICAST_IF、macOS IP_BOUND_IF、Linux SO_BINDTODEVICE；不可用时 UI 明确提示仍受 VPN/路由影响。
- 网络变化：每 2 秒轮询接口表；auto 重新选主地址并重绑，manual 地址消失则报 2003 并提示重选，不静默降级。
- 切换会断开当前连接；有活动传输时先暂停并落检查点，重绑后按现有恢复流程继续。
- IPv6 链路本地地址必须携带 scope id，UI 显示为 address%interface。
- 接口列表只在本机 UI/日志出现，不进入协议，不上传。

---

## 4. 关键技术决策

| ID | 决策 | 推荐方案 | 理由 | 备选 |
| --- | --- | --- | --- | --- |
| ADR-01 | 代码组织 | Cargo workspace + 5 个 crate | CLI 与 GUI 共用同一引擎，边界可测 | 单 crate 堆模块 |
| ADR-02 | 控制帧编码（对应 DESIGN ADR-001） | 长度前缀 + Protobuf Envelope（prost）；数据块用二进制定长头 | 设计推荐；schema 演进；数据块零编码开销 | JSON、CBOR |
| ADR-03 | 分块调度 | 接收方拉取（pull），窗口 4 | 天然支持续传、背压、缺失重取 | 发送方推送 |
| ADR-04 | 设备身份 | Ed25519 密钥对；device_id = BLAKE3(pubkey) 前 16 字节 | 无中心、可自证、体积小 | X.509 CA、账号体系 |
| ADR-05 | 连接认证 | 自签证书 mTLS + 证书指纹钉扎 + 首次配对 SAS | 复用 QUIC TLS 1.3，首次配对防 MITM | 预共享密钥、Noise |
| ADR-06 | 连接复用 | 每对设备 1 条 QUIC 连接；1 条控制流 + 每块 1 条单向流 | 满足设计第 4 节，隔离大文件 | 每消息一条连接 |
| ADR-07 | 持久化 | 无数据库；JSON 原子写 + 每传输独立检查点 | 设计第 8 节；故障面小 | SQLite、sled |
| ADR-08 | IPC 类型 | tauri-specta 生成 TS bindings 与事件类型 | 前后端契约单一来源 | 手写类型 |
| ADR-09 | UI 基础 | Neut 0.0.17 精确锁定 + Tailwind 4 + lucide-solid | 项目定位与作者生态一致；零外部依赖 | Kobalte、Park UI |
| ADR-10 | 发现 | mdns-sd/平台 NSD + 热点默认网关 UDP 探测；有限广播兜底；桌面手动地址仅作诊断 | 热点/AP 隔离下仍可发现；不依赖 QR | 只做 mDNS |
| ADR-11 | 文件访问 | core 定义 FileSource/FileSink/FileAccessManager；桌面 std::fs，Android SAF 经插件，iOS 安全作用域 URL | 设计第 7 节 | 直接传路径字符串 |
| ADR-12 | 测试 | nextest + proptest + cargo-fuzz + 自研 udp-chaos + criterion | 覆盖解码、断网、损坏、性能 | 仅手工测试 |
| ADR-13 | 移动端架构 | Tauri 2 + 自研 Kotlin/Swift 插件；SAF fd、NsdManager、dataSync FGS、Keystore | 移动差异集中，core 保持平台无关 | 第三方 android-fs 插件、Tauri 3 alpha |
| ADR-14 | 发布列车 | 桌面 v0.2.0 与 Android v0.3.0 分离 | 移动不阻塞桌面，且移动需要独立硬件/发布节奏 | 单一版本强行捆绑 |
| ADR-15 | 多网卡选择 | 单一活动地址；auto 选默认路由/首选私网，manual 覆盖；QUIC 绑定与 mDNS 广播使用同一地址 | 解决 VPN/虚拟网卡误选；行为可解释 | 绑定全部地址、按 peer 选源地址 |
| ADR-16 | 文件夹传输 | batch_id 聚合多个单文件 TransferId；每文件独立 manifest/checkpoint；相对路径 + 空目录清单 | 复用续传；风险低；桌面优先 | 单 TransferId 多文件 Manifest |

补充说明：

- Neut 目前为 0.0.x，API 可能变动。前端必须通过 src/ui 下的一层包装组件使用它，禁止在业务组件里散落 Neut 导入，以便升级或替换。
- tauri-specta 2.0 仍为 rc 系列，P0 锁定具体 rc 版本并记录升级条件；若阻塞，退化为手写 bindings + 类型测试。
- 控制帧上限设为 8 MiB，Manifest 上限 4 MiB，足以覆盖 128 GiB 文件（32768 块 × 约 45 字节）；超出则在 Offer 阶段拒绝。

---

## 5. 协议设计 v1

### 5.1 传输与连接

- 传输：QUIC over UDP，ALPN 固定为 fluxy/1。
- 监听地址由 §3.5 的 NetworkSelection 决定；生产模式可选以太网/Wi-Fi/热点，QUIC 只绑定选定地址，mDNS 只广播该地址；开发回环用 --dev-loopback 显式开启，不参与 mDNS。
- 发现：mDNS/NSD 为主；热点客户端向默认网关的固定 UDP 42420 发送 FLUX-DISCOVER/1，热点主机监听并回复 device_id/名称/QUIC 端口/证书指纹；mDNS 2s 无结果或热点接口无组播时触发，默认网关失败再向 255.255.255.255 发有限广播，禁止全网段扫描。探测包不建立信任，仍需 SAS。手机热点由用户在系统设置开启，Fluxy 不调用热点开关。
- 加密：TLS 1.3，双方自签证书，mTLS。
- 首次连接：接受未知证书但记录指纹，应用层交换身份并生成 SAS；用户确认后写入信任库。
- 后续连接：校验证书指纹与信任库一致，并校验应用层签名，任一不符立即断开。
- 0-RTT 关闭；会话恢复允许，但每次连接仍执行应用层 Hello。
- 保活：keepalive 10s，空闲超时 30s；连接断开后指数退避重连（1s、2s、4s、8s、15s、30s，上限 30s）。
- 每条连接由发起方打开一条双向控制流；服务端接受后进入握手。数据块使用独立单向流。
- QUIC 传输参数：max_concurrent_uni_streams 16、max_concurrent_bidi_streams 8、stream_receive_window 8 MiB、receive_window 32 MiB、max_idle_timeout 30s、keep_alive_interval 10s。4 MiB 块要求单流接收窗口不小于 4 MiB，不能沿用 quinn 默认值，否则会在块中途停顿。

### 5.2 帧格式与 Envelope

控制帧使用长度前缀 + Protobuf Envelope；数据块使用独立二进制定长头，不经过 Protobuf。

控制帧：

~~~text
+--------+---------------------------+
| len u32| Envelope (protobuf)       |
+--------+---------------------------+
len 为 Envelope 字节数，最大 8 MiB
~~~

Envelope 字段：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| protocol_version | uint32 | 当前为 1 |
| message_id | bytes16 | UUIDv7；所有控制消息必填 |
| source_device_id | bytes16 | 必须与 TLS 对端身份一致 |
| session_id | bytes16 | 发起方在 HelloAck 后生成；Hello/HelloAck 为全零 |
| created_at_ms | int64 | 仅显示/诊断，不作可信时序凭据 |
| kind | int32 | 消息类型；未知值拒绝 |
| payload | bytes | 该 kind 对应消息的 Protobuf 编码 |

数据块流（每条单向流一个块）：

~~~text
+--------+---------------+-------------+----------+-------------+--------+---------+
| "FCH1" | transfer_id16 | chunk_idx4  | offset8  | payload_len4| hash32 | payload |
+--------+---------------+-------------+----------+-------------+--------+---------+
头部 68 字节；payload_len 为该块字节数（末块可小于 chunk_size）；hash32 为该块 BLAKE3
~~~

约束：

- 整数一律大端；Envelope 的字节序由 Protobuf 定义。
- transfer_id 使用 UUIDv7 的 16 字节形式；v1 每条数据流对应一个单文件传输，transfer_id 即该文件的 file_id；文件夹批次用控制帧 batch_id 关联。
- kind 使用 int32 枚举值，字段号只追加不复用；未知字段跳过，未知 kind 返回 1003。
- 控制帧解析失败即关闭连接并记录协议错误；不得猜测。
- 字符串统一 UTF-8；拒绝非法编码、超长字段、message_id/session_id 长度不符或来源不匹配的帧。
- 数据块长度、offset、索引、块哈希不合法时 reset 该流，不关闭整条连接。

### 5.3 握手与配对

握手（每次连接，所有帧封装在 Envelope 中；Hello/HelloAck 的 session_id 为全零）：

~~~text
A                                   B
|-- TLS 1.3 (mTLS, 自签) ----------->|
|-- Hello -------------------------->|  version, device_id, name, os,
|<------------------------ HelloAck -|  cert_fp, nonce, ed25519 签名
|-- Capabilities -------------------->|  chunk/stream/feature 交集
|<----------------------- Capabilities -|
|-- PairRequest / ResumeHello ------>|  仅未信任或需要重新配对时
|<------------------------ PairReply-|
|-- PairConfirm -------------------->|
|<------------------------ Ready ----|
~~~

- Hello 的 version 与 protocol_version 不一致时返回 1001；Capabilities 取双方交集。
- Capabilities 至少包含 max_control_frame、max_chunk_size、max_streams、features（folder/resume/clipboard）、clipboard_max_bytes；未知 feature 忽略。
- 发起方在 HelloAck 后生成 session_id，并在 Capabilities 及后续 Envelope 中使用；接收方必须回显同一 session_id。
- 移动端 v0.3.0 只通过自动发现进入配对，不提供二维码或手动地址；桌面端手动地址仅作诊断，不作为移动端前提。热点场景由网关探测覆盖。
- 基础控制：控制流空闲 10s 发 Ping，30s 无 Pong 视为半开并重连；与 QUIC keepalive 10s/空闲 30s 一致，Ping 不计入聊天限速。
- Error 携带 code、message、retryable；不可恢复错误关闭会话，可恢复错误由上层提示，不把 Error 当业务成功。

配对（首次）：

1. 双方 Hello 中携带证书指纹、设备公钥、随机数，并对握手 transcript 签名。
2. 双方计算 SAS = BLAKE3(排序后的两个 device_id + 两个 nonce + 两个证书指纹) 的前 6 位十进制数字。
3. 两端同时显示 6 位数字，用户比对一致后在两端分别确认；任一端取消即失败。
4. 双方交换对包含 SAS 的 transcript 的签名，验证通过后写入信任库。
5. 配对窗口 120 秒，每设备每分钟最多 5 次配对尝试；失败不写入任何状态。

安全边界：

- 首次配对依赖用户比对 SAS，这是 v1 唯一的防 MITM 手段，必须在 UI 上明确提示“只在两台设备显示相同数字时确认”。
- 信任库记录 device_id、公钥、证书指纹、名称、平台、配对时间、策略。
- 设备名称可变，device_id 不可变；名称冲突时 UI 追加平台或 ID 前缀。
- 密钥优先存 OS 凭据库（keyring）；不可用时退回 0600 权限文件并在设置页告警。

### 5.4 聊天

消息：

| 字段 | 说明 |
| --- | --- |
| msg_id | UUIDv7，发送方生成 |
| text | 纯文本，UTF-8，≤ 32 KiB |
| sent_at | Unix 毫秒 |
| reply_to | 可选，仅 UI 引用 |
| copy_hint | 可选；接收端按该设备 clipboard 策略决定自动写入，否则只显示“复制”入口 |
| sequence | 每会话单调递增，仅用于 UI 排序与去重 |

msg_id 即 Envelope.message_id；payload 中不重复。

流程：

1. A 发送 ChatText；UI 立即显示“发送中”。
2. B 收到后投递到内存会话并回 MessageAck（status=delivered）。
3. A 收到 MessageAck 后标记“已送达”。
4. 1s/3s/7s 重试 3 次；10s 未确认标记失败，用户可手动重发。
5. 对端离线时直接返回 PeerOffline，不排队。
6. 按 message_id 去重，缓存 10 分钟/最多 1024 条；重复消息只回 MessageAck，不重复展示；sequence 跳号不阻塞接收。

不持久化：应用退出即丢失；切换设备或重启不恢复。UI 必须提示“聊天不保存”。

### 5.5 剪贴板

消息：

- ClipboardOffer：digest、长度、预览前 160 字符、origin_id、hop。
- ClipboardText：digest、完整文本。
- ClipboardAck：applied / rejected / too_large / failed。

主动共享流程：

1. 发送方读取本机剪贴板文本，计算 BLAKE3 摘要。
2. 发送 ClipboardOffer；接收方弹出预览与“接收/拒绝”。
3. 接收方同意后发送方发 ClipboardText；接收方写入系统剪贴板并回 ClipboardAck。

接收策略：可对每台设备设置询问 / 自动接收 / 拒绝。自动接收只取决于该设备策略；全局自动同步开关只控制本机是否监听并主动发送，二者不要混为一个开关。

自动监听与发送（桌面，M3）：

- 默认关闭；开启后每 300ms 防抖读取剪贴板。
- 仅同步文本，≤ 1 MiB。
- 内容摘要与最近 16 条已应用摘要相同则忽略，防止回声。
- 消息携带 origin_id 与 hop；hop 最大为 1，收到后不再转发给第三方，杜绝循环。
- 移动端因系统限制默认不提供自动监听，只保留手动共享与接收。

### 5.6 文件传输

消息与流：

| 消息 | 方向 | 作用 |
| --- | --- | --- |
| TransferOffer | 发送方 → 接收方 | 单文件或文件夹批次；单文件带根哈希/manifest 摘要，文件夹带相对路径元数据与 batch_id |
| TransferAccept | 接收方 → 发送方 | 接受并声明目标位置策略 |
| TransferReject | 接收方 → 发送方 | 拒绝原因 |
| TransferManifest | 发送方 → 接收方 | 每块 BLAKE3 哈希数组 |
| ChunkRequest | 接收方 → 发送方 | 请求若干块索引，窗口内最多 4 个 |
| ChunkData | 单向流 | 一个块一条流 |
| ChunkAck | 接收方 → 发送方 | 块写入并校验成功 |
| TransferProgress | 接收方 → 发送方 | 每 64 块或 1s 汇总一次 |
| TransferResume | 接收方 → 发送方 | 缺失块位图摘要与恢复请求 |
| TransferPause / TransferCancel / TransferComplete | 双向/接收方 | 状态与生命周期 |
| TransferError | 双向 | 稳定错误码与细节 |

文件夹按批次实现：TransferOffer 带 batch_id、root_name、entries[]（relative_path、size、mtime、is_dir；manifest_digest 在子传输 Manifest 中）。接收方接受后一次创建目录（含空目录），每个文件建立独立 TransferId/Manifest/checkpoint；UI 按 batch 聚合，暂停/取消批处理递归到子传输。

- 只接受相对路径；拒绝绝对路径、..、NUL、Windows ADS/保留名、超长路径、重复路径和符号链接；最终路径必须落在接收根目录内。
- 默认上限 10,000 个文件、总大小 128 GiB、目录深度 32；超出在 Offer 阶段拒绝。
- 同名根目录默认创建 root_name (1)，不合并覆盖；文件级同名按接收策略处理。
- 文件夹在 Offer 阶段只读元数据；接受后逐文件预哈希并发送 Manifest，避免 10k 文件阻塞首屏。

状态机：

~~~text
发送方: Created → Offered → AwaitingApproval → Preparing → Transferring → Completed
                    ↘ Rejected                         ↘ Failed / Canceled
接收方: Offered → AwaitingApproval → Preparing → Receiving ⇄ Paused → Verifying → Completed
                    ↘ Rejected                         ↘ Failed / Canceled
中断: Transferring → Interrupted → Resuming → Transferring
权限: Transferring → WaitingForFileAccess → Transferring
~~~

- WaitingForFileAccess 是显式状态：源/目标授权失效时发 FileAccessRequired，恢复后回到 Transferring，保留 checkpoint；不得伪装成失败或重新开始。
- 完成状态只能由 core 在块校验、整文件校验和最终 rename 后置位；GUI 不能直接把任务改成 Completed。

调度规则：

- 接收方持有位图，按窗口主动发 ChunkRequest；发送方只发送被请求的块。
- 单文件并发 4 条流；每块 4 MiB；全局同时活跃传输默认 2 个（可配置）。
- 每块 15s 超时，重试 3 次；仍失败则记为缺失，传输进入 Failed(MissingChunks)，保留检查点供续传。
- 接收方对每块校验长度与 BLAKE3；不一致则 reset 流并重新请求。
- 传输完成后对整文件重算 BLAKE3，与根哈希比对；通过后才 rename 为最终文件。
- 单文件在 Offer 前预计算 manifest；文件夹在 accept 后逐文件预哈希，避免 10k 文件阻塞首屏。并行哈希（rayon），UI 显示“准备中”与进度。10 GiB 在 NVMe 上目标 15s 内完成预计算。
- 根哈希 = BLAKE3(整个文件字节流)；manifestDigest = BLAKE3(按块索引顺序拼接的 32 字节块哈希)。单文件写入 Offer/检查点；文件夹写入各子传输的 Manifest/checkpoint。

检查点：

- 位于传输目录，独立于任何聊天数据。
- 每 2s 或累计 32 MiB 更新一次；暂停、取消、应用退出时强制更新。
- 原子写：同目录临时文件 → 写入 → fsync → rename；保留上一版本 .bak。
- 恢复匹配条件：transfer_id + manifest_digest + peer_id + 文件大小 + 分块大小全部一致；文件夹还要 batch_id/relative_path 一致，否则重新开始。
- 发送方同样保留最小恢复记录 outgoing/[transfer_id].json（sourceHandle、manifest_digest、大小、mtime），否则应用重启后无法知道该续传哪个文件。
- 恢复时接收方发送 TransferResume（含已收位图摘要）；发送方按 transfer_id 找到 outgoing 记录，校验源文件仍存在、大小/mtime 未变，并重算根哈希或缺失块边界哈希；不匹配返回 3002，不拼接不同版本。
- 源文件变化：发送前与发送后 stat 对比；若变化，返回 SourceChanged，UI 提示重新发送，不静默续传。

文件名与落盘：

- 对端文件名/相对路径只作为展示；落盘名由接收方生成，去除路径分隔符、控制字符、Windows 保留名与 ADS 冒号，限制 255 字节。
- 不覆盖已有文件；默认追加 “ (1)” 后缀，或在接受时让用户选择。
- 接收先写临时 .part，校验通过后 rename；移动端写入应用沙盒后再导出。

### 5.7 错误码

| 码 | 名称 | 场景 |
| --- | --- | --- |
| 1001 | VersionMismatch | 协议版本无交集 |
| 1002 | Unauthorized | 未信任或签名失败 |
| 1003 | BadFrame | 帧超限、Protobuf 非法、kind 未知 |
| 1004 | RateLimited | 配对或消息频率超限 |
| 2001 | PeerOffline | 对端不在线 |
| 2002 | PeerBusy | 并发传输超限 |
| 2003 | NetworkUnavailable | 无可用接口、绑定失败或所选地址消失 |
| 3001 | FileNotFound | 源文件不存在 |
| 3002 | SourceChanged | 源文件大小/mtime/内容变化 |
| 3003 | PermissionDenied | 文件权限失效 |
| 3004 | DiskFull | 接收端空间不足 |
| 3005 | HashMismatch | 块或整文件校验失败 |
| 3006 | MissingChunks | 重试后仍缺块 |
| 3007 | Timeout | 握手或块超时 |
| 3008 | Canceled | 用户取消 |
| 4001 | ClipboardRejected | 接收方拒绝 |
| 4002 | ClipboardTooLarge | 超过 1 MiB |
| 5001 | Internal | 未分类内部错误 |

错误码一经发布不得改变含义；新增码只能追加。

### 5.8 限制参数

| 参数 | 默认值 | 可配置 |
| --- | --- | --- |
| 控制帧上限 | 8 MiB | 否 |
| 聊天消息 | 32 KiB | 否 |
| 剪贴板文本 | 1 MiB | 是（上限 1 MiB） |
| 分块大小 | 4 MiB | 是（配对时协商） |
| 单文件并发流 | 4 | 是 |
| 全局活跃传输 | 2 | 是 |
| Manifest 上限 | 4 MiB | 否 |
| 单次多选文件数 | 100 | 是（上限 100） |
| 文件夹条目 | 10,000 | 是（上限 10,000） |
| 最大连接数 | 32 | 否 |
| 消息速率 | 50 条/秒/连接 | 否 |
| 配对窗口 | 120s | 否 |
| 握手超时 | 10s | 否 |
| 空闲超时 | 30s | 否 |
| 热点发现端口 | UDP 42420 | 否 |

---

## 6. 数据与存储

### 6.1 目录布局

| 内容 | 路径（相对应用数据目录） |
| --- | --- |
| 配置 | config.json |
| 信任设备 | trusted_devices.json |
| 身份种子 | 优先 OS 凭据库；降级为 identity.json（0600） |
| 传输检查点（接收方） | transfers/[transfer_id]/checkpoint.json |
| 文件夹批次 | batches/[batch_id].json（根名、空目录、子传输 ID） |
| 发送方恢复记录 | outgoing/[transfer_id].json |
| Manifest 缓存 | transfers/[transfer_id]/manifest.json |
| 日志 | logs/fluxy.log（滚动） |
| 接收临时文件 | 目标目录下的 [name].part |

应用数据目录使用 Tauri app_data_dir（标识 com.thep0y.fluxy）；CLI 使用同一目录规则，保证 CLI 与 GUI 可共享信任库。接收文件默认在系统下载目录下的 Fluxy 子目录，可在设置中修改。

平台差异：

- Windows：%APPDATA%/com.thep0y.fluxy
- macOS：~/Library/Application Support/com.thep0y.fluxy
- Linux：~/.local/share/com.thep0y.fluxy（遵循 XDG）
- Android/iOS：应用沙盒内 Application Support 等价目录

### 6.2 配置

~~~json
{
  "version": 1,
  "deviceName": "WZL-DESKTOP",
  "listenPort": 0,
  "network": {
    "mode": "auto",
    "interfaceId": null,
    "address": null
  },
  "downloadDir": "<系统下载目录>/Fluxy",
  "autoClipboardSend": false,
  "clipboardMaxBytes": 1048576,
  "notifications": true,
  "maxActiveTransfers": 2,
  "chunkSize": 4194304,
  "streamsPerFile": 4,
  "language": "zh-CN"
}
~~~

规则：未知字段保留（向前兼容）；version 不一致时执行迁移函数；写入使用原子替换。network 缺失时迁移为 auto；保存后立即重绑，活动传输先暂停并落检查点。

### 6.3 信任库

clipboard 取值 ask / auto / deny；files 在 v1 仅支持 ask / deny，auto 预留给后续版本。

~~~json
{
  "version": 1,
  "devices": [
    {
      "deviceId": "b3f1...",
      "name": "MacBook",
      "platform": "macos",
      "publicKey": "<base64 ed25519>",
      "certFingerprint": "sha256:<hex>",
      "pairedAt": "2026-10-21T03:00:00Z",
      "lastSeen": "2026-10-21T04:00:00Z",
      "policy": {
        "clipboard": "ask",
        "files": "ask"
      }
    }
  ]
}
~~~

### 6.4 恢复元数据

接收方检查点（位图是唯一真相，损坏时不得猜测）：

~~~json
{
  "version": 1,
  "transferId": "0192...",
  "peerId": "b3f1...",
  "batchId": null,
  "rootName": null,
  "relativePath": null,
  "fileName": "movie.mkv",
  "size": 11811160064,
  "chunkSize": 4194304,
  "chunks": 2816,
  "manifestDigest": "blake3:<hex>",
  "received": "<base64 bitmap>",
  "partPath": "movie.mkv.part",
  "source": { "size": 11811160064, "mtimeMs": 1760000000000 },
  "updatedAt": "2026-10-21T04:00:00Z"
}
~~~

发送方恢复记录（应用重启后据此找回源文件）：

~~~json
{
  "version": 1,
  "transferId": "0192...",
  "peerId": "b3f1...",
  "batchId": null,
  "rootName": null,
  "relativePath": null,
  "sourceHandle": "<FileHandleId>",
  "fileName": "movie.mkv",
  "size": 11811160064,
  "mtimeMs": 1760000000000,
  "manifestDigest": "blake3:<hex>",
  "updatedAt": "2026-10-21T04:00:00Z"
}
~~~

FileHandleId 只在本机有效，由 FileAccessManager 映射到路径/URI；恢复记录不含明文路径，句柄失效时续传返回 3003，提示用户重新选择源文件，不静默改为新传输。文件夹子传输必须在 checkpoint/outgoing 中带 batchId、rootName、relativePath；空目录和子传输列表写入 batches/[batch_id].json（收发双方均写），恢复时先读批次再逐子传输继续。

### 6.5 原子写与故障恢复

- 所有 JSON 写入走同一 write_atomic(path, bytes)：同目录临时文件、写入、sync_all、rename；Unix 额外 fsync 目录。
- 读取失败或 JSON 损坏时，尝试 .bak；仍失败则把坏文件改名为 .corrupt 并重建默认值，同时上报事件。
- 检查点损坏时不允许猜测位图，删除检查点并按新传输处理，已存在 .part 由用户决定是否删除。
- 日志不得记录消息正文、剪贴板内容、文件内容或完整本地路径；只记录 ID、大小、状态、错误码。

### 6.6 事件存储接口（预留）

core 定义 EventSink trait（append(event)），v1 不提供实现，仅用于未来企业版留存、审计与检索。任何 v1 代码不得依赖该接口。

---

## 7. 跨平台文件访问

### 7.1 抽象

~~~rust
pub struct FileHandleId(Uuid); // 只在本机有效，绝不上线传输

pub struct FileCapabilities {
    pub random_read: bool,
    pub random_write: bool,
    pub persistent_access: bool,
    pub size_known: bool,
}

pub enum FileReference {
    Path(PathBuf),
    Uri(String),
    Bookmark(Vec<u8>),
}

pub trait FileSource: Send + Sync {
    fn meta(&self) -> FileMeta;
    fn capabilities(&self) -> FileCapabilities;
    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes>;
    fn identity(&self) -> SourceIdentity;       // 用于源变化检测
}

pub trait FileSink: Send {
    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<()>;
    fn sync(&mut self) -> Result<()>;
    fn finalize(self: Box<Self>) -> Result<FinalPath>;
    fn abort(self: Box<Self>);
}

pub trait FileAccessManager: Send + Sync {
    fn register_source(&self, reference: FileReference) -> Result<FileHandleId>;
    fn open_source(&self, id: FileHandleId) -> Result<Box<dyn FileSource>>;
    fn create_sink(&self, name: &str, size: u64) -> Result<Box<dyn FileSink>>;
    fn export(&self, path: &Path) -> Result<()>; // 移动端导出/分享
}
~~~

实现要点：

- transfer crate 只接收 FileHandleId/FileSource，不接收平台原始路径或 URI；FileHandleId 与本地注册表的映射不进入协议。
- 桌面随机访问：每个 worker 独立打开文件句柄，seek + read_exact，在线程池中执行。
- Android SAF 的常规文件通过 fd + pread 随机访问；仅当 provider 不支持 seek/大小未知时降级为流式暂存。
- 流式来源（iOS 安全作用域、不支持 seek 的 provider）：顺序读取到临时文件后转为随机访问，或直接顺序发送并禁用暂停续传。
- 能力必须由实际句柄探测，不能按 URI 后缀或平台推断；接收端必须支持 write_at，若平台只能顺序写，则用临时文件 + 顺序追加实现。
- 文件夹由平台适配层遍历并逐个 register_source，生成 relative_path/空目录清单；符号链接不跟随，所有路径来自接收方本地策略。

### 7.2 平台映射

| 平台 | 选择文件 | 读取 | 写入 | 导出 |
| --- | --- | --- | --- | --- |
| Windows | 原生对话框，返回路径 | std::fs | std::fs | 直接可见 |
| macOS | 原生对话框 | std::fs（沙盒外需用户授权） | std::fs | 直接可见 |
| Linux | 原生对话框 | std::fs | std::fs | 直接可见 |
| Android | SAF 选择器，content:// | 自研 Kotlin 桥转 fd；常规文件 pread，异常 provider 流式 | 应用沙盒 | MediaStore/SAF 导出、系统分享 |
| iOS | 文档选择器，security-scoped URL | 获取访问权后复制到沙盒或直读 | 应用沙盒 | 分享面板 |

文件夹：桌面端支持递归选择与发送；Android/iOS v0.3.0 不提供文件夹选择，后续按 SAF/文档选择器能力评估。

### 7.3 路径安全检查清单

1. 拒绝包含 .. 段、绝对路径、盘符、UNC 前缀的对端文件名。
2. 过滤控制字符与 Windows 保留名（CON、PRN、AUX、NUL、COM1-9、LPT1-9）。
3. 拒绝 Windows ADS 冒号；限制 UTF-8 字节长度 ≤ 255。
4. 规范化后确认目标位于用户选定目录内；文件夹的每个相对路径都必须落在批次根目录内，符号链接不得逃逸且不跟随。
5. 接收端创建文件使用不跟随符号链接的打开方式；Unix 使用 O_NOFOLLOW。
6. 接受前检查剩余空间 ≥ 文件/文件夹总大小 × 1.05 + 64 MiB。
7. 已存在文件不覆盖，生成唯一名或让用户确认。
8. 权限失效、磁盘写满、rename 失败都要清理临时文件并回报稳定错误码。
9. 接收文件默认不自动打开或执行；只有用户显式操作才打开。

---

## 8. GUI 与交互计划

### 8.1 信息架构

桌面端采用左侧设备栏 + 主工作区：

| 区域 | 内容 |
| --- | --- |
| 左栏 | 本机身份、发现状态、已信任设备、附近设备、手动添加地址 |
| 主区标签 | 聊天 / 剪贴板 / 传输 |
| 底部状态栏 | 连接状态、当前吞吐、活动传输数、设置入口 |
| 全局弹层 | 配对 SAS、剪贴板接收确认、文件/文件夹接收确认、错误详情 |

移动端改为底部三个标签（设备、传输、设置），聊天进入设备详情页。

### 8.2 路由与页面

| 路由 | 页面 | 关键内容 |
| --- | --- | --- |
| / | 设备与会话 | 设备列表、选中设备聊天、空状态引导 |
| /transfers | 传输中心 | 进行中、已完成、失败；文件夹批次折叠/展开；暂停/继续/取消/重试 |
| /clipboard | 剪贴板 | 当前内容预览、手动共享、接收记录、自动发送开关、逐设备接收策略 |
| /settings | 设置 | 设备名、下载目录、网络接口、通知、语言、信任设备管理、诊断 |

### 8.3 Tauri 命令与事件

命令（全部返回 Result，错误使用稳定错误码）：

| 命令 | 作用 |
| --- | --- |
| engine_status | 返回版本、监听端口、当前绑定地址、连接数、发现状态 |
| state_snapshot | 返回 snapshot_version + peers/transfers/settings 快照；事件断档时重建 |
| network_list | 列出接口、地址、推荐项与当前绑定 |
| network_select | 设置 auto/manual 并重绑；返回新状态 |
| peers_list | 附近与已信任设备 |
| pair_start / pair_confirm / pair_cancel | 配对流程 |
| trust_list / trust_update / trust_remove | 信任与策略 |
| identity_rotate | 轮换本机身份密钥并标记旧信任失效；返回新指纹，需重新配对 |
| chat_send | 发送文本，可带 copy_hint，返回 msg_id |
| clipboard_share / clipboard_respond | 主动共享与接收确认 |
| register_file | 注册文件/文件夹条目，返回 FileHandleId；前端不持有路径 |
| start_transfer | 用 FileHandleId 列表发起单文件或文件夹批次 |
| transfer_respond / transfer_control | 接受/拒绝、暂停/继续/取消 |
| transfers_list | 传输快照 |
| settings_get / settings_set | 配置读写 |
| open_path | 用 FileHandleId 在文件管理器中显示；不接受对端路径 |

Tauri capabilities 按窗口最小化：主窗口仅允许命令白名单；文件路径/句柄不暴露给前端；命令参数再次校验所有权。

DESIGN 第 12.1 节命令为示意命名，对应关系：list_devices→peers_list，request_pairing→pair_start，confirm_pairing→pair_confirm，send_text→chat_send，send_clipboard_text→clipboard_share，pause/resume/cancel_transfer→transfer_control，get_state_snapshot→state_snapshot；register_file/start_transfer 同名。

事件（单一 Channel，tagged enum）：

- peer：发现、丢失、状态变化
- pairing：SAS 就绪、成功、失败、超时
- chat：收到消息、送达、失败
- clipboard：收到 Offer、已应用、应用失败、被拒绝
- transfer：准备、进度、暂停、完成、失败、FileAccessRequired
- trust：新增、更新、移除
- engine：启动、停止、错误
- error：AppError（稳定码、可恢复标志）

事件 envelope 带 seq（进程内单调递增）与 snapshot_version；前端发现 seq 跳号或版本不符时调用 state_snapshot 重建，不得只靠增量事件猜测状态。

进度事件节流到 100ms；终态事件不节流。前端断线重连 Channel 后先调用 state_snapshot 做全量对齐。

### 8.4 前端状态

- 单一 engineStore：连接状态、设备表、传输表、设置、snapshotVersion/eventSeq。
- chatStore：按 deviceId 分组的消息数组，消息状态为 sending/sent/delivered/failed。
- 所有写操作先本地乐观更新，收到事件后校正；失败保留重试按钮。
- 不在组件内直接 invoke；统一走 src/api 层，便于 mock 与测试。

### 8.5 组件与视觉

- 依赖：@neut/ui 0.0.17（精确版本）、tailwindcss 4、@tailwindcss/vite、lucide-solid、class-variance-authority、tailwind-merge、@solidjs/router 1.x。
- 包装层：src/ui 下封装 Button、Dialog、Toast、Tabs、Progress、Input、Tooltip，业务组件只用包装层。
- 视觉方向：工具型、信息密度中等、状态色克制；浅色/深色跟随系统；传输进度使用单一强调色，不用渐变堆叠。
- 交互要求：拖拽文件或文件夹到窗口即进入发送确认；文件夹按批次展示总进度与子文件，支持整批暂停/取消；错误提供“复制详情”。
- 网络设置：Auto/Manual、接口与地址列表、当前绑定地址、虚拟/VPN 标记、切换确认；诊断可复制接口和地址。
- 无障碍：所有对话框焦点陷阱与 Esc 关闭；图标按钮有 aria-label；进度用 aria-live=polite；支持键盘完成配对确认。
- 国际化：zh-CN 默认，en 备用；文案集中在 src/i18n，禁止硬编码在组件中。

### 8.6 移动适配

移动端不是桌面 UI 的等比缩小，而是复用组件与状态层的独立交互：

- 响应式断点 768px；移动端隐藏侧栏，使用底部导航“设备 / 传输 / 设置”。
- 文件选择走 SAF/文档选择器，接收后导出；Android 使用 dataSync 前台服务，iOS 后台主动暂停。
- Android 16 的本地网络权限、Android 15 的 6 小时前台服务上限、Android 13+ 的剪贴板限制都直接改变产品行为，必须在 UI 中如实表达。
- 自动剪贴板在移动端不提供；自动发现（含手机热点）、分享面板、通知操作是移动端的一等入口。

完整方案、fd 桥、权限、构建与测试矩阵见 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md)。

---

## 9. 工作分解（WBS）

估算单位为人日（1 人 1 天），已包含自测，不含跨阶段缓冲。

### 9.1 P0：工程基线（第 1 周，9 人日）

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| P0-01 | 版本与元数据统一 | 0.2.0、LICENSE、README、packageManager | 0.5 | 无 | 三处版本一致，bun 锁定 |
| P0-02 | Cargo workspace 与 crate 骨架 | 5 个 crate + 依赖规则 | 1 | P0-01 | cargo check 全绿 |
| P0-03 | CI 基线 | rust/web 两作业 + 缓存 | 1.5 | P0-02 | PR 上 CI 可跑 |
| P0-04 | 可观测性与错误基线 | tracing、AppError、配置加载 | 1 | P0-02 | 启动日志含版本与路径 |
| P0-05 | 协议与 ADR 初稿 | PROTOCOL.md、ADR-01..16 | 1 | 无 | 评审通过 |
| P0-06 | 移动端技术预研 | SAF fd、NsdManager、热点网关/AP 接口、FGS、Keystore 五项最小验证；结论写入 MOBILE_PLAN | 4 | 无 | 能读 content:// 文件、发现服务、热点探测、启动 FGS、重启身份不变 |
|  |  |  | 9 |  |  |

### 9.2 M0：CLI 通信原型与设备认证（第 2–5 周，31 人日）

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| M0-01 | 帧编解码与消息模型 | Protobuf schema、Envelope、round-trip、fuzz 入口 | 4 | P0-05 | 非法帧不 panic |
| M0-02 | 身份、密钥与信任库 | Ed25519、device_id、JSON 信任库、密钥轮换/重置身份 | 4 | P0-02 | 重启后身份稳定；轮换后旧信任失效 |
| M0-03 | QUIC 端点与证书钉扎 | 服务端/客户端、mTLS、指纹校验 | 5 | M0-01/02 | 指纹不符即拒绝 |
| M0-04 | 配对握手与 SAS | 双端 SAS、确认、超时、限流 | 3 | M0-03 | 错误 SAS 无法写入信任 |
| M0-05 | 控制流、ACK、重连、心跳 | 控制流复用、重试、退避重连 | 5 | M0-03 | 断网 30s 自动恢复 |
| M0-06 | mDNS 发现与 PeerTable | 广播/浏览、TXT 解析、去重、接口过滤验证 | 3 | M0-01 | 3s 内发现同网设备；选定接口只广播该接口 |
| M0-07 | CLI 命令集 | id/discover/pair/chat/serve、--dev-loopback | 4 | M0-04/05/06 | 两进程完成配对与聊天；回环不同端口也通过 |
| M0-08 | 故障注入与集成测试 | udp-chaos、双节点测试 | 2 | M0-05 | 丢包/延迟下可恢复 |
| M0-09 | 威胁模型与安全评审 | SECURITY.md、评审记录 | 1 | M0-04 | 高风险项有结论 |
|  |  |  | 31 |  |  |

M0 出口（第 5 周末）：

- 两台设备或两个进程能发现、配对、互发文本并收到 ACK。
- 未配对设备无法发消息；版本不匹配返回 1001；证书变化被拒绝。
- 断网 30 秒后自动重连，聊天继续可用。
- 1 小时浸泡无内存增长异常；协议文档冻结为 v1。

### 9.3 M1：桌面聊天与主动剪贴板（第 6–8 周，24 人日）

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| M1-01 | Tauri 适配层与 specta | Engine 生命周期、命令、Channel、state_snapshot/seq | 4 | M0-05 | 前端可收到真实事件 |
| M1-02 | UI 壳与设计基线 | 路由、布局、主题、包装组件 | 3 | P0-05 | 空状态/加载态/错误态齐全 |
| M1-03 | 设备列表与配对界面 | 附近设备、SAS 对话框、信任管理、发现诊断 | 2 | M1-01/02 | 配对全流程可点；无 QR/手动入口 |
| M1-04 | 聊天界面 | 会话列表、消息流、状态、重试 | 3 | M1-01 | 送达/失败状态正确 |
| M1-05 | 桌面剪贴板与主动共享 | 读写系统剪贴板、Offer/Text | 3 | M1-01 | 文本跨设备共享成功 |
| M1-06 | 信任策略与循环防护 | 询问/自动/拒绝、摘要去重 | 2 | M1-05 | 同内容不回传 |
| M1-07 | 通知与托盘 | 后台消息/接收提醒 | 1.5 | M1-01 | 失焦时通知可点回窗口 |
| M1-08 | 前端测试 | vitest + mock store | 1.5 | M1-02/04 | 关键组件有测试 |
| M1-10 | 热点/网关发现回退 | UDP 网关探测、有限广播、诊断 | 2 | M0-06 | 手机热点下桌面可发现并配对 |
| M1-09 | 端到端验收 | Windows↔Linux/macOS 脚本 | 2 | 全部 | 主路径稳定复现 |
|  |  |  | 24 |  |  |

M1 出口：

- 桌面端从发现到配对到聊天全流程可用，重启后信任关系保留，聊天清空。
- mDNS 可用时优先；手机热点下通过默认网关探测发现，桌面能作为客户端连接热点主机。
- 主动共享剪贴板需接收方确认；自动发送开关默认关闭，自动接收只对显式授权的设备生效。
- 通知、托盘、错误提示可用；不出现未处理 Promise rejection。

### 9.4 M2：可靠文件传输（第 9–12 周，35.5 人日）

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| M2-01 | 文件抽象与桌面实现 | FileSource/Sink/Manager | 3 | P0-02 | 单元测试覆盖读写边界 |
| M2-02 | Manifest 与预哈希 | 并行 BLAKE3、进度、缓存 | 3 | M2-01 | 10 GiB 预计算 ≤ 15s |
| M2-03 | 分块协议与调度器 | 拉取窗口、单向流、校验 | 6 | M2-02 | 4 流并发稳定 |
| M2-04 | 检查点与跨重启续传 | 两侧恢复元数据、原子写、恢复匹配、位图 | 5.5 | M2-03 | 任一端 kill -9 后继续 |
| M2-05 | 暂停/取消/重试 | 状态机与错误码 | 2.5 | M2-04 | 任意状态可取消且清理 |
| M2-06 | 校验、落盘与源变化 | 根哈希、rename、stat 检测 | 2.5 | M2-03 | 篡改块被拒绝 |
| M2-07 | 传输 UI | 列表、进度、速度、错误 | 3 | M1-02 | 100ms 节流不卡顿 |
| M2-08 | 拖拽与多文件队列 | 拖放、批量、并发上限 | 1.5 | M2-07 | 拖 10 个文件可排队 |
| M2-09 | 故障测试与 10 GiB 基准 | 断网/损坏/重启脚本 | 3 | M2-04 | 验收矩阵 T-01..T-06 绿 |
| M2-10 | 路径与空间安全 | 名称清洗、空间检查、覆盖策略 | 1.5 | M2-01 | 路径攻击用例全拒绝 |
| M2-11 | 多网卡/多 IP 选择与绑定 | 接口枚举、auto/manual、QUIC 绑定、平台接口绑定、mDNS 过滤、IPv6 scope、网络变化重绑 | 4 | M0-06、M1-01 | 多网卡机器可指定地址并连接；T-11 绿 |
|  |  |  | 35.5 |  |  |

M2 出口：

- 10 GiB 单文件在 Windows↔Linux 间传输成功且 BLAKE3 一致。
- 传输 50% 时断网 60 秒、杀进程、重启，均能从缺失块续传。
- 传输中修改源文件返回 3002；磁盘不足返回 3004；取消后无残留临时文件（检查点按策略保留）。
- 大文件传输期间聊天 p95 延迟 ≤ 250ms。
- 多网卡机器可指定接口/IP；绑定地址与 mDNS 广播一致；切换网络后传输可恢复。

### 9.5 M3：桌面完整度、文件夹与自动剪贴板（第 13–15 周，20 人日）

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| M3-01 | 设置、i18n、主题、无障碍 | 设置页、zh/en、键盘操作、重置身份入口 | 3 | M1-02 | 键盘可完成主流程 |
| M3-02 | 传输详情与通知完善 | 详情、重试、失败原因、通知 | 2 | M2-07 | 失败可一键重试 |
| M3-03 | 自动剪贴板监视 | 防抖、摘要环、逐设备策略 | 3 | M1-05/06 | 双向不会无限循环 |
| M3-04 | 托盘、开机启动、防火墙文档 | 托盘菜单、启动项、帮助 | 2 | M1-07 | 关闭窗口仍可接收 |
| M3-06 | 网络接口设置与诊断 | auto/manual、地址显示、切换确认、错误引导 | 2 | M1-01、M2-11 | 切换后重连；错误可行动 |
| M3-07 | 文件夹批量传输 | 相对路径、空目录、batch 元数据、批次 UI、路径安全、恢复 | 5 | M2-08 | 嵌套目录 10 GiB 可收发；T-13 绿 |
| M3-05 | 回归与缺陷修复 | 缺陷清单与修复 | 3 | 全部 | 阻塞缺陷清零 |
|  |  |  | 20 |  |  |

M3 出口：桌面功能完整，文件夹递归传输可用且路径安全；自动剪贴板默认关闭且可逐设备授权；发布候选的 UX 评审通过。

### 9.6 M4：Android 优先的移动端（并行轨道，38 人日）

DESIGN 的 M4 目标不变，但拆成并行轨道，不占桌面关键路径；任务 ID 与 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md) 一致。Android 工程师 W1 入职并参与 P0-06、W9 起全职到 W21；iOS 6 人日排在 W18 后。

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| A-01 | 构建、签名、16KB、CI | APK、16KB 校验、签名流水线 | 3 | P0-06 | APK 安装、16KB 校验通过 |
| A-02 | 插件骨架、Keystore、engine_status | tauri-plugin-fluxy-mobile、身份存储 | 3 | A-01 | 重启身份不变 |
| A-03 | SAF 发送 fd 桥与续传重开 | content:// → fd、持久 URI、流式降级 | 5 | A-02、M2-01 | 10 GiB 可读且可续 |
| A-04 | 接收沙盒、导出、清理 | .part、StatFs、MediaStore/SAF 导出 | 4 | A-02、M2-06 | 10 GiB 接收并导出 |
| A-05 | NsdManager、热点网关探测、LNP、诊断 | 发现、权限引导、网络诊断 | 4 | A-02 | Wi-Fi 3 秒发现；热点 5 秒发现；拒绝有引导 |
| A-06 | dataSync 前台服务与 6h 处理 | FGS、通知操作、onTimeout、锁 | 4 | A-04 | 锁屏 10 分钟；超时安全停止 |
| A-07 | 移动 UI、分享、剪贴板 | 底部导航、分享面板、手动剪贴板 | 4 | A-05 | 主流程可完成；无 QR/手动入口 |
| A-08 | beta 硬化、测试矩阵、APK 分发 | 10 GiB、进程死亡、权限撤销、热点 | 4 | A-01–A-07 | T-M01–T-M15 绿 |
| A-09 | iOS 桥与 beta | NWBrowser、安全作用域 URL、Keychain | 6 | A-02 | 真机主流程 |
| A-10 | 发布材料、隐私说明 | GitHub Release、隐私政策、发布说明 | 1 | A-01–A-08 | v0.3.0 APK 可下载、校验和正确 |
|  |  |  | 38 |  |  |

M4 出口：Android beta 于 W20 出 GitHub APK，W21 发布 v0.3.0；iOS beta W25。移动端出包不改变桌面 v0.2.0 的 W20 日期；若 W20 桌面 RC 与 Android beta 冲突，桌面优先，Android beta 可顺延 1–2 周。

### 9.7 M5：桌面性能、回归与发布（第 16–19 周，13.5 人日）

| ID | 任务 | 产出 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- | --- |
| M5-01 | 性能剖析与优化 | 火焰图、吞吐/内存优化、并发自适应参数 | 3 | M2 出口 | 达到第 11.3 节目标 |
| M5-02 | 跨平台回归矩阵 | Windows/macOS/Linux 组合 | 3 | M3 出口 | 矩阵全绿 |
| M5-03 | 安全审计、依赖与 fuzz | cargo-deny、fuzz 10 分钟、清单 | 2 | M2 出口 | 无高危，已知项有说明 |
| M5-04 | 打包、签名与安装器 | MSI/NSIS、DMG、AppImage/deb | 2 | M3 出口 | 干净机器可安装 |
| M5-05 | 文档与发布说明 | README、使用手册、已知问题 | 1.5 | 全部 | 发布清单完成 |
| M5-06 | 桌面 RC 浸泡与发布 | 24h 浸泡、标签、Release | 2 | 全部 | 无 P0/P1 缺陷；v0.2.0 发布 |
|  |  |  | 13.5 |  |  |

---

## 10. 里程碑与排期

### 10.1 总览

| 阶段 | 人日 | 日历 | 出口 | 里程碑标签 |
| --- | --- | --- | --- | --- |
| P0 | 9 | 第 1 周 | W1 | m0-baseline |
| M0 | 31 | 第 2–5 周 | W5 | m0 |
| M1 | 24 | 第 6–8 周 | W8 | m1 |
| M2 | 35.5 | 第 9–12 周 | W12 | m2 |
| M3 | 20 | 第 13–15 周 | W15 | m3 |
| M4 | 38 | 第 9–25 周（Android 并行；iOS 到 W25） | W20 Android beta / W21 Android 发布 / W25 iOS beta | m4-android-beta |
| M5 | 13.5 | 第 16–19 周（桌面） | W19 | v0.2.0-rc |
| 缓冲 | 12 | 第 20 周 | W20 | v0.2.0 |

桌面轨道 133 人日，2 人约 13.3 周净开发，排期到 W20；移动轨道 38 人日（Android 32 + iOS 6），1 名 Android 工程师 W1 入职并参与预研、W9 起全职到 W21，iOS 由 B 或 iOS 工程师在 W18 后投入。总净工作量 171 人日。

### 10.2 周计划与并行轨道

| 周 | A（Rust/网络） | B（前端/适配） | 出口 |
| --- | --- | --- | --- |
| 1 | P0-02/03/04 | P0-01/05 + Neut/Tailwind；P0-06 移动工程师并行 | 工程基线 |
| 2 | M0-01/02 | IPC 契约、事件 mock、设计 token | 协议可编解码 |
| 3 | M0-03 | UI 壳、设备列表（mock） | QUIC 回声 |
| 4 | M0-04/05 | 配对对话框、聊天页（mock） | 配对与 ACK |
| 5 | M0-06/07/08/09 | 前端单测、联调 | M0 出口 |
| 6 | M1-01 | M1-02/03 | 真实事件进 UI |
| 7 | M1-05/06 | M1-04/07 | 剪贴板共享 |
| 8 | M1-10 + M2-01 | M1-08 + M1-09 | M1 出口 |
| 9 | M2-02/11 | M2-07 | Manifest/接口选择 |
| 10 | M2-03/11 | M2-07/08 | 分块/绑定 |
| 11 | M2-04/05 | 传输详情 | 续传 |
| 12 | M2-06/09/10 | 验收脚本、修复 | M2 出口 |
| 13 | M3-03 | M3-01/02 | 自动剪贴板 |
| 14 | M3-04 | M3-05/06 | 网络设置 |
| 15 | M3-07 | M3-07 验收 | M3 出口 |
| 16 | M5-01/03 | M5-02/05 | 性能与安全 |
| 17 | M5-04 | M5-04 | 安装器 |
| 18 | M5-06 | M5-06 | 桌面 RC |
| 19 | 缓冲 | 缓冲 | 发布候选 |
| 20 | 发布 | 发布 | v0.2.0 |

### 10.3 移动轨道（并行，不占桌面关键路径）

| 周 | Android | iOS |
| --- | --- | --- |
| W1 | P0-06 技术预研 | — |
| W9–W10 | A-01 构建/16KB、A-02 插件/Keystore | — |
| W10–W13 | A-03 SAF fd 发送 | — |
| W13–W15 | A-04 接收/导出、A-05 发现/LNP | — |
| W15–W17 | A-06 FGS、A-07 UI/分享 | — |
| W18–W19 | A-08 硬化与测试矩阵 | A-09 开始 |
| W20 | Android beta（GitHub APK） | A-09 |
| W21 | GitHub Release、v0.3.0 | A-09 |
| W22–W25 | 维护、缺陷修复 | iOS beta |

### 10.4 关键路径

桌面关键路径：M0-03（QUIC 与钉扎）→ M0-04（配对）→ M1-01（适配层）→ M2-03（调度）→ M2-04（续传）→ M5-04（打包）。

移动关键路径：A-01（构建）→ A-03（SAF fd）→ A-04（接收/导出）→ A-05（发现/热点）→ A-06（FGS）→ A-08（硬化）→ GitHub Release。

关键路径上的任何延误都会直接推迟对应列车：

- M0-03 与 M0-04 安排最强的人力，且第 3 周结束必须有一个可演示的加密回声。
- 前端从第 2 周起基于 mock 事件开发，不等后端命令全部就绪。
- Android 从 W9 起做垂直切片（配对+聊天+100 MB 传输），不等桌面全部完成；P0-06 已提前验证 SAF/NsdManager/热点网关/FGS/Keystore。
- 协议与 FileAccess 端口在 W5/W12 冻结；之后变更必须同时评估桌面与移动两条列车。
- 每周五做一次 30 分钟联调演示，禁止“最后一周集成”。

---

## 11. 测试与验收

### 11.1 测试分层

| 层级 | 范围 | 工具 | 运行时机 |
| --- | --- | --- | --- |
| 单元 | Protobuf Envelope、位图、路径清洗、状态机 | cargo test / nextest | 每次提交 |
| 协议 | 未知字段/kind、长度上限、版本协商 | prost round-trip + proptest | 每次提交 |
| 属性测试 | 帧随机损坏、位图边界、名称 fuzz | proptest | 每次提交 |
| 集成 | 双节点连接、配对、聊天、传输 | tokio 测试 + 真实 UDP 回环 | 每次提交 |
| 故障注入 | 丢包、延迟、断连、重复、乱序 | tools/udp-chaos | 每日与发版前 |
| 端到端 | CLI 双进程；桌面 GUI 主路径 | 脚本 + 手工清单 | 每阶段出口 |
| 网络矩阵 | 多网卡、VPN、虚拟网卡、IPv6 链路本地、接口切换 | 真机 + 手工清单 | M2 起发版前 |
| 性能 | 吞吐、内存、CPU、哈希 | criterion + bench 子命令 | 每周与发版前 |
| 模糊 | 控制帧与文件头解析 | cargo-fuzz | 每夜 10 分钟 |
| 前端 | store、组件、交互 | vitest + @solidjs/testing-library | 每次提交 |
| 移动端 | SAF fd、FGS 生命周期、LNP、权限撤销 | 真机 + adb + 16KB 模拟器 | 移动阶段每日 |

### 11.2 DESIGN 第 10 节验收矩阵

| ID | 验收项 | 方法 | 通过标准 | 阶段 |
| --- | --- | --- | --- | --- |
| T-01 | 大于 10 GiB 文件 | 10 GiB 随机文件，三平台两两组合 | 完成、根哈希一致、无内存暴涨 | M2 |
| T-02 | 断网恢复 | 传输中切断 UDP 30s/120s | 自动续传，不重传已完成块 | M2 |
| T-03 | 异常退出恢复 | 30%/70% 时分别杀接收端/发送端，重启 | 检查点匹配，缺失块继续 | M2 |
| T-04 | 源文件变化 | 追加、截断、替换、改 mtime | 返回 3002，不产出坏文件 | M2 |
| T-05 | 权限失效 | 撤销 ACL、chmod、SAF 授权 | 返回 3003，进入 WaitingForFileAccess，保留 .part/checkpoint；恢复授权后继续 | M2/M4 |
| T-06 | 聊天与传输并行 | 满速传输时发 100 条消息 | 无丢失，p95 ≤ 250ms | M2 |
| T-07 | 跨平台校验一致 | 同一 fixture 三平台互传 | BLAKE3 一致；Unicode/emoji 名称正常 | M5 |
| T-08 | 配对与未授权 | 错误 SAS、未配对、伪造证书、撤销信任后重新配对、密钥轮换 | 全部拒绝并记录原因；重配对/轮换后仅新信任生效 | M0 |
| T-09 | 路径与资源攻击 | ../、绝对路径、ADS、超长名、超大 Manifest、文件夹相对路径/符号链接 | 全部拒绝且不落盘 | M2/M3 |
| T-10 | 剪贴板循环与默认关闭 | 双端自动同步、重复内容、未信任设备 | 不循环；默认不覆盖 | M3 |
| T-11 | 多网卡/多 IP 选择 | Windows 同时启用 Ethernet、Wi-Fi、Hyper-V、VPN；auto/manual 切换 | 绑定与广播均为所选地址；可达；错误提示可行动 | M2/M3 |
| T-12 | 接口消失/网络切换 | 传输中禁用 Wi-Fi、切换 VPN、拔网线 | 暂停并落检查点；auto 重选或 manual 返回 2003；可恢复 | M3 |
| T-13 | 文件夹递归传输 | 嵌套目录、空目录、Unicode、10k 文件、同名根目录、符号链接、路径穿越 | 结构/内容一致；非法路径/链接拒绝；同名新建；可续传 | M3 |
| T-14 | 热点发现与传输 | 手机 A 开热点、B 连接；关闭 mDNS；收发聊天/文件 | 5 秒内发现；网关探测生效；传输可完成 | M2 |
| T-15 | 本地回环双实例 | 两个 CLI 实例绑定 127.0.0.1 不同端口，--dev-loopback | 配对/聊天/传输通过；回环不广播；默认关闭时拒绝 | M0 |

移动端专项矩阵 T-M01–T-M15 见 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md) 第 11 节；其中 T-M01/02/03/04 直接覆盖 10 GiB、权限失效、异常退出。

### 11.3 性能目标

| 指标 | 目标 | 测量条件 |
| --- | --- | --- |
| 回环吞吐 | ≥ 1.0 GiB/s 聚合 | 2 进程、NVMe、4 流 |
| 1 GbE 吞吐 | ≥ 105 MB/s | 两台桌面、4 流 |
| 10 GiB 内存峰值 | ≤ 200 MB RSS | 接收端 |
| CPU | ≤ 1.5 核 @ 500 MB/s | 桌面中端 CPU |
| 聊天延迟 | 空闲 p95 ≤ 100ms；传输中 p95 ≤ 250ms | 局域网 |
| 恢复首块 | 重启后 ≤ 3s 发出首个缺失块请求 | 10 GiB 传输 50% |
| 发现时延 | ≤ 3s 出现在列表 | 同一交换机 |
| 冷启动 | ≤ 1.5s 显示主界面 | 桌面 |

移动端另设目标：中端机 Wi-Fi 5 下 ≥ 30 MB/s，10 GiB 前台传输可完成，锁屏 10 分钟不中断；详见 [docs/MOBILE_PLAN.md](MOBILE_PLAN.md)。

性能不达标时优先排查：分块大小、流窗口、文件读取方式、JSON 控制帧频率、进度事件节流。

### 11.4 安全验收清单

- 未配对设备不能发送聊天、剪贴板或文件。
- 首次配对必须双端 SAS 一致；错误 SAS 不写信任库。
- 已配对设备证书变化必须拒绝并提示重新配对。
- 所有对端文件名经过清洗；不存在路径穿越。
- 自动剪贴板默认关闭；启用后只对显式授权的设备生效。
- 日志不含正文与文件内容。
- cargo-deny 无高危许可证/漏洞；fuzz 10 分钟无 panic。
- 连接数、帧大小、消息速率、配对频率均有限制。
- 移动端身份种子必须 Keystore/Keychain 加密，不得明文落盘。
- Android fd 只能来自用户选择的 URI；对端不能提供路径/URI。
- 移动端剪贴板只在前台读写；传输前台服务只在用户发起传输时启动。

---

## 12. 工程规范与 CI/CD

### 12.1 工具链

- Rust stable（当前 1.99），edition 2024，workspace resolver = 3。
- Protobuf：prost/prost-build；protoc 由 protoc-bin-vendored 固定，CI 不依赖系统 protoc。
- Node 22 + bun 1.4.3（packageManager 锁定）。
- Biome 2 负责格式化与 lint；cargo fmt + clippy 负责 Rust。
- 命令统一：just 或 package.json scripts 二选一，推荐根目录 justfile 包装两侧命令。
- Android：JDK 17、SDK 36、NDK r28+、AGP 8.9.2+；仅移动作业需要，桌面构建不受影响。

### 12.2 提交与分支

- 主干 0.2.X 作为集成分支；功能分支 feat/m0-quinn、fix/transfer-resume。
- Conventional Commits：feat、fix、refactor、test、docs、chore。
- 每个 PR 关联任务 ID（如 M2-04），描述含验证步骤与截图（UI）。
- 里程碑打标签 m0、m1、m2、m3、v0.2.0-rc、v0.2.0、m4-android-beta、v0.3.0-android。
- 首次提交即建立基线，禁止把模板 greet 带入功能提交。

### 12.3 CI 作业

| 作业 | 内容 | 触发 |
| --- | --- | --- |
| rust | fmt、clippy -D warnings、protobuf 生成校验、nextest（win/mac/linux） | push/PR |
| web | biome ci、tsc --noEmit、vitest、vite build | push/PR |
| tauri-smoke | tauri build --debug（linux + windows） | main |
| security | cargo-deny、依赖缓存审计 | 每日 |
| nightly | fuzz 10 分钟、10 GiB 基准、24h 浸泡（周末） | 定时 |
| mobile | Android assembleDebug/APK、16KB 校验、iOS build（macOS runner） | W9 起，tag 触发 release |

缓存：Swatinem/rust-cache、bun install --frozen-lockfile。CI 目标 10 分钟内出结果；超过则拆分。

### 12.4 发布流程

1. 桌面 v0.2.0：冻结 0.2.X，跑桌面验收矩阵与 24h 浸泡；更新版本/CHANGELOG，打 v0.2.0-rc。
2. 三桌面平台签名安装包；干净虚拟机验证首次启动、防火墙、配对、10 GiB 传输。
3. 发布 GitHub Release v0.2.0；Android 轨道继续，不等待移动端。
4. Android v0.3.0：W20 出 beta GitHub APK，W21 修缺陷并发布 v0.3.0；不提交 Play。
5. iOS v0.3.x：TestFlight beta，稳定后另行发布。
6. 每次发布附协议版本与已知问题。

---

## 13. 风险登记册

| ID | 风险 | 概率/影响 | 触发信号 | 缓解措施 |
| --- | --- | --- | --- | --- |
| R1 | QUIC 无原生流优先级，大文件挤占控制流 | 中/高 | 传输时聊天延迟 > 500ms | 独立控制流、限制并发、必要时应用层限速或预留带宽 |
| R2 | mDNS 在 Windows/移动端/热点下不稳定 | 中/中 | 设备偶发不可见 | 热点默认网关探测、有限广播、最近地址缓存、移动端平台 API |
| R3 | Neut 0.0.x API 变动 | 中/中 | 升级即编译失败 | 精确锁版本、src/ui 包装层、升级独立 PR |
| R4 | Android SAF / iOS 文件访问受限 | 高/高 | P0 Spike 失败 | 提前预研、插件兜底、流式暂存方案 |
| R5 | 大文件预哈希耗时影响体验 | 中/中 | 10 GiB 准备 > 30s | 并行 BLAKE3、进度 UI、按 stat 缓存 |
| R6 | 检查点损坏或掉电 | 低/高 | 恢复后位图错误 | 原子写、.bak、校验失败即重传 |
| R7 | Windows 防火墙/杀软拦截 | 高/中 | 首次连接/热点发现失败 | 安装器加规则（QUIC + UDP 42420）、文档、诊断 |
| R8 | iOS 后台限制导致传输中断 | 高/中 | 切后台即断 | 主动暂停、回前台续传、明确文案 |
| R9 | 范围蔓延（聊天历史、图片剪贴板、同步盘） | 高/中 | 新需求进入 M2 | 非目标清单、变更需走评审 |
| R10 | 移动端硬件/签名资源不足 | 中/高 | M4 无真机 | 提前准备两台 Android 13+ 手机、16KB 设备与 release keystore |
| R11 | 10 GiB 测试环境不足 | 中/中 | 无足够磁盘/网络 | 提前准备测试机与虚拟盘 |
| R12 | 密钥存文件被窃取 | 低/高 | 无 OS 凭据库 | 优先 keyring，文件 0600 + 告警 |
| R13 | LNP 强制后权限被拒 | 中/高 | Android 16 设备无法发现/连接 | 首启引导、设置深链、诊断页；见 MOBILE_PLAN RM-1 |
| R14 | FGS 6 小时上限 | 中/中 | 长传输被系统停止 | onTimeout 落盘停止；仅传输时启动；回前台续传 |
| R15 | 16 KB/Tauri 工具链不兼容 | 中/高 | 16KB 设备崩溃 | P0 构建验证、NDK r28、链接参数 |
| R16 | 热点下 mDNS/广播被 AP 隔离 | 高/高 | 手机 B 发现不到手机 A | 默认网关 UDP 42420 优先、有限广播兜底、T-14；A-05/M1-10 |
| R17 | 多网卡/VPN 选错地址导致不可达 | 高/中 | 设备可见但连接超时 | auto 排序、manual 覆盖、平台接口绑定、诊断；T-11/T-12 |
| R18 | 切换接口导致活动连接中断 | 中/中 | 切换后传输失败 | 切换前暂停/落检查点；确认后重绑；自动重连 |
| R19 | 文件夹路径穿越/符号链接/海量小文件 | 中/高 | 非法相对路径或 10k 文件卡死 | 仅相对路径、拒绝符号链接、条目上限、批次 UI；T-13 |
| R20 | Protobuf schema 演进不兼容 | 中/中 | 旧端解析新字段失败 | 字段号只追加、未知 kind 拒绝、版本协商、round-trip/fuzz |
| R21 | GitHub APK 侧载/更新摩擦 | 中/中 | 用户安装失败或版本不一致 | SHA-256、版本检查、安装说明；不依赖 Play |

风险复盘固定在每周五，触发信号出现即升级，不等阶段结束。

---

## 14. 前 10 个工作日

| 天 | A（Rust） | B（前端） | 当日可演示 |
| --- | --- | --- | --- |
| D1 | P0-01/02：版本、workspace、crate 骨架 | 安装 Neut/Tailwind/路由，搭 UI 壳 | cargo check + 空页面 |
| D2 | P0-03：CI 两作业；P0-04 日志 | 设计 token、包装组件、mock 事件 | PR 上 CI 绿 |
| D3 | PROTOCOL 帧格式与 Protobuf schema | 设备列表与空状态 | schema 可生成、字段评审 |
| D4 | Envelope 编解码 + 异常用例 | 配对对话框（mock） | 非法帧不崩溃 |
| D5 | QUIC 回声（ALPN、自签证书） | 聊天页 mock、消息状态 | 两进程 echo |
| D6 | 身份密钥、device_id、信任库、密钥轮换 | 设置页骨架 | 重启身份不变；轮换后旧信任失效 |
| D7 | 证书钉扎与校验失败路径 | 真实 invoke 打通 engine_status | 指纹不符被拒 |
| D8 | 配对 SAS 与双端确认 | 配对流程接真实命令 | 双端显示同一 SAS |
| D9 | 控制流、ACK、重连 | 聊天接真实事件 | 断网后恢复 |
| D10 | 两进程演示 + 回顾 + M0 细化 | 联调与缺陷修复 | 发现→配对→聊天 |

第 10 天产出：一个可重复执行的 demo 脚本、M0 剩余任务看板、更新后的风险清单。

P0-06 由 Android 工程师在 W1 并行完成（4 人日），覆盖 SAF fd、NsdManager、热点网关/AP 接口、FGS、Keystore。

---

## 15. 决策与待确认问题

Q1/Q6/Q11/Q12/Q16 已按 2026-10-21 产品决策固化；Q13（移动端网络开关）已取消，网络范围是硬约束，不再作为设置项。其余为建议默认值，仍需确认。

| 编号 | 问题 | 建议默认值 | 决策截止 |
| --- | --- | --- | --- |
| Q1 | 移动端是否阻塞 v0.2.0 发布 | 不阻塞；Android 独立 v0.3.0，1 名工程师 W1 入职、W9 起全职 | W1 |
| Q2 | 桌面自动剪贴板是否 v1 必选 | 可选，默认关闭，M3 交付 | W1 |
| Q3 | 是否需要离线消息队列 | 不需要，失败即提示 | W1 |
| Q4 | 是否支持文件夹传输 | 支持；桌面 v0.2.0 文件夹批次，移动 v0.3.0 不支持 | W1 |
| Q5 | 身份密钥存储 | 优先 keyring，降级 0600 文件 | W2 |
| Q6 | 是否保留手动 IP 连接 | 桌面保留作为诊断；移动端 v0.3.0 只自动发现 | W1 |
| Q7 | 是否引入 just | 引入，统一双端命令 | W1 |
| Q8 | 产品名与标识 | Fluxy / com.thep0y.fluxy | W1 |
| Q9 | 自动更新与遥测 | v1 不做 | W1 |
| Q10 | 协议版本策略 | fluxy/1 冻结；不兼容改动升 v2 | W5 |
| Q11 | Android 发布渠道 | 仅 GitHub APK/Release；不上架 Play | W9 |
| Q12 | Android minSdk | 33（Android 13） | W1 |
| Q14 | 多网卡默认策略 | auto 选默认路由/首选私网，manual 可覆盖；一次只绑定一个地址 | W3 |
| Q15 | 文件夹同名与符号链接 | 同名根目录新建 root_name (1)；符号链接拒绝 | W13 |
| Q16 | 手机热点下如何发现对端 | 默认网关 UDP 42420 优先；失败后有限广播；不做 QR/手动输入 | W1 |

---

## 附录 A：控制帧示例

以下为 Protobuf Envelope 的 JSON 等价表示，仅用于说明字段；线上 payload 是 protobuf 字节，不是 JSON。后两段省略 Envelope 公共字段，只展示 kind 与 payload。

~~~json
{
  "protocol_version": 1,
  "message_id": "0192...",
  "source_device_id": "b3f1...",
  "session_id": "00000000-0000-0000-0000-000000000000",
  "created_at_ms": 1760000000000,
  "kind": "HELLO",
  "payload": {
    "protocol_min": 1, "protocol_max": 1,
    "device_id": "b3f1...", "name": "WZL-DESKTOP", "platform": "windows",
    "cert_fingerprint": "sha256:...", "nonce": "base64...",
    "signature": "base64..."
  }
}
~~~

~~~json
{
  "kind": "TRANSFER_OFFER",
  "payload": {
    "transfer_id": "0192...", "batch_id": "0192...",
    "relative_path": "photos/2026/a.jpg", "file_name": "a.jpg",
    "size": 11811160064, "mtime_ms": 1760000000000,
    "chunk_size": 4194304, "chunks": 2816,
    "root_hash": "blake3:...", "manifest_digest": "blake3:..."
  }
}
~~~

~~~json
{ "kind": "CHUNK_REQUEST", "payload": {
  "transfer_id": "0192...", "indices": [0, 1, 2, 3]
} }
~~~

## 附录 B：依赖清单（P0 锁定，Cargo.lock/bun.lock 为准）

Rust 建议：

- tokio 1（rt-multi-thread、sync、time、fs、net、io-util、macros）
- quinn 0.11、rustls 0.23、rcgen（自签证书）
- ed25519-dalek 2、rand、zeroize、base64
- blake3 1、rayon（预哈希）
- serde 1、serde_json 1、uuid 1（v7）、bytes 1、thiserror 2
- prost、prost-build（P0 锁定；protoc 由 protoc-bin-vendored 提供）
- tracing 0.1、tracing-subscriber 0.3、tracing-appender
- mdns-sd（P0 锁定支持接口过滤的版本）、if-addrs、futures-util 0.3、async-trait
- keyring（身份种子）、directories（CLI 路径）
- 开发：nextest、proptest、criterion、cargo-fuzz、cargo-deny

前端建议：

- solid-js ^1.9、@solidjs/router ^1
- @neut/ui 0.0.17（精确）、tailwindcss 4、@tailwindcss/vite 4
- lucide-solid、class-variance-authority、tailwind-merge
- @tauri-apps/api ^2、plugin-clipboard-manager、plugin-notification、plugin-dialog
- 开发：vitest、@solidjs/testing-library、jsdom、biome、typescript

移动端：

- 本地 plugins/tauri-plugin-fluxy-mobile（Kotlin/Swift）
- 不引入 QR/扫码插件；热点发现由 Rust GatewayProbe + 平台 local_network_info 完成
- tauri-plugin-android-fs（P0 评估；不满足 fd/持久 URI 则不用）

## 附录 C：任务卡模板

~~~text
ID：M2-04
标题：检查点与跨重启续传
背景：10 GiB 传输中断后需从缺失块继续，不能重传已完成块。
范围：检查点模型、原子写、恢复匹配、位图加载、TransferResume 流程。
不做：多设备共享检查点、云端同步。
验收：
  1. kill -9 后重启，3s 内发出首个缺失块请求。
  2. manifest 不匹配时拒绝续传并重新开始。
  3. 检查点损坏时安全回退，不产出坏文件。
测试：单测（位图/原子写）、集成（重启续传）、故障注入（掉电模拟）。
依赖：M2-03。
风险：Windows 上 rename 覆盖行为差异。
~~~

---

**执行原则：** 每个里程碑都必须先有 CLI 或自动化脚本证明引擎能力，再进入 GUI；任何绕过协议的临时实现都必须在合并前删除。
