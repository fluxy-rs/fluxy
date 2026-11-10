# Fluxy

简体中文 | [English](README.md)

Fluxy 是一个本地优先、仅限局域网使用的桌面与移动传输工具。它提供设备发现、
文字聊天、剪贴板共享，以及可续传的文件与文件夹传输，不需要账号、不经过云端
中继，聊天记录也不落盘。

本仓库包含 v0.2.0 桌面实现，以及 Android v0.3.0 这条线。

## 与 0.1.x 的对比

0.2 是传输路径的重写，不是换皮。0.1.x 是一个 Tauri 1 应用，用单个 Rust crate
通过明文 HTTP 把网页发给对端；0.2 把引擎拆成多个 crate，并把线缆换成了 QUIC。
两者不能互通：0.1.x 的 HTTP 上传与 0.2 的 QUIC 会话在传输层和分帧上都不一样，
所以两端必须在同一条线上。

### 0.2 更好的地方

- **引擎是库，不是窗口。** 0.1.x 的传输逻辑藏在 `src-tauri/src/server/` 的 Tauri
  命令后面，只有打开应用才能触达。0.2 把分帧放进 `fluxy-proto`，把传输与发现放进
  `fluxy-net`，把报价、检查点与路径规则放进 `fluxy-core`，把应用状态放进
  `fluxy-app`。`fluxy-cli` 可以无界面地驱动这套引擎，`tools/udp-chaos` 则在测试中
  向它注入丢包、乱序与停顿。

- **加密与认证是构造出来的。** 0.2 的会话是 QUIC over TLS 1.3，每台设备一张自签
  证书，配对时被钉住。发现记录只是提示：`pair` 由双方的设备 ID、nonce 和证书
  指纹导出六位码，只有双方都确认才写入信任，而证书、密钥或 ID 变过的设备会被
  拒绝。0.1.x 用的是不加密的 HTTP，靠二维码 URL 里的一次性 token 认证，拿到链接
  的人就拥有被扫码那台手机的同等权限，线上的字节也可读。

- **传输挺得过网络，而不只是挺得过连接。** 文件按 4 MiB 分块到达，每块用 BLAKE3
  校验，整个文件再对根哈希校验，接收方每块写一个检查点，于是暂停、崩溃或重启都
  能从缺口续上。0.1.x 每个文件一个 HTTP POST：连接一断，这个文件就得从零重发。

- **设备自己互相发现，而不是被塞一个地址。** mDNS（`_fluxy._udp.local.`）加 UDP
  广播与网关探测兜底，自动选网卡时会避开 VPN 与虚拟网卡，并每两秒复查一次。
  0.1.x 打印一个 URL 让手机去扫，地址一变就什么都连不上。

- **文件夹就是文件夹，路径先校验。** 一个批次带着整棵目录树（含空目录）过去，
  每个相对路径在写入第一个字节之前都会校验：不允许绝对路径、目录穿越、保留设备
  名、备用数据流、控制字符或双向字符、仅大小写不同的重名，符号链接一律拒绝而不
  跟随。0.1.x 是在浏览器里一次一个文件。

- **聊天与剪贴板共用同一个会话。** 文字消息与剪贴板内容走同一条已认证的连接，
  每台设备各自有接收策略，并保证从对端收到的内容不会再被发回去。0.1.x 两者都
  没有。

### 0.1.x 仍然更好的地方

- **对端什么都不用装。** 0.1.x 的手机流程是：在任意浏览器里扫码、选文件、上传。
  0.2 要求两台设备都装 Fluxy，而 Android 客户端仍在 0.3.0 这条线上，所以客人用
  手机给你发文件，得先装个应用。

- **它会自己更新。** 0.1.x 带了 Tauri 的 updater，有发布端点和 minisign 公钥，
  新版本会自己装进来。0.2 目前没有 updater 插件：升级意味着重新下载安装，而且
  安装包未签名。

- **它可以用 `curl` 戳。** 0.1.x 暴露的是 `GET /connect`、`GET /files`、
  `GET /download/<path>` 和 `POST /upload`，外面包一层 JSON，一条 shell 命令就能
  试，第三方客户端一个下午就能写出来。0.2 的线缆是承载 protobuf 的加密 QUIC：
  只能用 CLI、抓包或 chaos 工具，另写一个客户端意味着读
  [docs/PROTOCOL.md](docs/PROTOCOL.md) 并接上 TLS 栈。

