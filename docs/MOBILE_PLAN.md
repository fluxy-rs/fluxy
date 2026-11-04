# Fluxy 移动端方案（Android 优先）

| 项 | 内容 |
| --- | --- |
| 依据 | [docs/DESIGN.md](DESIGN.md)、[docs/DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) |
| 目标版本 | Android beta v0.3.0；桌面 v0.2.0 不受影响 |
| 平台范围 | Android 13–16（API 33–36）；iOS 随后复用同一组端口 |
| 状态 | Draft，待确认第 12.4 节资源与第 13 节开放问题 |

---

## 0. 结论摘要

- Android 是移动端第一优先，但走**独立发布列车**：桌面 v0.2.0 仍按 W20 发布，Android beta W20、v0.3.0 GitHub APK W21，不阻塞桌面。
- 复用 Tauri 2、SolidJS UI、Rust 引擎与 fluxy/1 协议；移动差异全部收敛到一层 Kotlin/Swift 平台插件。
- 文件访问不传路径、不只靠复制：SAF 选择后把 **fd** 交给 Rust，4 MiB 分块直接读；接收先写应用沙盒，校验后再导出。
- 发现用 Android **NsdManager**，并以**热点网关探测**兜底：手机 B 连手机 A 的热点后，向默认网关的 Fluxy 发现端口发探测包；不依赖 QR、手动 IP 或热点侧 mDNS。配对保留 6 位 SAS。
- 后台只做**传输前台服务**（dataSync），明确接受 Android 15 的 6 小时/24 小时上限；不做 7×24 后台在线，移动端剪贴板不做自动同步。
- 需要 1 名 Android/Kotlin 工程师，W1 入职并参与 P0-06，W9 起全职到 W21；不设“没有移动工程师”的备选排期。

---

## 1. 目标与范围

DESIGN 的 M4 目标（Android/iOS 文件访问与前台通信适配）不变；本方案只把 M4 从桌面关键路径拆出，改为独立发布列车。

### 1.1 v0.3.0 Android beta 必须交付

| 能力 | 移动端边界 |
| --- | --- |
| 发现 | NsdManager 浏览/注册；热点默认网关探测 + 有限广播兜底；LNP 权限引导 |
| 配对 | 6 位 SAS 双端确认；Keystore 保存身份；信任关系跨重启保留 |
| 聊天 | 纯文本、仅内存、前后台切换后不恢复；后台不保证收取 |
| 剪贴板 | 仅手动共享/接收；前台读写；通知预览后由用户点击复制 |
| 文件发送 | SAF 选择、fd 直读、10 GiB、暂停/取消/续传 |
| 文件接收 | 应用沙盒 .part、校验、导出到 Downloads/SAF；断点续传 |
| 后台传输 | dataSync 前台服务、进度通知、锁屏继续、6 小时上限处理 |
| 系统集成 | Android 分享面板收文本/文件；深链接只用于应用内跳转，不承载配对 |
| 构建 | targetSdk 36、minSdk 33、16 KB page size、分 ABI APK；不上架 Play |

### 1.2 明确不做

- Android 后台自动读取/覆盖剪贴板（系统限制，且隐私风险高）。
- 7×24 常驻前台服务维持在线；不做无通知的隐形后台连接。
- 文件夹递归传输、相册批量备份、自动同步盘。
- iOS Share Extension（v0.3.0 只做应用内分享面板）。
- 使用 Tauri 3 alpha；v0.3.0 固定在 Tauri 2.x 稳定线。
- FCM/APNs 推送、云端中继或账号体系；Fluxy 仍是纯局域网工具，离线消息不排队。

---

## 2. 平台约束与设计对策

| 约束 | 事实 | 对本项目的影响 | 决策 |
| --- | --- | --- | --- |
| 分区存储 / SAF | 外部文件通过 content:// URI 访问，不能假设真实路径 | Rust 不能直接 open 路径 | Kotlin 打开 fd，Rust 接管 fd；持久化 URI 授权用于续传 |
| 前台服务时限 | target 35+ 的 dataSync 服务在后台累计 6 小时/24 小时，超时回调 onTimeout，必须 stopSelf | 大文件慢网可能超过 6 小时 | 超时前落检查点并停止；回前台可继续；不规避系统限制 |
| 剪贴板 | Android 13+ 只有前台应用或默认输入法可读剪贴板 | 无法做后台自动同步 | 仅前台手动共享；接收用通知+显式复制 |
| 本地网络权限 | Android 16 引入 LNP，当前 opt-in，后续强制；覆盖 QUIC UDP、mDNS、NsdManager、网关探测、局域网 TCP | 权限被拒则完全无法发现/连接 | 首启引导 NEARBY_WIFI_DEVICES，预留 ACCESS_LOCAL_NETWORK；拒绝时给设置入口，网关探测也受同一权限约束 |
| 通知权限 | Android 13+ POST_NOTIFICATIONS 为运行时权限 | 前台服务通知可能不显示 | 首次传输前请求；拒绝时降级为应用内提示 |
| 后台限制 | Doze、厂商杀后台、进程死亡 | 连接会断、消息收不到 | 默认前台在线；传输用 FGS；状态全部可从磁盘恢复 |
| 16 KB page size | 面向 Android 15+ 的 native 库需 16 KB 对齐；部分设备/商店会校验 | Rust .so 可能不兼容 | NDK r28+、AGP 8.9.2+、链接参数、构建后校验 |
| targetSdk | 现代 Android 设备与 16 KB 工具链需要较新 target | 旧 target 可能触发兼容问题 | compileSdk/targetSdk 36，minSdk 33；不涉及 Play 审核 |
| Tauri 版本 | Tauri 3 仍在 alpha | 移动端生态与插件兼容性未稳 | 固定 Tauri 2.x 最新稳定版，P0 锁定具体版本 |

