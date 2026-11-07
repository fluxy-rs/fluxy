# 多文件传输：按流身份配对的重新设计

本文记录一次未完成的重构。起因是多文件发送只能成功一个，
下面写清楚根因、目标设计、已经试过并失败的方案，以及接下来该怎么做。

## 症状

发送三个文件，接收端弹出**一个**对话框列出三个文件，点接收后：

- 一个文件传完
- 另外两个以 `invalid manifest: manifest has a different transfer id` 或
  `invalid manifest: chunk request has a different transfer id` 失败
- 更早的版本里，三个全部停在 `transferring` / 0 字节，既无进度也无错误

## 根因

QUIC 的双向流**不带任何身份信息**。

```
接收端 A: open_bi()  →  流 1
接收端 B: open_bi()  →  流 2
发送端 X: accept_bi() → 拿到流 1 或流 2，无从判断
```

发送端拿到一条流之后，会把自己文件的 manifest 写上去。如果拿到的是对方的流，
对端读到的 manifest 里 transfer_id 不符，于是失败。

**单向流同样如此**：块数据走 `open_uni` / `accept_uni`，一样会串。

所以这不是并发数的问题，是**协议层缺少流归属的声明**。

## 目标设计（采纳接收端控制并发的方案）

```
发送端：把选中的文件挂成一个 file server，等别人来取
接收端：按自己的任务数，open 一条流，先写「我要哪个文件」，再拉
发送端：accept → 读那 16 字节 → 查表 → 服务那个文件
```

四条约束：

1. **一条双向流承载一个文件的全部交互**——流名、manifest、块请求、块数据。
2. **接收端决定并发**，`max_active_transfers` 的语义从「发送端同时发几个」
   变成「接收端同时拉几个」。
3. **发送端不得丢弃不匹配的流**。那条流属于另一个接收端，它只有这一条；
   reset 它等于让对方的传输必然失败。（这一条是上一轮 hello 方案全部超时的原因。）
4. **块按定长头 + 长度读**，不能用 `read_to_end`——同一双向流不会结束。

## 已试过并失败的方案

| 方案 | 结果 | 原因 |
| --- | --- | --- |
| `TRANSFER_HELLO` protobuf 帧 | 三个全部卡死 | 发送端丢弃不匹配的流，毁掉对方的流 |
| 默认并发改 1 | 无效 | 用户已有配置里存着 2，默认值只影响新配置 |
| 接收端加闸门串行 | 无效 | 发送端三个 `send_file` 早已并发在 `accept_bi` 上 |
| 发送端串行（整批 claim 后顺序跑） | offer 发不出去 | 卡在 `wait_for_session` |
| 十六字节流名 + 发送端串行 | 全部 timeout | `max_active_transfers` 仍是 2，两个并发 accept 仍在抢流 |

## 代码现状

```
crates/fluxy-net/src/transfer.rs
  send_file            已拆成「配对流」+ send_file_on_stream
                       块改在同一双向流上顺序写（未验证）
  pull_file            仍用 accept_uni 收块、read_to_end 读帧 —— 必须跟着改
  read_chunk           用 read_to_end，依赖流结束；同一条双向流上会死锁
```

**发送端已改、接收端未改，工作树跑不起来。** 那部分改动存在
`git stash` 里（`wip: stream identity refactor (unfinished)`），
当前工作树已恢复到 `13a0ba0`。

## 接下来的步骤

1. **`read_chunk` 改为按帧读**：先 `read_exact(CHUNK_HEADER_LEN)`，解析出负载
   长度，再 `read_exact` 读负载。不要用 `read_to_end`。
2. **`pull_file` 的 `select!` 区分控制帧和块**：控制帧有 4 字节长度前缀，
   块以定长头开始。需要一个明确的判别方式——建议先读 4 字节，
   若长度值落在合理区间则当作控制帧，否则视为块头的前 4 字节。
3. **`pull_file` 去掉 `accept_uni` 分支**，块直接从 `control_recv` 读。
4. **`start_transfers` 改为登记表 + 一个 accept 循环**：不再为每个文件 spawn
   `run_sender`，而是把这批文件登记进可查询的表，由 accept 循环读 id 查表服务。
5. **`accept()` 加接收端并发限制**（目前直接 `spawn(run_receiver)`，没有限制）。
6. 测试：`serve_with_tampering`、`receiver_fails_when_no_chunks_arrive` 两个
   假服务端要跟着改（它们 `accept_bi` 后直接写 manifest，不读流名）。

## 验证方法

```powershell
bun tauri android dev          # 唯一可用于真机验证的方式
```

**注意**：`eprintln!` 在 Android 上不进 logcat，只有被 `android_logger` 接管的
`tracing` 才会显示。`fluxy-core` 的日志默认 target 会被 `fluxy=info` 过滤掉，
需要 `tracing::info!(target: "fluxy", ...)`。

日志在 `adb logcat -s RustStdoutStderr`。

判断 APK 是否包含改动，比对 `adb shell dumpsys package com.thep0y.fluxy` 的
`lastUpdateTime` 与源文件的 mtime。

## 相关 commit

```
13a0ba0  fix(transfer): recover from a stale checkpoint and report failures
641176e  chore(transfer): trace the sender/receiver stream handshake
637632c  fix(transfer): give a batch one offer slot, not one per file
d137e57  perf(transfer): offer a batch only once every file is hashed
e014d73  feat(chat): send and accept several files at once
```

前四个是真 bug 的修复，最后一个包含前端的多文件选择、批量弹窗和拖拽。
多文件真正能传，取决于本文的重构。