- **它要构建、要审计的东西更小。** 一个 Tauri 1 + Salvo 的 crate，对比一个链接
  QUIC、rustls、prost、BLAKE3 并且要求 Rust 1.85 与 edition 2024 的 workspace。
  干净构建和 `cargo deny` 要审的依赖面都变大了。

- **它跑得更久，仪式感也更少。** 0.1.x 走过了十七个版本，而 0.2.0 还是 alpha，
  带着 [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md) 里的已知限制。配对也更严格：
  重装一台设备会换掉它的证书，于是必须重新配对，而不是悄悄重连。

0.1.x 这条线没有被删掉。它保留在
[fluxy-rs/fluxy-0.2](https://github.com/fluxy-rs/fluxy-0.2) 的 `0.1.X` 分支上，
发布也仍然在线，供需要浏览器流程的人使用。本仓库的 `main` 是 0.2，开发在这里
继续。

## 文档

| 文档 | 内容 |
| --- | --- |
| [用户指南](docs/USER_GUIDE.md) | 安装、配对、聊天、剪贴板、传输、设置与故障排查 |
| [已知问题](docs/KNOWN_ISSUES.md) | 这个版本还做不到什么，以及绕行办法 |
| [打包](docs/PACKAGING.md) | 安装包、签名、干净机器上的检查清单 |
| [发布矩阵](docs/RELEASE_MATRIX.md) | 哪项检查在哪个平台验证过，怎么复现 |
| [发布清单](docs/RELEASE_CHECKLIST.md) | 打标签前的检查项 |
| [0.2.0 发布说明](docs/RELEASE_NOTES_0.2.0.md) | 这一版交付了什么，会看到什么 |
| [防火墙指南](docs/FIREWALL.md) | 各平台规则与常见的连不通原因 |
| [安全模型](docs/SECURITY.md) | 威胁模型、控制措施与验证证据 |
| [安全审计](docs/SECURITY_AUDIT.md) | 依赖策略、公告状态、unsafe 清单、模糊测试 |
| [测试](docs/TESTING.md) | 每个测试套件证明什么，怎么跑 |
| [实现状态](docs/IMPLEMENTATION_STATUS.md) | 逐里程碑的记录与测试数量 |
| [协议](docs/PROTOCOL.md) | 帧、握手、传输协商、分块格式 |
| [设计](docs/DESIGN.md) | 产品与架构决策 |
| [开发计划](docs/DEVELOPMENT_PLAN.md) | 里程碑、估时与验收标准 |
| [移动端计划](docs/MOBILE_PLAN.md) | Android 这条线 |
| [OpenHarmony 计划](docs/OPENHARMONY_PLAN.md) | HarmonyOS NEXT 这条线（实验性，按分支启用） |

发布候选版本用 `bun run soak --minutes 60` 做浸泡测试：它先让 CLI 配对一次，然后
循环聊天与带哈希的文件传输，把每一轮记进 `.soak/log.jsonl`。
## 环境要求

- Rust 稳定版（edition 2024，MSRV 1.85）
- Bun 1.4+
- 对应操作系统的 Tauri 2 系统依赖

## 命令

```bash
bun install
bun run build          # 前端生产构建
bun run tauri:dev      # 桌面开发外壳
bun run tauri:build    # 桌面安装包
bun run check          # TypeScript 契约检查
bun run check:security # CSP/能力/危险汇点的回归检查
bun run test:e2e       # 跨平台 CLI 验收（配对、聊天、剪贴板、重启）

cargo check --workspace
cargo test --workspace
cargo run -p fluxy-cli -- status --data-dir ./.fluxy-dev
```

早期里程碑中，CLI 是引擎的参考消费者。它支持 `id`、`status`、`config`、
`discover`、`interfaces`、`serve`、`pair`、`chat`、`clipboard`，以及兼容用的旧
`connect` 组合命令。

## 设备发现

非回环的监听端会通过 mDNS 广播 `_fluxy._udp.local.` 并打印 `discovery=mdns`。
同一局域网内的另一台机器可以浏览到它：

```bash
cargo run -p fluxy-cli -- serve --listen 0.0.0.0:0 --data-dir ./.fluxy-a
# listen=0.0.0.0:xxxxx
# device=<server device UUID>
# fingerprint=<64 hex characters>
# discovery=mdns service=_fluxy._udp.local.

cargo run -p fluxy-cli -- discover --timeout 3
# peer device=<server device UUID> name=... platform=... addr=... port=xxxxx version=1
```

在 `serve` 或 `discover` 上加 `--interface <name>` 可以把组播限制在一张网卡上；
`discover` 加 `--json` 便于脚本解析，JSON 里带证书指纹可作 pin 提示。发现记录只是
提示，仍然需要 `pair`，QUIC/TLS 握手才是权威。

`serve` 省略 `--listen` 时，选择权交给网卡选择逻辑：`--interface`（可选再给
`--address`）钉住一块网卡和一个地址；否则自动选择会取承载默认路由的那块网卡，
优先真实网卡而不是 VPN 或虚拟网卡，并每两秒复查一次，从而跟上拔插网线或新连上的
Wi-Fi。平台支持时监听套接字会绑到选中的网卡上（Windows `IP_UNICAST_IF`、macOS
`IP_BOUND_IF`、Linux `SO_BINDTODEVICE`），serve 行会打印 `selection=`、`listen=`、
`interface_pinned=`，脚本可以据此看出发生了什么：

```bash
cargo run -p fluxy-cli -- interfaces          # 每块网卡，并标出自动选中的那块
cargo run -p fluxy-cli -- serve --interface "Wi-Fi" --data-dir ./.fluxy-a
cargo run -p fluxy-cli -- serve --address 192.168.1.20 --data-dir ./.fluxy-a
```

桌面应用里，同一个选择是一个持久化偏好（`config.json` 里的 `network`），由
**设置 → 网络接口** 通过 `set_network_selection` 驱动。面板会显示当前网卡、为什么
选它、套接字是否绑定、网络变化让监听端重绑过几次，然后列出每块网卡的类型、序号、
地址，以及推荐、默认路由、虚拟三种标记。切换模式或网卡会先要求确认，因为这会断开
当前会话并暂停进行中的传输（它们会从检查点续上）。手选的网卡消失时会报错 2003
并给出恢复步骤——换一块网卡，或者把网线插回去——应用绝不会自行降级到另一个地址。
复制按钮会把网卡列表与地址以文本形式写出，方便求助。

回环地址永不被广播。监听在回环地址上的服务会打印 `discovery=disabled-loopback`；
回环直连只在显式给出 `--dev-loopback` 时可用。

## 文件访问

传输层永远看不到平台路径。`fluxy-core::files` 定义了 `FileHandleId`、
`FileSource`、`FileSink` 与 `FileAccessManager`；桌面实现注册
`FileReference::Path` 来源，并把收到的文件写到接收根目录下的临时文件，只有每个
字节都覆盖到之后才对外可见。边界测试：

~~~text
cargo test -p fluxy-core files
~~~

`fluxy-core::transfer` 增加了传输前使用的清单模型：并行的 BLAKE3 窗口扫描产生
分块哈希、文件根哈希与清单摘要，绑定身份的磁盘缓存会在源未改变时跳过重算。跑
针对性测试，以及（默认 ignored 的）10 GiB 关卡：

~~~text
cargo test -p fluxy-core manifest
FLUXY_BENCH_FILE=/path/to/10gib.bin cargo test -p fluxy-core --release --lib benchmark_10gib_prehash_under_15s -- --ignored --nocapture
~~~

接收方随后以有界窗口拉取文件：`fluxy-core::transfer` 持有分块位图与拉取调度器
（16 个在途请求、15 秒重试期限、3 次尝试），`fluxy-net::transfer` 在传输自己的流
上把每个请求作为 `FCH1` 帧提供服务，并在写入前按清单校验每块哈希。每个传输都会
先用传输 id 为自己的流命名，因此多个文件可以在一条连接上并行而不会串流。回环测试
覆盖并发传输、乱序重复、同一连接上的重复传输，以及缺块失败路径：

~~~text
cargo test -p fluxy-core transfer
cargo test -p fluxy-net transfer
~~~

续传状态按传输存放在 `checkpoints/<transfer-id>/` 下：二进制的 `manifest.bin`，
以及带校验和的 `progress.bin`（记录角色、状态、源身份与已收分块位图）。两者都经
fsync 的临时文件加原子重命名写入，接收方通过 `resume_sink` 重新打开确定性的
`.fluxy-<transfer-id>.part` 文件，从而只拉缺失的分块。检查点损坏或不匹配会重开
传输，而不是把两个内容版本缝在一起：

~~~text
cargo test -p fluxy-core checkpoint
cargo test -p fluxy-net transfer::tests::sender_crash
cargo test -p fluxy-net transfer::tests::receiver_crash
cargo test -p fluxy-net transfer::tests::receiver_task_abort
~~~

传输还可以暂停、取消或重试。`fluxy-core::transfer` 持有强制的生命周期
（`StateMachine`）和一个可克隆的 `TransferControl` 句柄；接收循环每 100 ms 轮询
它，因此暂停只会冻结请求窗口而不消耗重试次数，取消则返回 3008 `Canceled`、把
状态置为 `cancelled`、丢弃 `.part` 文件并删除检查点：

~~~text
cargo test -p fluxy-core state
cargo test -p fluxy-core control
cargo test -p fluxy-net transfer::tests::pause_then_resume
cargo test -p fluxy-net transfer::tests::cancel_from_paused
~~~

完成的拉取在可见之前要校验。每块对照自身的负载哈希与清单块哈希检查，不匹配时按
块序号拒绝并重试，而不是写入。最后一块落地后，接收方对部分文件流式计算 BLAKE3
并与 `manifest.root_hash` 比较；只有匹配才把状态置为 `completed` 并把
`.fluxy-<transfer-id>.part` 重命名到位。不匹配以 3005 `HashMismatch` 失败、删除
检查点、丢弃部分文件；发送方在每次请求前重新 stat 源文件，因此传输中途被改动的
文件会以 3002 终止传输：

~~~text
cargo test -p fluxy-core files::desktop::tests::root_hash
cargo test -p fluxy-core scheduler::tests::a_rejected_chunk
cargo test -p fluxy-net transfer::tests::a_tampered_chunk
cargo test -p fluxy-net transfer::tests::a_source_changed
cargo test -p fluxy-net transfer::tests::a_root_hash_mismatch
~~~

传输中心（`/transfers`）是这一切面向用户的一面。它通过原生选择器和受信设备选择器
发送文件，在写入任何东西之前先对传入的报价询问，并显示每个任务的进度、大小、速度
与剩余时间，带暂停、继续、取消与重试。后端把进度节流到每个传输每 100 ms 最多一次
更新，所以再快的传输也不会淹没界面，而终态事件从不丢弃。

每个任务都有一个**详情**对话框：状态、方向、对端、进度、速度与剩余时间、创建与
更新时间；失败时还有本地化原因、协议码及其编号、核心是否认为可重试，以及原始消息。
原因由稳定的错误码解析（`src/lib/transfer-errors.ts`）；来自更新版本对端的未知码会
降级为中性提示，而不是原始键。只有核心判定可重试时才提供重试（源文件变了或哈希
不匹配会要求重新发送），并且直接从对话框里就能重试。复制详情会以文本给出同样的
字段，而文件系统路径永远不会离开 Rust：

~~~text
cargo test -p fluxy transfer::tests
bun run test:frontend
~~~

文件也可以直接拖到窗口上，或一次批量选取。每个文件都是独立传输，所以一个失败不会
挡住其余。每次发送最多排队 100 个文件，同时运行的传输数有上限（默认 2，可在传输
中心调整）——其余的在队列里等，暂停中的传输仍占着名额。已经到达上限的设备会用
2002 `PeerBusy` 拒绝新报价，而不是开一个它服务不了的传输。拖放本身在 Rust 里
处理，因此 WebView 收到的是不透明句柄与基础文件名，而不是本地路径：

~~~text
cargo test -p fluxy transfer::tests
cargo test -p fluxy-app
bun run test:frontend
~~~

路径、覆盖与空间策略的加固在 M2-10 落地。
## 本地开发

两个 CLI 实例可以在回环上配对。先启动监听端，抄下打印出来的设备 id、地址与指纹，
再从第二个终端配对：

```bash
cargo run -p fluxy-cli -- serve --listen 127.0.0.1:0 --dev-loopback --data-dir ./.fluxy-a
# listen=127.0.0.1:xxxxx
# device=<server device UUID>
# fingerprint=<64 hex characters>

cargo run -p fluxy-cli -- pair 127.0.0.1:xxxxx \
  --fingerprint <64 hex characters> --dev-loopback --data-dir ./.fluxy-b
```

两边会打印同一个六位 SAS，并在 stdin 上等 `yes`。证书按指纹钉住，双方的信任库
都只在 SAS 被接受之后才写入。配对完成后，可以在之后的进程里直接发聊天，不再弹
SAS：

```bash
cargo run -p fluxy-cli -- chat 127.0.0.1:xxxxx \
  --device <server device UUID> --message "hello" --dev-loopback --data-dir ./.fluxy-b
```

`chat` 在控制流上发送 UTF-8 文本（最多 32 KiB），并等待应用层的 `MESSAGE_ACK`。
旧的 `connect` 命令把配对或恢复与会选的 `--message` 组合在一起，用于兼容。

剪贴板共享是两步交换。发送方先给预览、大小与 BLAKE3 摘要，接收方接受之后文本才
会发出：

```bash
cargo run -p fluxy-cli -- clipboard 127.0.0.1:xxxxx \
  --device <server device UUID> --text "hello" --dev-loopback --data-dir ./.fluxy-b
```

`clipboard` 最多等接收方 90 秒做决定，并且只在最终 ACK 之后才打印
`clipboard-applied`。文本上限 1 MiB，且从不写入磁盘或日志。无界面的 `serve` 没有
征求同意的界面，会以 `cli_no_consent` 拒绝报价。

文件传输是在同一会话上由接收方驱动的拉取。`transfer-send` 报价一个文件，
`transfer-recv` 接受它并报告最终的根哈希：

```bash
cargo run -p fluxy-cli -- transfer-recv --listen 127.0.0.1:0 --dev-loopback --data-dir ./.fluxy-a
cargo run -p fluxy-cli -- transfer-send 127.0.0.1:xxxxx \
  --device <server device UUID> --file ./payload.bin --dev-loopback --data-dir ./.fluxy-b
```

两边都打印 `FLUXY_XFER key=value` 行，脚本可以据此跟踪进度并读取 `root_hash`、
`bytes_written` 与 `resumed`。中断后重试时传入同一个 `--transfer-id` 就会复用
检查点；`transfer-recv` 从报价里取得 id。

脚本化测试只需要一条连接时，给 `serve` 加 `--once`。TLS 证书与私钥材料存放在数据
目录下，因此服务端指纹与客户端证书 pin 在重启之间保持稳定。聊天只在内存里，任何
对话都不落盘。

回环绑定与回环目标都要求显式给出 `--dev-loopback`；回环地址永不通过 mDNS 广播，
也不是生产网络模式。

## 桌面外壳

`bun run tauri:dev` 启动 Tauri 外壳与 Solid 前端。外壳用哈希路由，因此打包后的
窗口可以在没有服务器兜底的情况下重载任意路由：`/`（设备与会话）、`/transfers`、
`/clipboard` 与 `/settings`。桌面布局是深色的仪器导轨、带分隔线的工作区，加一条
状态栏；宽度到 768px 及以下时导轨折叠为底部导航。

主题默认跟随系统，可从顶栏或设置里固定为浅色或深色。语言默认 `zh-CN`，可切到
英文；文档的 `lang` 与窗口标题会跟着变。界面文案只存在于 `src/i18n`，当两种语言的
条目不一致、某个占位符在翻译里消失，或组件里硬编码了文案时，目录测试会让构建失败。
设置里还可以改设备名（1-64 个字符；保存会重启局域网广播，让别的设备立刻看到新
名字），以及开关桌面通知。主流程只用键盘也能走完：跳转链接直达正文，导轨、配对
对话框、报价对话框与输入框都是原生控件，对话框会捕获焦点并用 Escape 关闭，关闭后
焦点回到打开它的控件。所有路由状态共用同一套空态、加载态与错误态原语，
`prefers-reduced-motion` 会关掉过渡动画。

附近的设备会自己出现在导轨与首页上。配对会打开一个对话框，显示双方设备名、两边
屏幕上相同的六位 SAS、120 秒的实时倒计时，以及一句明确的“比对两块屏幕”的警告。
确认按钮在勾选确认框之前一直禁用；Fluxy 从不自动确认。没有二维码，也没有手动输入
地址。配对成功即写入信任，设置里可以移除受信设备，或查看发现诊断（网卡、监听端、
mDNS 状态与附近设备数）。

发现优先走 mDNS。当手机热点或 Windows 热点过滤了组播时，桌面端还会向受限广播地址
和每个定向子网广播地址发送一个小的 UDP 通告，并以单播探测默认网关。这样找到的
对端仍然只是提示：正常的 TLS 证书 pin 与六位 SAS 确认才决定信任。设置里会显示兜底
是否生效、网关与目标、探测/回应计数，以及只靠兜底发现到的设备。

首页还包含仅驻内存的聊天工作区：受信设备列表、消息流、输入框，以及每条消息的
投递状态。发送会用选中的对端打开一次受信恢复；来自受信方的恢复则成为一次聊天会话。
消息在对端的应用层 ACK 到达之前保持**发送中**，会话先结束则变为**失败**并提供重试，
重试复用原始消息 id，接收方不会重复投递。会话最多 1000 条消息，永不持久化，退出或
身份轮换时清空。

剪贴板页列出在线的受信设备，只有点击**发送当前剪贴板**之后才会发送当前系统剪贴板。
发送方在那一刻读取剪贴板，先发预览/大小/摘要的报价，接收方接受之后才发正文。传入
的报价会显示对端、预览、大小与倒计时；在点击**接受**之前不会写入系统剪贴板。接受的
文本会对照报价摘要校验，历史只保留内存中的预览与状态。每台受信设备都有一份持久化
的接收策略：**询问**（默认）显示传入队列，**自动**需要按设备显式选中并跳过提示直接
写入，**拒绝**则静默回绝。

**自动发送**是另一个独立的开关，默认关闭。打开后后端每 300 ms 读一次系统剪贴板，
只有当内容在一个完整的防抖窗口内保持不变时才发出报价，因此半截的复制或一连串复制
只会产生一次报价，而不是一串。报价发往所有在线的受信设备，接收方仍各自套用自己的
接收策略。真正让它开着也安全的是环路规则：报价带来源 id 与跳数，接收方从不转发，
而本地剪贴板从对端应用过的摘要绝不会被再发出去——所以刚收到文本的设备不会把它回声
给发送方，哪怕它现在就在自己的剪贴板里。同样的内容不会被自动发送两次，只有文本会
同步，1 MiB 上限依旧生效。关掉开关会停止所有剪贴板读取，而打开开关会先记录剪贴板
上已有的内容而不是把它发出去，所以启用这个动作永远不会广播你刚刚复制的密码。
## 安装

发布构建按平台提供安装包：Windows 产出 NSIS 与 MSI，macOS 是 `.dmg`，Linux 是
AppImage 与 deb。Windows 安装包按用户安装（无需提权），可选英文或简体中文。

~~~text
bun x tauri build --bundles nsis     # target/release/bundle/nsis/*.exe
pwsh -NoProfile -File scripts/package/windows.ps1
~~~

安装包默认未签名；签名（Windows 证书、macOS 公证）与干净机器上的安装检查清单在
[docs/PACKAGING.md](docs/PACKAGING.md)。卸载会移除程序与托盘项，但保留
`%APPDATA%/com.thep0y.fluxy`，因此重装不会丢下设备身份和已有配对。

## 平台

Fluxy 面向 Windows、macOS 与 Linux。CI 矩阵在三个平台上跑格式检查、lint、完整
测试套件、解码器模糊测试与 CLI 验收场景，并按需跑 `tauri build --no-bundle`；
[docs/RELEASE_MATRIX.md](docs/RELEASE_MATRIX.md) 记录哪一行在哪个平台验证过、哪些
还需要每个平台的宿主机。

## 安全检查

`cargo deny` 强制许可证白名单、只允许 crates.io 来源的规则，以及禁止通配符依赖
（`deny.toml`）；解码器模糊测试在每次 `cargo test` 中运行，也可以推到上百万次变异；
`bun run audit` 把两者都跑一遍，外加 workspace 里五个 `unsafe` 块的清单。结果、两个
许可证决定，以及已知项——包括需要联网的公告检查——都在
[docs/SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md)。

## 性能

传输是测出来的，不是估出来的。可重复的基准在一条传输流上让两个端点经回环搬运
256 MiB，并把收到的文件写盘：

```bash
cargo test -p fluxy-net --release --lib benchmark_loopback_throughput -- --ignored --nocapture
```

在桌面 CPU 上大约是 240-265 MiB/s，足以覆盖 1 GbE 链路（105 MB/s），但低于计划里
1.0 GiB/s 的回环目标：瓶颈是一个 UDP 套接字的包速率与每块的控制往返，而不是带宽或
拷贝，所以要补上这个差距需要多个套接字或 GSO/GRO，而不是调参。流数、窗口大小、
MTU、套接字缓冲与每块拷贝都扫过一遍，没有一个能把数字推离噪声范围；
docs/TESTING.md 记录了这些测量。

文件夹可以作为一个批次发送：把它拖到窗口上，或用传输页的**发送文件夹**，确认框会在
任何东西移动之前报出根目录、文件数、空目录数与总大小。接收方在自己的接收目录下重建
这棵树——包括空文件夹——并拒绝任何会逃出该目录、是绝对路径、含穿越片段、控制字符
或双向字符、备用数据流、保留设备名、结尾点、空片段，或仅大小写不同的相对路径。批次
里的每个文件仍然是一次普通传输，各有自己的进度、检查点与重试，而传输页为每个批次
显示一行可折叠的记录，带批次级的暂停、继续与取消。符号链接一律拒绝而不跟随，一个
批次上限 10,000 项。

原生托盘图标可以还原隐藏或最小化的窗口；它的菜单——**打开 Fluxy**、**自动发送
剪贴板**与**开机启动**两个开关、**帮助与防火墙**，以及**退出**——跟随所选界面语言，
两个勾选项与持久化偏好保持一致。关闭窗口不再结束进程：关闭请求被拦截、窗口隐藏，
于是聊天、剪贴板报价与传输继续在托盘里到达，只有**退出**才停掉监听端。**开机启动**
注册一个按用户的登录项（Windows 的 Run 值、macOS 的 launchd plist、Linux 的 XDG
autostart 条目），以 `--hidden` 启动应用，因此登录启动会落到托盘里；如果偏好开着而
条目被别的什么删掉了，应用下次启动会把它修好。窗口失焦时，收到的聊天消息（以及在
**询问**策略下的待处理剪贴板报价，或**自动**策略下已应用的剪贴板）会弹一条系统通知，
只包含对端名字与事件类型，绝不包含消息或剪贴板正文。传输完成或失败也会弹一条，点名
文件与对端；用户主动取消则保持安静；一次立刻完成的大批发送会按设备合并成一条通知，
而不是每个文件一条。设置里可以全部静音。点击通知或托盘图标会还原并聚焦窗口。在
Windows 上，原生 toast 需要安装版构建；开发构建可能显示 PowerShell 的品牌信息。

由于应用只在局域网内工作，操作系统防火墙必须放行它的入站 UDP 流量：发现使用固定
UDP 端口，传输使用监听端绑定的端口（默认由系统分配）。应用内的**帮助与防火墙**页面
与 [docs/FIREWALL.md](docs/FIREWALL.md) 列出各平台规则，显示运行中的实例实际用了
哪些端口，并覆盖常见的“设备互相看得见却始终连不上”的情形。

Rust 适配层通过 tauri-specta 暴露 `engine_status`、`state_snapshot`、
`events_subscribe`、`identity_rotate`、`pair_start`、`pair_confirm`、`pair_cancel`、
`trust_remove`、`clipboard_set_policy`、`chat_open`、`chat_send`、`chat_retry`、
`clipboard_share`、`clipboard_accept`、`clipboard_reject`、`set_auto_clipboard`、
`set_autostart`、`set_network_selection`、`pick_folder`、`start_folder`、
`shell_set_labels`、`discovery_diagnostics`、`pick_file`、`pick_files`、
`register_file`、`start_transfer`、`start_transfers`、`set_transfer_concurrency`、
`set_device_name`、`set_notifications`、`transfer_respond`、`transfer_control` 与
`transfers_list`。UI 先加载一份快照，然后订阅单个事件 Channel；每个事件带进程级的
`seq` 与 `snapshot_version`，一旦出现缺口就重新 `state_snapshot`，而不是猜测增量。

生成的契约在 `src/bindings.ts`，不要手改。改动命令或 IPC 类型后重新生成：

```powershell
$env:FLUXY_EXPORT_BINDINGS='1'; cargo run -p fluxy
```

前端组件测试跑在 Vitest 与 jsdom 下，用带类型的 mock 引擎上下文，因此不需要 Tauri
IPC：`bun run test:frontend`。`bun run test` 先跑 Rust workspace 套件，再跑组件套件。
`scripts/e2e/` 里的 M1-09 验收脚本在 Windows、Linux 与 macOS 上跑配对、聊天、
剪贴板同意、重启/续传与拒绝路径；见 [scripts/e2e/README.md](scripts/e2e/README.md)。

## 安全

M0 威胁模型、高风险决策与发布关卡在 [docs/SECURITY.md](docs/SECURITY.md)。评审记录在
[docs/security/M0_REVIEW.md](docs/security/M0_REVIEW.md)。M1-03 的外壳运行在严格的
生产 CSP 下，主窗口能力只使用 `core:default`，没有 `opener` 插件，并启用了
`freezePrototype`；`bun run check:security` 守护这些设置。mDNS 里的证书指纹只是 pin
提示；首次配对的所有信任仍然来自双屏 SAS 比对。CLI 与外壳仍是开发原型：由操作系统
密钥库托管的密钥、全局连接与消息限额，以及文件路径防护仍是发布关卡。

## 故障注入

`tools/udp-chaos` 以一条有损链路转发 UDP，这样不必拔网线就能测恢复。它支持丢包、
固定延迟、抖动、重复、乱序，以及一次性中断：

```bash
cargo run -p udp-chaos -- \
  --listen 127.0.0.1:42430 \
  --target 127.0.0.1:42420 \
  --loss 0.05 \
  --delay-ms 20 \
  --outage-after-ms 3000 \
  --outage-duration-ms 500
```

让 `pair` 与 `chat` 指向代理地址，而服务端保持它真实的地址与指纹。代理从不创建信任，
也无法绕过 TLS/SAS。手工流程见 [docs/TESTING.md](docs/TESTING.md)，自动化的双节点
恢复套件用 `cargo test -p udp-chaos`。

### 验收矩阵与 10 GiB 基准

`scripts/acceptance/` 驱动真实的 CLI 覆盖计划里的 T-01..T-06 矩阵：配对、根哈希一致
的本地传输、链路中断后续传、接收端重启后续传、故障注入测试，以及带接收端内存上限的
基准。

```bash
node scripts/acceptance/acceptance.mjs local --size 64MiB
node scripts/acceptance/acceptance.mjs matrix              # T-01..T-06，T-05 为手工步骤
node scripts/acceptance/acceptance.mjs bench --size 10GiB --release
```

背后是 `transfer-send` 与 `transfer-recv` 两个 CLI 模式。T-05（文件访问被撤销）需要
改 ACL，是 [scripts/acceptance/README.md](scripts/acceptance/README.md) 里有记录的
手工步骤。

## 许可证

GNU Affero General Public License v3.0。见 [LICENSE](LICENSE)。

这是仍然算得上开源许可证里最严格的一个，选它只有一个理由：**改动必须保持开源**。
AGPL 第 13 条把 GPL 第 12 条扩展到网络使用，所以把改过的 Fluxy 当作服务运行、让
别人来连它，也算分发——必须以同样的条款提供源码。

**商业使用。** AGPL 并不禁止商业使用；禁止的话，这个项目就成了“源码可见”而不是
开源，因为开源定义要求许可证不得限制任何领域的使用。AGPL 禁止的是把改动闭源的那种
商业使用。对公司而言，这意味着在同一台服务器上以 AGPL 发布它做的每一处改动，所以
现实中没人会拿这份代码去做闭源产品。

如果你确实需要——保持改动私有，或把 Fluxy 装进一个闭源产品——单独的商业许可证是
唯一途径。到 <https://github.com/fluxy-rs/fluxy/issues> 开一个 issue 说明用途。

**贡献。** 修 bug 与加功能请放在这里，而不是 fork：对 `main` 的 pull request 更好
评审、更好延续，也让所有人待在同一份实现上。贡献该长什么样，见
[AGENTS.md](AGENTS.md#pull-requests-one-purpose-split-into-commits) 与
[AGENTS.md](AGENTS.md#commits-conventional-and-saying-why)。
