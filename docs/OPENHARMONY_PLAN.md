# Fluxy 鸿蒙方案（OpenHarmony / HarmonyOS NEXT，实验性）

| 项 | 内容 |
| --- | --- |
| 依据 | [docs/DESIGN.md](DESIGN.md)、[docs/MOBILE_PLAN.md](MOBILE_PLAN.md) |
| 目标 | HarmonyOS NEXT 的 HAP；实验性，不承诺发布日期 |
| 平台范围 | OpenHarmony 5.x / HarmonyOS NEXT 5.x，arm64 真机；x86_64 仅用于 DevEco 模拟器 |
| 状态 | Draft；S0 spike 尚未开始，全部估算待校准 |
| 决策 | 走 Tauri `feat/open-harmony`，不迁移 Flutter；HarmonyOS 4.x 及更早继续用 Android APK |

---

## 0. 结论摘要

- **鸿蒙不是"再加一个 target"**：桌面与 Android 走的是 Tauri 2 稳定线，鸿蒙走的是上游一条**未合并的实验分支**。因此鸿蒙必须作为**独立轨道**存在，不能拖慢 0.2.X 的发布。
- **HarmonyOS 4.x 及更早不需要 HAP**：这些设备能装 APK，现有的 Android 产物已经覆盖。真正需要 HAP 的只有 **HarmonyOS NEXT（纯血鸿蒙）**，所以第一步是确认反馈用户的系统版本，而不是先写代码。
- **Rust 核心零改动复用**：`fluxy-core` / `fluxy-net` / `fluxy-proto` / `fluxy-app` 与本机工具链已经支持 `aarch64-unknown-linux-ohos`（见 1.4），鸿蒙的适配量集中在**宿主层**：ArkTS 工程、平台插件、构建脚本、条件编译。
- **风险集中在发现层**：`mdns-sd` 组播与 `default-net` 网关探测在鸿蒙上大概率不可直接用（无 `NETLINK_ROUTE`、无组播锁），需要与 Android 同构地改用 `@ohos.net.mdns`。这是 S0 的分水岭。
- **分发门槛与框架无关**：HAP 必须签名；不上市只能开发者模式侧载。上架华为应用市场需要开发者认证、软著/备案。选 Tauri 还是 Flutter 都绕不过去。

## 1. 事实核查（2026-10）

### 1.1 上游分支

