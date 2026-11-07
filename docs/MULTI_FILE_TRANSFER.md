# 多文件传输：按流身份配对

本文记录一次已经完成的重构。起因是多文件发送只能成功一个，
下面写清楚根因、最终设计、已经试过并失败的方案，以及验证方式。

状态：**已实现**（分支 `fix/multi-file-stream-identity`）。

## 症状

发送三个文件，接收端弹出一个对话框列出三个文件，点接收后：

- 一个文件传完
- 另外两个以 `invalid manifest: manifest has a different transfer id` 或
  `invalid manifest: chunk request has a different transfer id` 失败
- 更早的版本里，三个全部停在 `transferring` / 0 字节，既无进度也无错误

## 根因

QUIC 的双向流**不带任何身份信息**。

~~~text
接收端 A: open_bi()  →  流 1
接收端 B: open_bi()  →  流 2
发送端 X: accept_bi() → 拿到流 1 或流 2，无从判断
~~~

发送端拿到一条流之后，会把自己文件的 manifest 写上去。如果拿到的是对方的流，
对端读到的 manifest 里 transfer_id 不符，于是失败。单向流同样会串，所以块数据
走 `open_uni` / `accept_uni` 也躲不开。

这不是并发数的问题，是**协议层缺少流归属的声明**。默认并发改成 1 只对新配置
生效，用户已有的配置里存着 2，所以线上的失败一直存在。

## 最终设计（已实现）

~~~text
接收端：open_bi() → 先写 16 字节 transfer_id → 发首批 chunk 请求 → 收 manifest
发送端：每个连接一个 accept 循环 → 读那 16 字节 → 查登记表 → 在这条流上服务
~~~

一条双向流承载一个文件的全部交互：流名、manifest、块请求、块应答、块数据。
接收端的每一条流都带名字，发送端按名字把它交给对应的 `send_file`。

四条约束：

1. **一条双向流承载一个文件的全部交互**——流名、manifest、块请求、块数据。
2. **接收端决定并发**：每条 `pull_file` 自己 open 一条流，不共享。
3. **发送端不得丢弃不匹配的流**：一条流属于另一个接收端，它只有这一条；
   reset 它等于让对方的传输必然失败。名字暂时没登记的流先挂起（park），
   等对应的 `send_file` 登记后再取走。
4. **块按定长头 + 长度读**，不能用 `read_to_end`——同一条双向流不会结束。

### 帧的区分

同一条流上既有控制帧又有块帧，用前 4 个字节区分：

- 控制帧：4 字节大端长度 + `Envelope`（上限 8 MiB）。
- 块帧：以 `FCH1` 开头。`0x46434831` 远大于 8 MiB，两者不可能混淆。

块帧先读满 68 字节定长头（`ChunkHeader::decode_fixed`），由 `payload_len`
决定再读多少字节。两端都不再对这条流调用 `read_to_end`。

### 读取与取消

帧读取不是 cancel-safe 的：`select!` 里被 tick 打断的 `read_exact` 会把已经
读掉的字节丢掉。所以每条传输流由一个独占的读取任务持有，主循环只从 channel
接收**整帧**；任务用 `AbortOnDrop` 管理，任何返回路径都会中止它。

### 发送端路由

- 每个 QUIC 连接一个 `StreamRouter`（`stable_id` 索引，全局表只存 Weak），
  懒启动，连接关闭时 accept 循环退出。
- 登记表按 transfer_id 找到等待中的 `send_file`；先到、还没登记的流进入
  parked 表（上限 256 条，超过丢最旧的），登记时先取 parked。
- 连接断开时，所有等待者立即收到错误，而不是各自等满 30 秒。

### 重复内容（dedup）顺带修好

旧代码在接收端判定“已经有了”时，打开一条流去读对方写的 manifest。但发送端
此时还停在 `wait_for_decision` 上，根本没去 accept 那条流，所以这条路径只会
超时。现在“已有此内容”的答复走控制会话的 `TRANSFER_DUPLICATE`，正好是
`SessionEvent::TransferDuplicate` 一直在等的那条消息，也不再产生一条无名的流。