参考：[前台服务超时](https://developer.android.com/develop/background-work/services/fgs/timeout)、[Android 16 行为变更](https://developer.android.com/about/versions/16/behavior-changes-16)、[Android 本地网络权限](https://developer.android.com/privacy-and-security/local-network-permission)。

---

## 3. 架构

### 3.1 分层

~~~text
┌──────────────────────────────────────┐
│ SolidJS UI（与桌面共用，响应式布局）   │
└──────────────────┬───────────────────┘
                   │ Tauri IPC / Channel
┌──────────────────▼───────────────────┐
│ src-tauri + fluxy-app Engine          │
│ fluxy-core / fluxy-net / fluxy-proto  │
└──────────────────┬───────────────────┘
                   │ 插件命令与事件
┌──────────────────▼───────────────────┐
│ plugins/tauri-plugin-fluxy-mobile     │
│ Kotlin（Android）/ Swift（iOS）        │
│ FileBridge · Discovery · SecretStore  │
│ TransferService · Share · Hotspot     │
└──────────────────────────────────────┘
~~~

原则：

- fluxy-core、fluxy-net、fluxy-proto 不出现 Android/iOS 类型；移动能力全部通过端口注入。
- Tauri 适配层只做命令/事件转换与生命周期桥接。
- 平台插件放仓库内 plugins/tauri-plugin-fluxy-mobile，Android 与 iOS 共用一个插件包，避免依赖不稳定的第三方移动插件。
- tauri-plugin-android-fs 仅作为 P0 评估对象：若它能直接提供 fd 与持久 URI，可减少自研量；否则按本方案自研。

### 3.2 新增端口

| 端口 | 桌面实现 | Android 实现 | iOS 实现 |
| --- | --- | --- | --- |
| SecretStore | OS keyring，降级 0600 文件 | Keystore AES-GCM 加密种子文件 | Keychain |
| PlatformDiscovery | mdns-sd | NsdManager | NWBrowser/NetService |
| FileAccessManager | std::fs | SAF URI + fd | 安全作用域 URL/沙盒 |
| TransferKeepAlive | 无（进程常驻） | dataSync 前台服务 | beginBackgroundTask + 主动暂停 |
| ShareTarget | 无 | ACTION_SEND / SEND_MULTIPLE | UIActivityViewController |
| ClipboardPort | arboard/插件 | ClipboardManager（前台） | UIPasteboard（前台） |

### 3.3 插件命令

| 命令 | 作用 |
| --- | --- |
| fs_open_source(uri) | 校验并返回 fd、大小、名称、MIME、mtime |
| fs_reopen_source(uri) | 重启后续传时重新打开 fd |
| fs_export(path, name, mime) | 导出到 Downloads/SAF，返回最终 URI |
| discovery_start / stop | 注册并浏览 _fluxy._udp.；热点下 mDNS 失效时由 Rust GatewayProbe 兜底 |
| fgs_start / update / stop | 启动/更新/停止传输前台服务 |
| secret_get / set | Keystore/Keychain 读写身份种子 |
| share_take | 取走启动或前台时收到的分享内容 |
| clipboard_read / write | 前台剪贴板读写 |
| local_network_info | 返回默认网关、接口、AP/client 模式、传输类型；供 Rust GatewayProbe 使用 |

平台插件内部可返回 fd/URI，但 fluxy-app 的 FileAccessManager 再分配 FileHandleId；前端与 transfer crate 都不直接持有原始路径或 URI。

---

## 4. 文件访问

### 4.1 发送：SAF → fd → Rust

流程：

1. 用户在 Android 文件选择器中选择文件（支持多选）。
2. Kotlin 调用 takePersistableUriPermission，保存 URI 读权限。
3. Kotlin 查询 OpenableColumns.DISPLAY_NAME/SIZE，用 openFileDescriptor(uri, "r") 打开。
4. 调 detachFd() 把 fd 所有权交给 Rust，连同大小、名称、MIME、mtime 返回。
5. Rust 用 File::from_raw_fd 接管；每个读取 worker 用 try_clone 出独立 fd，用 pread/read_at 按偏移读，避免共享文件偏移竞争。
6. 传输结束或取消时 Rust 关闭所有 fd；Kotlin 不再持有。

~~~kotlin
val pfd = resolver.openFileDescriptor(uri, "r") ?: throw IOException("open failed")
val stat = pfd.statSize
val fd = pfd.detachFd()
// 返回 fd、stat、displayName、mime、lastModified
~~~

~~~rust
let file = unsafe { std::fs::File::from_raw_fd(fd) };
let meta = file.metadata()?;
// 每个 worker: let worker = file.try_clone()?;
~~~

约束：

- fd 只能来自用户明确选择的 URI；绝不接受对端提供的路径或 URI。
- 所有权约定：detachFd 成功后由 Rust 关闭；若 IPC 序列化或命令失败，Kotlin 在异常路径关闭 fd，Rust 不重复关闭。
- 校验 fstat 为普通文件，大小与 SAF 查询一致；不一致以 fstat 为准并记录。
- 不支持 seek 的 provider（管道/部分云盘）标记为 Streaming：顺序读到应用缓存临时文件后再按普通文件传输；该情况暂停/续传能力降级。
- 大小未知（-1）时先流式暂存得到真实大小；超过剩余空间直接拒绝。
- 重启续传：FileAccessManager 的本地注册表保存持久 URI（不进入协议/前端），outgoing 只保存 FileHandleId；回前台后重新 open + 校验大小/mtime。
- mtime 不可用的 provider 用“大小 + 每块 BLAKE3”检测源变化；无法检测时在 UI 明示。

### 4.2 接收：沙盒 → 校验 → 导出

流程：

1. 收到 FileOffer，用户在通知或应用内接受。
2. 默认目标：getExternalFilesDir(DIRECTORY_DOWNLOADS)/Fluxy，无需存储权限；不可用时退回 filesDir。
3. Kotlin 用 StatFs 检查剩余空间 ≥ 文件大小 × 1.05 + 64 MiB；不足返回 3004。
4. Rust 写 文件名.part，检查点记录位图；传输中锁屏由前台服务保护。
5. 完成后整文件 BLAKE3 校验，通过后 rename 为最终文件。
6. 导出：MediaStore.Downloads 创建 IS_PENDING=1 条目，复制完成后清除 pending；或让用户用 SAF ACTION_CREATE_DOCUMENT 选择位置。
7. 导出是独立任务，记录在 exports/[transfer_id].json；进程死亡后可在下次启动继续或重试。

权衡：

- 沙盒接收 + 导出会多一次读写；10 GiB 文件需要约 2 倍空间和额外时间。UI 必须显示“正在导出”进度，并提供“保留在应用内”选项。
- minSdk 33，MediaStore.Downloads/IS_PENDING 与 Scoped Storage 均可用；不保留 API 26–28 的 WRITE_EXTERNAL_STORAGE 分支。
- 不在传输中直接写 MediaStore：失败会留下半成品条目，且随机写不保证；沙盒方案故障面更小，符合 DESIGN 第 7 节。
- 导出完成后保留沙盒文件一周或用户手动清理；设置页提供“清理已导出文件”。

### 4.3 续传与异常

| 场景 | 行为 |
| --- | --- |
| 应用被杀 | 检查点保留；下次启动扫描未完成传输，回前台后重开 URI 继续 |
| SAF 授权被撤销 | 返回 3003，进入 WaitingForFileAccess，提示重新选择源文件；保留 .part 与检查点 |
| 磁盘不足 | 接受前 3004；传输中再次检查，失败时保留检查点 |
| 源文件被修改 | 大小/mtime 变化或块哈希不符返回 3002；不静默继续 |
| 导出失败 | 沙盒文件保留；导出任务可重试；不重新下载 |
| 卸载应用 | 沙盒与检查点删除；信任关系与身份随 Keystore 一起丢失，需重新配对 |

---

## 5. 发现、配对与本地网络权限

### 5.1 Android 发现

- 首选 NsdManager 注册 _fluxy._udp.，TXT 记录与桌面一致：v=1、id=[device_id]、name=[urlencoded]、os=android、port=[u16]、fp=[证书指纹前缀]。
- **热点兜底（必须实现）**：手机 B 连接手机 A 的热点后，B 从 local_network_info 取默认网关，向网关的固定 UDP 42420 发送 FLUX-DISCOVER/1 探测；A 在热点接口监听并回复 device_id、名称、平台、QUIC 端口、证书指纹。mDNS 在热点下常被 AP 隔离，不能作为唯一发现方式。
- 若默认网关不可用，再向 255.255.255.255:42420 发有限广播；探测 2s 超时、每秒最多 1 次，禁止全网段扫描和广播风暴。
- 热点主机 A 不由应用开关热点，用户在系统设置开启；A 用 local_network_info 找到 AP 接口并监听 42420。Android 可能不把 AP 暴露成 Network，P0-06 验证 ap0/wlan1/swlan0 与 0.0.0.0 绑定的收包路径。
- 移动端固定只允许 Wi-Fi/热点（含热点主机与客户端）；蜂窝、VPN 和其他接口直接判定为不可用并提示，不提供“允许蜂窝/其他网络”的设置开关。
- DiscoveryListener 维护服务表，按 device_id 去重；网络切换（Wi-Fi/热点）时重建浏览；mDNS 与网关探测结果合并。
- 发现只代表“可达”，不代表可信；探测包和应答不含信任结论，未配对设备仍需 SAS。
- Capabilities 必须声明 folder=false、剪贴板仅前台、单文件与分块上限；桌面不得向不支持的移动端发送文件夹。
- 不在后台常驻浏览；应用退到后台且无传输时停止，回前台恢复。
- Android 同时存在 Wi-Fi/蜂窝/VPN：v0.3.0 保证 Wi-Fi 与手机热点路径；检测到 VPN/蜂窝时明确警告。完整接口选择留到后续版本。
- 移动端 v0.3.0 不提供 QR、手动 IP 或隐藏地址输入；发现失败时只给网络/权限诊断，不要求用户输入参数。

### 5.2 本地网络权限（LNP）

Android 16 起 LNP 逐步强制，覆盖 QUIC 的 UDP、mDNS、NsdManager、局域网 TCP。设计：

1. Manifest 声明 NEARBY_WIFI_DEVICES（带 neverForLocation），并预留未来 ACCESS_LOCAL_NETWORK。
2. 首次进入设备页时解释用途并请求权限；拒绝后显示“去设置”按钮，不反复弹窗。
3. 权限被拒或系统返回 EPERM 时，发现与连接都走“不可用”状态，明确提示原因。
4. 自动发现、网关探测和热点传输同样受 LNP 限制；不要向用户暗示任何方式可以绕过权限。
5. CI/测试用 adb shell am compat enable RESTRICT_LOCAL_NETWORK 包名 打开限制，验证不崩溃、有引导。

~~~xml
<uses-permission
    android:name="android.permission.NEARBY_WIFI_DEVICES"
    android:usesPermissionFlags="neverForLocation" />
~~~

### 5.3 配对

- 复用桌面协议：mTLS + 证书钉扎 + Ed25519 身份 + 6 位 SAS。
- 移动端全屏展示 SAS，双端确认后写入信任库；SAS 不一致立即取消。
- 身份种子由 Keystore 加密保存，重启后 device_id 不变。
- v0.3.0 只通过自动发现进入配对，不提供二维码、手动 IP 或深链接配对；热点场景由网关探测解决。
- 发现失败时提示检查 Wi-Fi/热点、LNP 权限和 AP 隔离，不要求用户输入地址。

---

## 6. 后台执行与前台服务

### 6.1 连接策略

| 应用状态 | 连接 | 说明 |
| --- | --- | --- |
| 前台 | 保持 | 正常聊天/剪贴板/传输 |
| 后台，无活动传输 | 30 秒后断开 | 不常驻；回前台重连；期间消息不可达 |
| 后台，有活动传输 | 保持 | dataSync 前台服务 + 通知 |
| 进程死亡 | 断开 | 重启后从配置/检查点恢复，聊天不恢复 |
| 开机 | 不自动启动 | 不注册 BOOT_COMPLETED 连接 |

移动端不提供“后台常驻在线”开关。产品文案必须明确：**Fluxy 在 Android 上需要保持前台，或正在进行文件传输，才能接收消息。**

### 6.2 传输前台服务

- 服务类型 dataSync，Android 14+ 声明 FOREGROUND_SERVICE_DATA_SYNC；API 33 使用无类型 startForeground，API 34+ 使用 dataSync 类型。
- 只能由可见 Activity 中的用户操作启动；不在后台偷偷拉起。
- 通知：文件名、进度、速度、暂停/取消操作；点击回到传输页。
- 传输期间持有 PARTIAL_WAKE_LOCK 与高 perf WifiLock；暂停/完成立即释放。
- 完成或失败后 stopForeground + stopSelf，不残留常驻通知。
- 进程被杀后不自动重启服务；依赖检查点，用户回前台继续。

### 6.3 Android 15 的 6 小时上限

- 系统对 dataSync 在后台累计 6 小时/24 小时；超时回调 Service.onTimeout(startId, fgsType)。
- 收到回调后：立即落检查点、停止传输、stopSelf()、发通知“后台传输已达系统时限，点击继续”。
- 绝不在超时后继续持有前台服务，避免 RemoteServiceException。
- 用户回到前台后计时重置，可继续；UI 只在接近时限时提示。
- 测试用 adb shell device_config put activity_manager data_sync_fgs_timeout_duration 60000 缩短超时。

### 6.4 厂商后台限制

- 小米/华为/OPPO/vivo 等可能杀后台；提供“电池优化设置”引导页，但不自动申请豁免。
- 传输中优先保证 FGS + WakeLock；仍被杀时依靠检查点续传。
- 不把“后台收消息”作为卖点。

---

## 7. 剪贴板与系统分享

### 7.1 剪贴板

- Android 10+ 后台读剪贴板被禁止；Fluxy 只在用户点击“共享剪贴板”或粘贴到输入框时读取。
- 接收方收到 ClipboardOffer 后显示通知（预览前 160 字符）；点击打开应用，在预览页点“复制”才写入剪贴板。
- 不在通知回调里直接写剪贴板，避免后台写入与误覆盖。
- Android 12+ 写入剪贴板会显示系统提示，这是预期行为。
- 自动同步开关在 Android 上隐藏，并在设置页说明系统限制。

### 7.2 分享入口

- Manifest 注册 ACTION_SEND（text/plain、application/octet-stream）与 ACTION_SEND_MULTIPLE。
- 分享文本：进入“选择设备发送”页，可当普通聊天发送或标记为剪贴板内容。
- 分享文件：调用与 SAF 相同的 fd 流程；URI 读权限由分享 Intent 授予，需在进程内尽快打开。
- 应用未运行时的分享通过启动 Activity + onNewIntent 处理；插件把 payload 放入待处理队列，前端就绪后取走。

---

## 8. 身份、安全与权限

### 8.1 身份密钥

- Android：Keystore 生成不可导出的 AES-256-GCM 密钥，加密 Ed25519 种子；密文存应用私有目录；别名 fluxy_identity_v1。
- 关闭 Android 备份（android:allowBackup="false" 或备份规则排除密钥文件），避免种子被云备份。
- 有条件时使用 StrongBox；不可用则退回 TEE/软件实现，不阻塞。
- iOS：Keychain，kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly，不同步 iCloud。
- 卸载/清除数据会丢失身份，需重新配对；UI 在设置页说明。
- Keystore 密钥失效（锁屏方式变更、恢复出厂）时提示重新配对，不静默生成新身份。

### 8.2 权限清单

| 权限 | 用途 | 请求时机 |
| --- | --- | --- |
| INTERNET | QUIC/局域网 | 安装时 |
| NEARBY_WIFI_DEVICES | LNP/发现 | 首次进入设备页 |
| POST_NOTIFICATIONS | 传输/接收通知 | 首次传输前 |
| FOREGROUND_SERVICE / FOREGROUND_SERVICE_DATA_SYNC | 后台传输 | 安装时声明 |
| WAKE_LOCK | 传输期间保活 | 安装时声明 |
| ACCESS_WIFI_STATE / CHANGE_WIFI_MULTICAST_STATE | Wi-Fi 锁/发现（如需要） | 安装时声明 |
| ACCESS_NETWORK_STATE | 读取默认网关/接口，热点探测 | 安装时声明 |

### 8.3 fd 与路径安全

- fd 只从用户选择的 URI 打开；Rust 不拼接、不解析对端文件名到路径。
- 接收文件名仍按桌面规则清洗；落盘在固定沙盒目录。
- 导出目标由用户选择或 MediaStore 固定目录，不接受对端指定。
- 插件命令只接受应用内调用，不导出给 WebView 以外的第三方；Tauri capability 限制到主窗口。
- 日志不记录剪贴板内容、文件内容、URI 中的敏感参数。

---

## 9. UI / UX

- 底部导航：设备 / 传输 / 设置；聊天进入设备详情。
- 设备页：本机身份、附近设备、已信任设备、自动发现/热点诊断、LNP 权限状态。
- 传输页：进行中/已完成/失败；进度、速度、暂停/取消/重试；导出状态。
- 设置：设备名、下载/导出策略、通知、权限状态、信任设备、诊断日志。
- 首次启动引导：本地网络权限 → 通知权限 → 设备名 → 配对说明。
- 大按钮、48dp 触控区、安全区适配、返回键处理、深色模式。
- 错误提示必须可行动：权限被拒给设置入口；AP 隔离提示换网络/热点；空间不足给清理入口。
- 通知：传输进度可暂停/取消；剪贴板预览点击进入应用；文件 offer 点击接受/拒绝。

---

## 10. 构建、签名与发布

### 10.1 构建参数

| 项 | 值 |
| --- | --- |
| Tauri | 2.x 最新稳定版，P0 锁定；不使用 3 alpha |
| compileSdk / targetSdk | 36 |
| minSdk | 33（Android 13） |
| NDK | r28+ |
| AGP | 8.9.2+（需支持 compileSdk 36；P0 锁定具体版本） |
| JDK | 17 |
| Gradle | 8.11+（AGP 8.9 要求；P0 锁定） |
| ABI | arm64-v8a 必需；x86_64 用于模拟器；armeabi-v7a 可选 |
| 打包 | 分 ABI APK（GitHub Release）；不构建 AAB、不上架 Play |

生成工程：src-tauri/gen/android 纳入版本控制，manifest、Gradle、Info.plist 改动必须可复现；若选择不提交，则提供 init 后的 patch 脚本并在 CI 校验。

### 10.2 16 KB page size

- 使用 NDK r28+ 与 AGP 8.9.2+，useLegacyPackaging=false。
- 如 Tauri 未自动传递链接参数，在 src-tauri/build.rs 对 Android target 增加 -Wl,-z,max-page-size=16384 与 -Wl,-z,common-page-size=16384。
- 构建后校验：
  - llvm-objdump -p libfluxy_lib.so 的 LOAD 段 align 为 2 的 14 次方。
  - zipalign -c -P 16 4 app.apk。
- 在 Android 16 的 16 KB 模拟器上安装启动验证。

### 10.3 签名与 GitHub 分发

- 使用本地 release keystore；keystore 与密码放 CI secret，不入库；GitHub Release 附 SHA-256。
- 发布说明包含 dataSync 前台服务用途、本地网络/热点发现权限、隐私政策和已知问题。
- targetSdk 36 用于现代设备与 16 KB 工具链；不涉及 Play 审核。
- 分发方式：GitHub Release 提供按 ABI 拆分的 APK 与校验和；不构建 AAB，不提交 Play。
- 应用包名保持 com.thep0y.fluxy；与桌面共用图标与品牌。

### 10.4 CI

| 作业 | 内容 | 触发 |
| --- | --- | --- |
| android-debug | assembleDebug + 单元测试 | 移动相关 PR |
| android-release | APK + 签名 + 16KB 校验 + SHA-256 | tag |
| android-instrumented | 核心 SAF/FGS 测试（可选） | nightly |
| ios-build | xcodebuild + TestFlight 上传 | tag |

缓存 Android SDK/NDK、Gradle、Rust target；Android 构建耗时高，PR 阶段只跑 debug，release 在 tag 或手动触发。

### 10.5 iOS

- NSLocalNetworkUsageDescription + NSBonjourServices（_fluxy._udp）；首次浏览触发本地网络授权。
- 文件：安全作用域 URL → startAccessingSecurityScopedResource，读完后停止；大文件复制到沙盒或直接读。
- 后台：无前台服务；用 beginBackgroundTask 争取短暂收尾，随后主动暂停并落检查点。
- 分享：应用内 UIActivityViewController；Share Extension 留到后续。
- 签名与 TestFlight 需要 Apple Developer 账号与 macOS runner；排在 Android beta 之后。

---

## 11. 测试矩阵

| ID | 场景 | 方法 | 通过标准 |
| --- | --- | --- | --- |
| T-M01 | SAF 发送 10 GiB | 本地 NAS/桌面作为对端 | 完成、哈希一致、内存稳定 |
| T-M02 | 接收 10 GiB 并导出 | MediaStore 与 SAF 两种目标 | 导出成功、无残留 pending 条目 |
| T-M03 | SAF 权限撤销 | 传输中在系统设置撤销 URI 授权 | 返回 3003、保留检查点、提示重选 |
| T-M04 | 进程死亡 | 30%/70% 时 adb shell am kill | 重启后续传，不重传已完成块 |
| T-M05 | 后台 6 小时上限 | 缩短 timeout 到 60s | onTimeout 后落盘并停止，无崩溃 |
| T-M06 | 锁屏/Doze | 传输中锁屏 10 分钟、deviceidle force-idle | 前台服务存活或安全暂停 |
| T-M07 | LNP 拒绝 | am compat enable RESTRICT_LOCAL_NETWORK + 拒绝权限 | 不崩溃，显示权限引导 |
| T-M08 | 通知权限拒绝 | Android 13+ 拒绝 POST_NOTIFICATIONS | 传输仍可前台进行，有应用内提示 |
| T-M09 | 分享入口 | 从相册/文件管理器分享文本与文件 | 正确进入发送流程，URI 可读 |
| T-M10 | 热点默认网关发现 | 手机 A 开热点、B 连接；关闭 mDNS/广播 | B 5 秒内发现 A 并进入 SAS；不使用 QR/手动输入 |
| T-M11 | 剪贴板限制 | 应用退后台后尝试读取 | 不读取、不崩溃；前台手动共享正常 |
| T-M12 | 16 KB 页面 | 16 KB 模拟器/设备安装 | 启动、传输正常 |
| T-M13 | 厂商杀后台 | 小米/华为真机锁屏 30 分钟 | 可恢复；有电池设置引导 |
| T-M14 | 热点主机/客户端与 AP 隔离 | A 开热点、B 连接；路由器开启 AP isolation | 聊天/文件可用；网关探测生效；失败诊断明确 |
| T-M15 | Wi-Fi/热点 + VPN/蜂窝 | 开启 VPN 后启动发现；关闭 Wi-Fi 用蜂窝；Wi-Fi 与热点切换 | 明确提示当前网络不适用；不崩溃；Wi-Fi/热点恢复后可发现 |

ADB 辅助：

~~~bash
adb shell am compat enable RESTRICT_LOCAL_NETWORK com.thep0y.fluxy
adb shell am compat enable FGS_INTRODUCE_TIME_LIMITS com.thep0y.fluxy
adb shell device_config put activity_manager data_sync_fgs_timeout_duration 60000
adb shell am kill com.thep0y.fluxy
adb shell dumpsys deviceidle force-idle
~~~

设备覆盖：Pixel（Android 14/15/16）、三星 One UI、小米/华为各一台、低端 Android 13 设备、Android 16 16 KB 模拟器、平板各一台。

---

## 12. 路线图与资源

### 12.1 发布列车

| 列车 | 内容 | 时间 | 阻塞关系 |
| --- | --- | --- | --- |
| v0.2.0 桌面 | Windows/macOS/Linux 完整功能 | W20 | 不被移动端阻塞 |
| v0.3.0 Android beta | 本文第 1.1 节全部能力 | W20 beta，W21 GitHub APK | 依赖桌面协议冻结（W5） |
| v0.3.x iOS beta | 配对/聊天/剪贴板/文件 | W25 | 依赖 Android 插件抽象稳定 |

发布优先级：桌面 v0.2.0 > Android v0.3.0 > iOS beta；资源冲突时移动端让路。

### 12.2 周计划（Android 轨道）

| 周 | 任务 | 出口 |
| --- | --- | --- |
| W1 | P0-06 移动预研（SAF fd、NsdManager、热点网关/默认网关、FGS、Keystore） | 五项各有可运行的最小验证 |
| W9–W10 | A-01 构建/签名/16KB；A-02 插件骨架与密钥 | 真机可装，身份稳定 |
| W10–W13 | A-03 SAF 发送 fd 桥 | 10 GiB 源文件可读 |
| W13–W15 | A-04 接收沙盒与导出；A-05 发现/热点网关探测/LNP | 收/发/热点发现闭环 |
| W15–W17 | A-06 FGS 传输服务；A-07 UI/分享 | 锁屏传输、分享入口可用 |
| W18–W19 | A-08 beta 硬化与测试矩阵 | 10 GiB、进程死亡、权限撤销、热点通过 |
| W20 | Android beta（GitHub APK） | beta 发布 |
| W21 | GitHub Release、v0.3.0 | 正式发布 |
| W18–W25 | A-09 iOS 桥与 beta | TestFlight |

### 12.3 工作分解

| ID | 任务 | 人日 | 依赖 | 完成判据 |
| --- | --- | --- | --- | --- |
| A-01 | 构建、签名、16KB、CI | 3 | P0-06 | APK 安装、16KB 校验通过 |
| A-02 | 插件骨架、Keystore、engine_status | 3 | A-01 | 重启身份不变 |
| A-03 | SAF 发送 fd 桥与续传重开 | 5 | A-02、M2-01 | 10 GiB 可读且可续 |
| A-04 | 接收沙盒、导出、清理 | 4 | A-02、M2-06 | 10 GiB 接收并导出 |
| A-05 | NsdManager、热点网关探测、LNP、诊断 | 4 | A-02 | Wi-Fi 3 秒发现；热点 5 秒发现；拒绝有引导 |
| A-06 | dataSync 前台服务与 6h 处理 | 4 | A-04 | 锁屏 10 分钟；超时安全停止 |
| A-07 | 移动 UI、分享、剪贴板 | 4 | A-05 | 主流程可完成；无 QR/手动入口 |
| A-08 | beta 硬化、测试矩阵、APK 分发 | 4 | A-01–A-07 | T-M01–T-M15 绿；热点矩阵通过 |
| A-09 | iOS 桥与 beta | 6 | A-02 | 真机主流程 |
| A-10 | 传输吞吐优化（release 基线 30–40 MiB/s） | 3 | A-04 | 同链路 ≥55 MiB/s，1 GiB 单文件计时入库 |

**A-10 的基线**：两端 release、5 GHz Wi-Fi 6（600 Mbit 链路）实测 30–40 MiB/s，即链路的 40–53%；
debug 构建下只有 9.5 MiB/s，说明这一项的前提是先按 release 测量，否则会追错方向。
剩余瓶颈按证据排序：`cwnd` 增长（release 下到 4.4 MB，但 `lost_packets` 仍有两万余）、
`initial_mtu` 1452 的 PMTU 探测、平台 UDP 接收缓冲。已排除：文件读取（1–17 ms/4 MiB）、
manifest 哈希（缓存命中 2 ms）、分块大小（4 MiB）、流数量（8 已是协议上限）、拥塞控制（BBR 无差异）。
优先级为 P1 而非 P0：30–40 MiB/s 下 1 GiB 约 30 秒，主流程可用，不阻塞 beta；A-06 的前台服务
是缺失功能，比这项体验优化更紧急。
| A-10 | GitHub Release、隐私说明、发布说明 | 1 | A-01–A-08 | v0.3.0 APK 可下载、校验和正确 |
|  |  | 38 |  |  |

### 12.4 资源

- 1 名 Android/Kotlin 工程师 W1 入职并参与 P0-06，W9 起全职到 W21；iOS 6 人日由 B 或 iOS 工程师在 W18 后投入；桌面 2 人不受影响；QA 0.5 人从 M1 介入，W18 后主攻移动矩阵。
- 32 个 Android 人日不含 20% 平台不确定性；若 A-03/A-05/A-06 超支，用 W22–W23 吸收，v0.3.0 可顺延 1–2 周，不影响桌面。
- 需要两台 Android 13+ 手机验证热点主机/客户端、一台 Android 16 真机/模拟器、一台低端机、一台小米/华为设备、一台 macOS（iOS 阶段）。
- release keystore、隐私政策页面、GitHub Release 权限；不需要 Play 开发者账号。

---

## 13. 风险与开放问题

| ID | 风险 | 概率/影响 | 缓解 |
| --- | --- | --- | --- |
| RM-1 | LNP 强制后权限被拒，局域网完全不可用 | 中/高 | 首启引导、设置深链、诊断页；不承诺绕过 |
| RM-2 | 热点下 mDNS/广播被 AP 隔离 | 高/高 | 默认网关 UDP 探测优先；有限广播兜底；A-05/T-M14 验证 |
| RM-3 | 厂商杀后台导致传输中断 | 高/中 | FGS+WakeLock+检查点；电池设置引导 |
| RM-4 | SAF provider 不支持 seek/大小未知 | 中/中 | 流式暂存；明确降级；超空间拒绝 |
| RM-5 | 导出双倍 I/O 与空间 | 中/中 | 空间预检、保留在应用内、导出进度、清理策略 |
| RM-6 | 16 KB/Tauri 工具链不兼容 | 中/高 | P0 构建验证；build.rs 链接参数；NDK r28 |
| RM-7 | iOS 本地网络/后台限制 | 高/中 | 延后 iOS；前台为主；主动暂停 |
| RM-8 | 第三方 android-fs 插件 API 变动 | 中/低 | 自研薄插件；P0 评估后再决定 |
| RM-9 | 移动端 UI 与桌面共用导致复杂度 | 中/中 | 响应式布局 + 平台能力查询；不复制业务逻辑 |
| RM-10 | 10 GiB 移动端存储/电量 | 中/中 | 空间预检、前台提示、分块与限速可配 |
| RM-11 | Android Wi-Fi/VPN/蜂窝多网络路径 | 中/中 | NsdManager 走 VPN 时发现不到局域网；检测并提示关闭 VPN、切回 Wi-Fi/热点，不提供网络开关、不承诺自动选路 |
| RM-12 | 热点主机接口或默认网关不可见 | 中/高 | local_network_info 诊断；网关失败走有限广播；明确提示而不是静默失败 |
| RM-13 | GitHub APK 侧载/更新摩擦 | 中/中 | 版本检查、SHA-256、清晰安装说明；不依赖 Play |

已确定：仅 GitHub APK（不上架 Play）、minSdk 33、不使用 QR/手动地址、Android 工程师 W1 入职、支持手机热点主机/客户端、移动端固定只使用 Wi-Fi/热点（不提供网络开关）。

剩余开放问题：

1. iOS 是否与 Android 同版发布？（建议 Android 稳定后再投入 iOS beta）
2. Android 是否在 v0.3.0 做完整多网卡选择？（建议只做 Wi-Fi/热点/VPN 提示与诊断，完整选路后续）

---

## 14. 参考

- [Android 前台服务超时](https://developer.android.com/develop/background-work/services/fgs/timeout)
- [Android 16 行为变更（本地网络权限）](https://developer.android.com/about/versions/16/behavior-changes-16)
- [Android ConnectivityManager](https://developer.android.com/reference/android/net/ConnectivityManager)
- [Android LinkProperties](https://developer.android.com/reference/android/net/LinkProperties)
- [Tauri 移动插件开发](https://v2.tauri.app/develop/plugins/develop-mobile/)
- [tauri-plugin-android-fs](https://github.com/aiueo13/tauri-plugin-android-fs)
- [Tauri 16 KB page size 讨论](https://github.com/tauri-apps/tauri/issues/14895)