| 事实 | 结论 | 来源 |
| --- | --- | --- |
| 分支名是 `feat/open-harmony`（连字符），同时存在于 `tauri`、`tao`、`wry` | 能跑通，但要同时钉三个仓库 | [tauri 分支](https://github.com/tauri-apps/tauri/tree/feat/open-harmony) |
| 尚未合入 `dev`；跟踪 issue 仍 open，标签 `help wanted` | 不能当稳定平台对待 | [issue #7287](https://github.com/tauri-apps/tauri/issues/7287) |
| 维护者在 2025-11 表示"2026 年一季度前合并不现实" | 不要等上游 | #7287 评论 |
| 分支头提交 2026-07-30，`dev` 已到 2026-10；"把 dev 合进分支"仍是未合并 PR | 分支本身在漂移 | [PR #16311](https://github.com/tauri-apps/tauri/pull/16311) |
| 分支上 PR 自述"实验性、非稳定平台发布请求"，且"单独不足以让生成的应用可用" | 需要 ArkTS 胶水与插件适配 | [PR #16171](https://github.com/tauri-apps/tauri/pull/16171) |

### 1.2 社区可复现路径

- [richerfu/tauri-demo](https://github.com/richerfu/tauri-demo)：一个能出未签名 HAP 的最小工程，配套 `ohrs` 与 `cargo tauri ohos` 用法（本文第 6 节照此整理）。它是"能不能跑通"的证据，不是"能不能发布"的证据。
- [TauriTavern 的鸿蒙开发说明](https://github.com/darkatse/tauritavern/blob/main/docs/OpenHarmonyDevelopment.md)：一个真实产品落地的记录，包含签名、插件 fork、ArkWeb 行为差异、`gen/ohos` 工程维护方式，以及明确标注"仅 x86_64 模拟器验收"的边界。

### 1.3 Tauri 核心的依赖风险

演示工程把依赖指向个人 fork（`richerfu/tauri` 的 `feat/open-harmony`），并用 `[patch.crates-io]` 钉住 `openharmony-ability`。这意味着：

- 供应链上多了一个非官方仓库；
- `openharmony-ability` 的 git rev 决定了 WebView 能力（例如某个 rev 之后移除了 webview feature）；
- 升级任何一环都要重跑 S0 验收。

**对策**：所有非 crates.io 依赖写进 `ohos/pins.json`（仓库, rev, 日期, 理由），分支打 `ohos-pin-YYYY.MM` tag，禁止在 manifest 里裸写 `branch =`。

### 1.4 本地工具链现状

`rustc --print cfg --target aarch64-unknown-linux-ohos` 的实际输出：

~~~text
target_os="linux"      # 注意：不是 "ohos"
target_env="ohos"      # 平台判定只能靠它
target_family="unix"
target_vendor="unknown"
~~~

并且 `rustup target list` 里 `aarch64-unknown-linux-ohos`、`armv7-unknown-linux-ohos`、`x86_64-unknown-linux-ohos` 均已安装。**结论：Rust 侧的可移植性是现成的，坑在条件编译与平台 API。**

## 2. 范围

### 2.1 必须交付（达到 Android beta 同等能力）

| 能力 | 鸿蒙实现 | 备注 |
| --- | --- | --- |
| 构建 | `cargo tauri ohos build` → 未签名 HAP | 签名走 DevEco 或自有证书 |
| 发现 | `@ohos.net.mdns` + Rust 网关探测兜底 | 见第 3 节 |
| 配对 | 6 位 SAS、证书固定 | 复用 `fluxy-net` 握手 |
| 聊天 | 纯文本、前台 | 与 Android 边界一致 |
| 剪贴板 | `@ohos.pasteboard`，仅前台手动 | 不做后台自动同步 |
| 文件发送 | picker URI → fd 读取（4 MiB 分块） | 与 Android 同构 |
| 文件接收 | 沙盒 `.part` → 校验 → 导出 | 与 Android 同构 |
| 后台传输 | 长时任务 + 进度通知 | 见第 3 节「后台执行」 |
| 系统集成 | 分享面板接收文本/文件 | 可选，M4 |

### 2.2 明确不做

- 不上架华为应用市场（首版只做开发者模式侧载与内测）。
- 不做 HarmonyOS PC 形态验收（同一 HAP 可运行，但不作为交付面）。
- 不做 7×24 后台在线；不做无通知的隐形连接。
- 不为鸿蒙改造 `fluxy-core` / `fluxy-net` 的协议或状态机。
- 不迁移 Flutter（理由见附录 A）。
- 不允许鸿蒙分支反向修改 0.2.X 的发布配置。

### 2.3 与 Android 轨道的关系

两条轨道共用：

- 同一套端口抽象（`SecretStore`/`PlatformDiscovery`/`FileAccessManager`/`TransferKeepAlive`/`ShareTarget`/`ClipboardPort`，见 [MOBILE_PLAN](MOBILE_PLAN.md) 3.2）；
- 同一个仓库内插件 `plugins/tauri-plugin-fluxy-mobile`（新增 `ohos/` 目录，与 `android/` 并列）；
- 同一份 RPC 与 UI。

区别只在平台实现层。**任何为了让鸿蒙跑通而改动的接口，必须同时定义 Android 的语义**，否则说明抽象漏了。

## 3. 平台约束与对策

| 约束 | 事实 | 影响 | 对策 |
| --- | --- | --- | --- |
| 平台三元组 | `target_os = "linux"`、`target_env = "ohos"` | 所有 `cfg(target_os = "android")` 判不中；`cfg(not(android))` 会命中桌面分支 | 新增 `cfg(target_env = "ohos")` 门控，见第 5 节 |
| Tauri 的 desktop 判定 | 稳定版 `tauri-build` 把鸿蒙当 desktop；分支把它当 mobile | `#[cfg(desktop)]` 的托盘代码会被编进来 | 依赖分支对 `tauri-build` 的修补；主线托盘代码已 `#[cfg(desktop)]`（lib.rs:14/92），无需改 |
| `tray-icon` / `muda` | `tauri` 用 `cfg(target_os = "linux")` 门控这两个依赖 | 鸿蒙上会去编 gtk3，直接失败 | 鸿蒙构建时去掉 `tray-icon` feature（见 5.1） |
| 网络接口枚举 | 普通应用不能绑 `NETLINK_ROUTE`；`default-net` 依赖 `/proc` | 网关探测可能拿不到接口 | ArkTS 提供 `local_network_info`，Rust 侧探测降级为兜底 |
| 组播 | 没有公开的组播锁接口；mDNS 行为依设备/网络而异 | 发现可能只单向可用 | 首选 `@ohos.net.mdns`；保留手动地址与网关探测 |
| ArkWeb DOM Storage | 默认关闭 | `localStorage` 为 null，前端白屏 | 确认 WebView 宿主打开 `domStorageAccess` 与 `databaseAccess` |
| ArkWeb 初始化脚本 | 多条目按字典序执行，不按数组顺序 | Tauri 引导脚本可能先于宿主脚本 | 依赖分支改用 `runJavaScriptOnDocumentStart`；升级时复验 |
| 文件 URI 授权 | 选择器返回的 URI 授权保存在 Ability 内存，进程结束失效 | 重启后续传需要重新选择文件 | 与 Android 一致：续传前重新授权；能持久化时优先持久化 |
| 后台执行 | 无 Android FGS 的等价物，靠长时任务与系统调度 | 大文件后台可能被挂起 | 长时任务 + 进度通知 + 检查点；超时前落盘停止 |
| 签名与分发 | 未签名 HAP 无法给普通用户安装 | 无法"下载即用" | 首版内测；上架评估放在 M4 之后 |

### 3.1 进程与生命周期

`RustAbility` 转发 Ability 生命周期（演示工程已实现）。需要在 S0 确认：切后台、锁屏、进程被杀、重新进入时，`fluxy-app` 的引擎状态如何重建。我们的引擎本身以磁盘状态为准（检查点 + 恢复），这一点与桌面一致，预期不需要新状态机。

### 3.2 与 Android 的差异清单（写代码前先读）

1. 没有 `NsdManager`，用 `@ohos.net.mdns`；
2. 没有 SAF，用 picker 返回的 URI + fd；
3. 没有 `Keystore` 的等价物命名，用 HUKS（`@ohos.security.huks`）；
4. 没有 `dataSync` 前台服务，用长时任务；
5. 目标三元组不同（`target_env` 而不是 `target_os`）。

## 4. 架构

~~~text
┌────────────────────────────────────────────┐
│ SolidJS UI（与桌面、Android 共用）           │
└──────────────────┬─────────────────────────┘
                   │ Tauri IPC / Channel
┌──────────────────▼─────────────────────────┐
│ src-tauri + fluxy-app Engine               │
│ fluxy-core / fluxy-net / fluxy-proto       │
└──────────────────┬─────────────────────────┘
                   │ 插件命令与事件（N-API）
┌──────────────────▼─────────────────────────┐
│ plugins/tauri-plugin-fluxy-mobile/ohos     │
│ ArkTS：Mdns · Picker/FD · Pasteboard       │
│ HUKS · LongTask · Share                    │
└────────────────────────────────────────────┘
~~~

端口实现对照（在 [MOBILE_PLAN](MOBILE_PLAN.md) 3.2 基础上增加一列）：

| 端口 | 桌面 | Android | 鸿蒙 |
| --- | --- | --- | --- |
| SecretStore | OS keyring | Keystore AES-GCM | HUKS |
| PlatformDiscovery | mdns-sd | NsdManager | `@ohos.net.mdns` |
| FileAccessManager | std::fs | SAF + fd | picker URI + fd |
| TransferKeepAlive | 无 | dataSync FGS | 长时任务 |
| ShareTarget | 无 | ACTION_SEND | 系统分享接收 |
| ClipboardPort | arboard | ClipboardManager | `@ohos.pasteboard` |

## 5. 代码改动清单

"归属"一列决定它落在主线还是 `feat/open-harmony` 分支（见第 7 节）。

### 5.1 依赖与 feature

| 文件 | 改动 | 归属 |
| --- | --- | --- |
| `src-tauri/Cargo.toml` | `arboard` 的 target 从 `cfg(not(target_os = "android"))` 扩为 `cfg(all(not(target_os = "android"), not(target_env = "ohos")))` | 主线 |
| `src-tauri/Cargo.toml` | 把 `tauri` 的 `tray-icon` 收进本地 feature（如 `tray`），鸿蒙构建用 `--no-default-features` 关闭；不改工程结构的话，由分支的 `prepare` 脚本剥离该 feature | 主线（推荐）/分支（兜底） |
| `src-tauri/Cargo.toml` | 增加 `napi-ohos`、`napi-derive-ohos`（分支的 `mobile_entry_point` 与 `#[ability]` 宏会展开 `napi_ohos::` 路径） | 分支 |
| `src-tauri/Cargo.toml` | `[patch.crates-io]`：`tauri`、`tauri-build`、`tauri-runtime`、`tauri-runtime-wry`、`tauri-utils`、`tauri-macros`、`openharmony-ability` | 分支 |
| `plugins/tauri-plugin-fluxy-mobile/Cargo.toml` | `[target.'cfg(target_env = "ohos")'.dependencies]` 段；保留现有 `cfg(any(android, ios))` 段 | 主线 |

### 5.2 条件编译

| 文件 | 位置 | 改动 | 归属 |
| --- | --- | --- | --- |
| `src-tauri/src/clipboard.rs` | 58、122、143 | `cfg(not(target_os = "android"))` → 同时排除 `target_env = "ohos"`，走"平台不支持"分支，直到 ArkTS 剪贴板接入 | 主线 |
| `plugins/tauri-plugin-fluxy-mobile/src/lib.rs` | 142–154 | `cfg(mobile)` 依赖分支把鸿蒙判为 mobile；未接入实现前，鸿蒙走 `keystore_available = false` | 分支（依赖判定） |
| `src-tauri/src/lib.rs` | 14–17、92 | 已 `#[cfg(desktop)]`，无需改动 | — |

### 5.3 新增文件

| 路径 | 作用 | 归属 |
| --- | --- | --- |
| `src-tauri/tauri.ohos.conf.json` | 鸿蒙专用配置；`bundle.resources` 置空（鸿蒙读内嵌资源） | 主线 |
| `.cargo/config.toml` | `[target.aarch64-unknown-linux-ohos]` 的 linker 与 sysroot（照 `OHOS_HOME` 展开） | 分支 |
| `plugins/tauri-plugin-fluxy-mobile/ohos/` | ArkTS 侧实现 + `module.json5` 片段 | 主线 |
| `scripts/ohos/env.sh` | 交叉编译环境（`TARGET_TRIPLE`、linker、bindgen） | 主线 |
| `scripts/ohos/prepare.py` | 按 pin 改写 manifest（不提交结果） | 分支 |
| `scripts/ohos/validate-hap.py` | 校验 `.so` 架构、`ets/modules.abc`、ZIP 完整性 | 主线 |
| `ohos/pins.json` | 非 crates.io 依赖清单 | 分支 |
| `src-tauri/gen/ohos/` | DevEco 工程（`AppScope`、`entry`、`hvigor`） | 分支 |

## 6. 工具链与构建

### 6.1 前置

- OHOS SDK 与 command-line-tools；`OHOS_HOME` 指向含 `ets`、`js`、`native`、`toolchains` 的目录；
- `ohpm`、`hvigorw` 在 `PATH`；
- `cargo install ohrs`；
- `cargo install tauri-cli --git https://github.com/tauri-apps/tauri --branch feat/open-harmony`（**npm 版 CLI 没有 `ohos` 子命令**）。

### 6.2 构建

参考 [tauri-demo 的 README](https://github.com/richerfu/tauri-demo)，Fluxy 的等价命令：

~~~bash
# 必须在仓库根目录执行：beforeBuildCommand("bun run build") 依赖 cwd 找到 package.json
cargo tauri ohos build --config src-tauri/tauri.ohos.conf.json
~~~

三点说明：

- 不要在 `src-tauri` 里执行。演示工程之所以覆盖 `beforeBuildCommand`，就是因为它在 `src-tauri` 里跑；我们在根目录跑就不需要覆盖。
- `--config` 必须显式传：分支的 `ohos build` 会按 Android 目标解析配置，不会自动合并 `tauri.ohos.conf.json`（TauriTavern 记录的行为）。
- 该配置里把 `bundle.resources` 置空——鸿蒙读取内嵌资源，不需要再把资源复制进 HAP。

产物：`src-tauri/gen/ohos/entry/build/default/outputs/default/entry-default-unsigned.hap`。

若 Rust 编译成功但 CLI 组装 HAP 失败，直接让 hvigor 打包：

~~~bash
cd src-tauri/gen/ohos
ohpm install
hvigorw clean --no-daemon
hvigorw assembleHap --no-daemon --stacktrace
~~~

签名与运行在 DevEco Studio 中打开 `src-tauri/gen/ohos` 配置。

### 6.3 产物校验

```bash
unzip -l entry-default-unsigned.hap | rg "libfluxy_lib.so|module.json|ets/modules.abc"
```

`scripts/ohos/validate-hap.py` 应固化为 CI 断言：应用库存在、ABI 与 ELF 架构匹配目标、ArkTS 字节码与模块清单存在。

### 6.4 CI

新增 `.github/workflows/ohos.yml`：

- 仅 `workflow_dispatch` + 分支推送触发，**不进 PR 必过检查**；
- 使用固定工具链镜像（记录 digest），或自建 runner 并缓存 SDK；
- 产出未签名 HAP 作为 artifact，不发布到 Release；
- 失败不影响 0.2.X 的其他 job。

## 7. 分支策略

`feat/open-harmony` 不是"第二主线"，而是**装钉子的抽屉**。

| 内容 | 主线（0.2.X） | feat/open-harmony |
| --- | --- | --- |
| 条件编译（`target_env = "ohos"`） | ✅ | 同步 |
| ArkTS 插件目录 | ✅ | 同步 |
| `scripts/ohos/`、`validate-hap.py` | ✅ | 同步 |
| `tauri.ohos.conf.json` | ✅ | 同步 |
| `[patch.crates-io]` / pin | ❌ | ✅ |
| `gen/ohos` 工程 | ❌ | ✅ |
| `ohos.yml` | ❌ | ✅（或主线但仅 dispatch） |
| 构建期 manifest 改写（`prepare.py`） | ❌ | ✅ |

规则：

- **合并方向单向**：`0.2.X → feat/open-harmony`，每周或每里程碑一次；反向只 cherry-pick 第 5.2 节的 cfg 改动。
- **pin 必须可追溯**：`ohos/pins.json` + `ohos-pin-YYYY.MM` tag。
- **退出条件**（写进分支 README）：连续两个里程碑合并成本 > 1 人日，或 pin 落后上游 > 1 个 minor 且无人跟进 → 冻结分支，标注"实验性、暂停维护"。
- 0.3.X 出现时，分支整体 rebase 到新发布线，并重跑 S0 清单。

## 8. 里程碑与验收

### 8.1 S0：可行性 spike（1–2 周，时间盒）

| ID | 项 | 通过判据 |
| --- | --- | --- |
| S0-1 | 构建链路 | 演示工程在本机出 HAP，装进 arm64 真机并启动 |
| S0-2 | 交叉编译 | `fluxy-net` 编到 `aarch64-unknown-linux-ohos`，UDP 绑定成功 |
| S0-3 | 发现 | `@ohos.net.mdns` 能与桌面端互相发现；Rust 侧组播结果如实记录 |
| S0-4 | 传输 | 真机 ↔ Windows 传 1 GiB，QUIC 握手、分块、续传正常 |
| S0-5 | 文件 | picker URI → fd → 4 MiB 分块读；取消不泄漏 fd |
| S0-6 | 界面 | ArkWeb 正常加载 SolidJS 前端，无白屏；`localStorage` 可用 |
| S0-7 | 生命周期 | 切后台/锁屏/杀进程后重启，会话状态可从磁盘恢复 |

**决策门**：S0-3、S0-4、S0-7 任一失败且无替代方案 → 停止本轨道，转为"文档说明 + 等待上游"；S0-2 失败 → 需重新评估 Rust 核心能否保留，此时才考虑原生重写。

### 8.2 后续里程碑（估算，待 S0 校准）

| 里程碑 | 内容 | 估算 |
| --- | --- | --- |
| M1 | 构建固化、pin、CI、`gen/ohos` 入库 | 3 人日 |
| M2 | 插件骨架：HUKS 身份、长时任务、通知 | 5 人日 |
| M3 | 文件收发闭环（picker/导出/续传） | 8 人日 |
| M4 | 发现层替换、UI 适配、分享入口、侧载说明 | 8 人日 |
| M5 | 硬化：真机矩阵、弱网、10 GiB、杀进程 | 5 人日 |

## 9. 风险

| ID | 风险 | 概率/影响 | 缓解 |
| --- | --- | --- | --- |
| RO-1 | 上游分支停滞、无人合并 | 高/高 | pin + 季度评估 + 明确的退出条件；随时可退回"仅文档支持" |
| RO-2 | `target_os = "linux"` 导致误编桌面依赖 | 高/中 | 第 5 节 cfg 清单；CI 只在鸿蒙 target 上跑一次 `cargo check` |
| RO-3 | 组播/接口枚举不可用 | 高/高 | ArkTS `@ohos.net.mdns` 优先；Rust 探测降级为兜底；诊断页兜底 |
| RO-4 | 文件 URI 授权不持久 | 中/中 | 续传前重新选择；能持久化则持久化，失败给出明确提示 |
| RO-5 | 后台挂起导致传输中断 | 中/中 | 长时任务 + 检查点 + 通知；不做"永不掉线"承诺 |
| RO-6 | ArkWeb 行为差异（DOM Storage、脚本顺序、白屏） | 中/高 | S0-6 前置验证；升级 HAR 时复验 |
| RO-7 | 工具链不可复现（个人 fork、镜像 digest） | 中/高 | `pins.json` + 自建 CI 镜像 + 产物校验脚本 |
| RO-8 | 上架门槛（认证/软著/备案） | 高/中 | 首版仅侧载内测；上架单独立项 |
| RO-9 | 单一维护者风险 | 中/高 | 分支归属写明责任人；无维护时冻结而非半死 |
| RO-10 | 鸿蒙改动倒灌 0.2.X 发布线 | 低/高 | 分支隔离 + 主线只允许 cfg/新增目录两类改动 |

## 10. 开放问题

1. 反馈用户的系统版本分布？若绝大多数是 4.x，本方案可整体降级为"文档 + 等上游"。
2. 首发形态：开发者模式侧载，还是申请上架？
3. 是否需要 HarmonyOS PC（2in1）？同一 HAP 可跑，但窗口/键盘/鼠标行为需要单独验收。
4. 发现层是否直接放弃 Rust 组播，全部走 `@ohos.net.mdns`？这取决于 S0-3。
5. 若上游最终合并，主线升级路径如何回收分支差异？（预期只需删掉 patch 与 `prepare.py`）

## 11. 参考

- [tauri-apps/tauri feat/open-harmony](https://github.com/tauri-apps/tauri/tree/feat/open-harmony)
- [tauri #7287 Add support for HarmonyOS](https://github.com/tauri-apps/tauri/issues/7287)
- [richerfu/tauri-demo](https://github.com/richerfu/tauri-demo)：最小可跑工程
- [TauriTavern OpenHarmony 开发说明](https://github.com/darkatse/tauritavern/blob/main/docs/OpenHarmonyDevelopment.md)：真实落地的坑与边界
- [@ohos.net.mdns](https://developer.huawei.com/consumer/cn/doc/harmonyos-references-V5/js-apis-net-mdns-V5)
- [@ohos.pasteboard](https://developer.huawei.com/consumer/cn/doc/harmonyos-references-V5/js-apis-pasteboard-V5)
- [docs/MOBILE_PLAN.md](MOBILE_PLAN.md)：Android 轨道与端口抽象

---

## 附录 A：为什么不迁移 Flutter

1. 引擎在 Rust：`fluxy-net` 是 QUIC + rustls + 自定义握手，Dart 没有对应栈；迁移要么重写引擎，要么维护一套 FFI 桥。
2. Flutter 官方同样不支持鸿蒙：只能依赖社区 SIG 分支，按公开路线其版本落差为 4–7 个月，且承认内存/负载指标偏高。
3. 迁移会同时抛掉现有的桌面 Tauri 体验、双端共用的 SolidJS UI 和 [MOBILE_PLAN](MOBILE_PLAN.md) 的 Android 排期。
4. 真正的保险是"Rust 核心与 UI 解耦"，不是"换 UI 框架"：即使 Tauri 分支死亡，也可以保留核心、改用 ArkTS 原生 UI。
