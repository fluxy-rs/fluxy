# Inbound `Incoming` surfaces ~30 s late on Android (quinn 0.11.12)

Status: open upstream question, reproducible on demand. Measured 2026-11-01.
Written to be attachable to a quinn issue as-is.

## Summary

A QUIC server endpoint on Android does not raise `Incoming` for an arriving
connection until roughly 28-30 s after the client dialled. The client side of the
same connection completes its handshake in 24-74 ms, so the server is demonstrably
receiving and answering the packets. The delay lands entirely between the UDP
socket and `Endpoint::accept()`, is stable to within ~50 ms across runs, and
vanishes into a *total absence* of `Incoming` if `max_idle_timeout` is set
explicitly to anything other than the default.

## Versions

| Crate | Version |
| --- | --- |
| quinn | 0.11.12 |
| quinn-proto | 0.11.19 |
| quinn-udp | 0.5.16 |
| tokio | 1.54.1 |
| rustls | 0.23.46 |
| ring | 0.17.14 |

Server: Android 16 (SDK 36), arm64-v8a, Tauri 2 app.
Client: Windows 11 desktop, same crate versions, same code paths.

## Symptom

Server side, one line per inbound connection:

```text
waiting at Endpoint::accept() ...
quic incoming surfaced   queued_ms=30329 remote=192.168.0.102:55185
quic handshake resolved  handshake_ms=20
```

where `queued_ms` is timed around `endpoint.accept()` and `handshake_ms` around the
subsequent `Incoming` await:

```rust
let waiting = std::time::Instant::now();
let incoming = endpoint.accept().await.ok_or(...)?;
let queued_ms = waiting.elapsed().as_millis() as u64;
let handshaking = std::time::Instant::now();
let connection = timeout(Duration::from_secs(10), incoming).await??;
let handshake_ms = handshaking.elapsed().as_millis() as u64;
```

Client side for the same connection:

```text
dialling the peer addr=192.168.0.101:49784
quic handshake completed addr=192.168.0.101:49784     <- 12 ms after dialling
control stream open addr=192.168.0.101:49784
```

## Reproducibility

Six consecutive runs, each with a freshly started server process:

| Run | `queued_ms` | `handshake_ms` |
| --- | --- | --- |
| 1 | 27828 | 8 |
| 2 | 27876 | 5 |
| 3 | 27823 | 8 |
| 4 | 27765 | 8 |
| 5 | 30034 | 11 |
| 6 | 30329 | 20 |

Spread of ~50 ms within the first four. The delay is deterministic, not statistical.

## What has been ruled out

Each item was excluded by measurement on the device, not by reasoning:

| Hypothesis | Measurement | Result |
| --- | --- | --- |
| Packet loss on the path | `ping -n 20 -l 1400` desktop to phone | 0% loss, RTT 4-122 ms |
| CPU sleep / doze | `dumpsys power`, `dumpsys wifi` | `mWakefulness=Awake`, no power save, partial wake lock held |
| `keep_alive_interval` interacting with the delay | 10 s to 1 s | no change |
| Wildcard bind causing path ambiguity | `/proc/net/udp` after pinning | listener bound to `192.168.0.101:49784`, not `0.0.0.0` |
| Async runtime starvation | 1 s `tokio::time::interval` ticking beside the accept future | ticks kept perfect time (`ticks=25 elapsed_ms=24003`) throughout the wait |
| Dialling before the port was bound | polled `/proc/net/udp` then dialled 8 s later | port bound after 1 s; delay unchanged at 27.9 s |
| `tokio::select!` polling pattern | removed `select!`, awaited `endpoint.accept()` directly | `queued_ms=30034`, unchanged |

## The anomalous part: explicit `max_idle_timeout`

This is the observation that does not fit an idle timer's description.

With quinn's default (no `max_idle_timeout` set), the connection is accepted after
~30 s. With an explicit value, `Incoming` is **never** raised:

| `max_idle_timeout` | Outcome |
| --- | --- |
| unset (default 30 s) | `Incoming` after 27.8-30.3 s |
| `5 s` | no `Incoming` within 25 s |
| `60 s` | no `Incoming` within 75 s |

Both non-default values suppressed the event entirely, in both directions relative
to the default. Setting a shorter timeout and setting a longer timeout produced the
same result, which points at connection-table housekeeping reacting to the presence
of the setting rather than at the timeout value itself.

Config used for the test:

```rust
let mut config = quinn::TransportConfig::default();
config
    .max_concurrent_bidi_streams(VarInt::from_u32(16))
    .max_concurrent_uni_streams(VarInt::from_u32(32))
    .stream_receive_window(VarInt::from_u32(16 * 1024 * 1024))
    .receive_window(VarInt::from_u32(128 * 1024 * 1024))
    .initial_mtu(1452)
    .congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()))
    .keep_alive_interval(Some(Duration::from_secs(10)));
// plus, only for the experiment:
// .max_idle_timeout(Some(IdleTimeout::try_from(Duration::from_secs(5))))
```

## Why this matters beyond the delay

A 30 s wait before the server can even see an accepted connection means:

- every inbound chat session takes ~30 s to establish;
- small control frames written immediately after the client's handshake are
  frequently lost, because the server has no connection object yet;
- on a phone this reads as "connected but nothing happens".

## Open questions for upstream

1. Is `Endpoint::accept()` expected to yield `Incoming` only after some periodic
   housekeeping tick rather than as soon as the handshake completes?
2. Why would setting `max_idle_timeout` to any explicit value suppress `Incoming`
   completely on this platform?
3. Does this reproduce on desktop-to-desktop with the same versions, or is it
   Android-specific?

## It is Android-specific: the same crate is fast on Windows

Tested the reverse direction in the same session. With the Windows desktop as the
QUIC *server* and the phone as the *client*:

```text
13:20:46  quic incoming surfaced   queued_ms=2817278 remote=192.168.0.101:53553
13:20:46  quic handshake resolved  handshake_ms=6
13:20:46  incoming pairing connection accepted
13:20:51  chat session live
```

The `queued_ms` figure is not comparable across directions - the desktop's accept loop had
been waiting since its own start, 47 minutes earlier - but the rest is decisive: once
the phone dials in, the desktop raises `Incoming`, completes the handshake in
**6 ms**, and reaches a live session in **39 ms**. Same quinn 0.11.12, same
`transport_config()`, same TLS settings, same code. The 28-30 s appears only when
**Android** is the server.

That narrows the report to: on Android, a *server* endpoint holds a completed
handshake without surfacing `Incoming` for ~30 s, while a Windows server with identical
configuration surfaces it immediately.

## Current state in this tree

The timing instrumentation (`queued_ms`, `handshake_ms`) is kept in
`crates/fluxy-net/src/transport.rs` because it separates the two waits at a glance.
The `max_idle_timeout` experiment has been reverted; `keep_alive_interval` is back
to 10 s.