## 代码位置

~~~text
crates/fluxy-net/src/transfer.rs
  StreamRouter / router_for / claim / dispatch    发送端按流名路由
  serve_on_stream                                 单条流上的 manifest + 块应答
  pull_file                                       写流名、收 manifest、收块
  read_transfer_frame / TransferFrame             控制帧与块帧的统一读取
crates/fluxy-core/src/transfer/chunk.rs
  ChunkHeader::decode_fixed                       只有定长头、不含负载的解析
src-tauri/src/transfer.rs
  receive_inner                                   dedup 改写控制会话消息
~~~

## 已试过并失败的方案

| 方案 | 结果 | 原因 |
| --- | --- | --- |
| `TRANSFER_HELLO` protobuf 帧 | 三个全部卡死 | 发送端丢弃不匹配的流，毁掉对方的流 |
| 默认并发改 1 | 无效 | 用户已有配置里存着 2，默认值只影响新配置 |
| 接收端加闸门串行 | 无效 | 发送端三个 `send_file` 早已并发在 `accept_bi` 上 |
| 发送端串行（整批 claim 后顺序跑） | offer 发不出去 | 卡在 `wait_for_session` |
| 十六字节流名 + 发送端串行 | 全部 timeout | `max_active_transfers` 仍是 2，两个并发 accept 仍在抢流 |

最后一行说明：光有流名不够，**发送端必须能按名字把流交给对应的任务**。
只加名字、仍然各自 `accept_bi` 的实现，会把别人的流拿到自己手里再丢掉。

## 测试

~~~text
cargo test -p fluxy-core -p fluxy-net --lib
~~~

新增的回归用例：

- `two_transfers_run_at_once_on_one_connection`：同一连接上两个文件同时传，
  两端都完成、内容一致。旧实现里两条流可能互换，正是线上失败的那条路径。
- `a_stream_named_for_another_transfer_is_left_alone`：先开一条名字对不上的流，
  再传真正的文件；发送端必须把先到的流挂起，不能往里面写 manifest。
- `reads_the_header_before_the_payload_arrives`：`decode_fixed` 只吃定长头。

## 兼容性

传输流的格式变了（接收端先写流名，块数据不再走单向流）。新旧版本之间可以配对、
聊天，但**传输会失败**。两端必须同时升级；`PROTOCOL_VERSION` 没有跟着改，
因为改它会让新旧版本互相不可见（发现与握手都按版本过滤），影响面比传输更大。

## 未做

1. **接收端并发上限**：`accept()` 目前对每个接受的 offer 直接 spawn
   `run_receiver`，没有上限。QUIC 的 `max_concurrent_bidi_streams` 是 16，
   超出的流会排队等流位，不会失败，但一个上百文件的批次会同时占住很多任务。
2. **大批次的 parked 上限**：256 是防御性上限，正常批次远小于它。
3. `DEFAULT_ACTIVE_TRANSFERS` 仍是 1：这是吞吐选择，不再是正确性要求。
   想让整批一起跑的用户可以在设置里调高。

## 验证方法

~~~powershell
bun tauri android dev          # 唯一可用于真机验证的方式
~~~

**注意**：`eprintln!` 在 Android 上不进 logcat，只有被 `android_logger` 接管的
`tracing` 才会显示。`fluxy-core` 的日志默认 target 会被 `fluxy=info` 过滤掉，
需要 `tracing::info!(target: "fluxy", ...)`。

日志在 `adb logcat -s RustStdoutStderr`。

判断 APK 是否包含改动，比对 `adb shell dumpsys package com.thep0y.fluxy` 的
`lastUpdateTime` 与源文件的 mtime。

## 相关 commit

~~~text
a940e13 Merge pull request #26 from fluxy-rs/feat/multi-file-send
e014d73 feat(chat): send and accept several files at once
13a0ba0 fix(transfer): recover from a stale checkpoint and report failures
637632c fix(transfer): give a batch one offer slot, not one per file
d137e57 perf(transfer): offer a batch only once every file is hashed
~~~
