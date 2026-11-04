# Implementation status

Updated: 2026-10-23

This file tracks the executable baseline, not the product plan. The plan remains
`docs/DEVELOPMENT_PLAN.md`.

## Completed in this slice

| Task | Evidence |
| --- | --- |
| P0-01 version/metadata | Workspace, Tauri config, frontend, and CLI all report `0.2.0`; MIT LICENSE and project README added; Bun pinned to `1.4.3` |
| P0-02 Cargo workspace | `fluxy-proto`, `fluxy-core`, `fluxy-net`, `fluxy-app`, `fluxy-cli`; `src-tauri` is a thin Tauri shell |
| P0-03 CI baseline | `.github/workflows/ci.yml` with Rust and Web jobs; the Web job runs `check`, `check:security`, and `build` |
| P0-04 observability/errors/config | `tracing` startup log includes version and data directory; serializable `AppError`; JSON config with atomic write; `engine_status` invoke |
| M0-01 frame/model | `proto/fluxy.proto`; length-prefixed Envelope encode/decode with 8 MiB cap and identifier/kind validation; malformed-frame smoke test |
| M0-02 identity/trust | Ed25519 identity persisted as JSON, stable device id, signature verification, reset/rotation, JSON trust store, SAS helper |
| M0-03 QUIC/TLS | Quinn 0.11 endpoint with ALPN `fluxy/1`; rcgen self-signed certificate; BLAKE3 server-certificate pinning; mandatory TLS client certificate for bootstrap; loopback echo and wrong-fingerprint rejection tests |
| M0-04 pairing | Signed Hello/HelloAck transcript, capability intersection, six-digit SAS, PairRequest/PairReply/PairConfirm/Ready, per-IP failed-attempt cooldown, both trust stores written only after confirmation; rejection writes no trust |
| M0-05 control session | One bidirectional control stream; in-memory UTF-8 chat up to 32 KiB; application `MESSAGE_ACK`; signed trusted resume without SAS; 10s Ping/Pong and 30s idle detection; 1/2/4/8/15/30s reconnect backoff with jitter; `PersistentSession` reconnects automatically |
| M0-06 discovery | mDNS/DNS-SD `_fluxy._udp.local.` advertise and browse; TXT version/device/name/platform/features/certificate-fingerprint parsing; malformed and incompatible records ignored; peer table keyed by stable device ID with a 120s TTL and offline removal; selected-interface filtering; loopback isolated behind an explicit dev flag; loopback peer discovered in under three seconds |
| M0-07 CLI command set | `id`, `discover`, `serve`, `pair`, `chat`, and legacy `connect`; `discover` supports timeout/interface/JSON and found the advertised local peer in under three seconds using a separate identity; non-loopback `serve` advertises mDNS and prints `discovery=mdns`; loopback listeners skip discovery; two loopback CLI processes pair and exchange an acknowledged chat message on different ports |
| M0-08 fault injection | `tools/udp-chaos` UDP proxy with loss, fixed delay, jitter, duplicate, reorder, and one-shot outage controls; two real QUIC endpoints complete pairing and chat through the proxy; a 250ms outage drops datagrams but the chat ACK still arrives after recovery; delayed, duplicated, and reordered datagrams also complete with counters asserted |
| M0-09 security review | `docs/SECURITY.md` threat model with assets, trust boundaries, 22 STRIDE threats, and controls; `docs/security/M0_REVIEW.md` records six high-risk decisions, medium/low findings, T-08/T-15 evidence, and owner/gate actions; missing-trust rejection test added; no unresolved critical issue and no high risk blocks the M0 CLI |
| M1-01 Tauri adapter | `tauri-specta 2.0.0-rc.25` generates `src/bindings.ts`; `engine_status`, `state_snapshot`, `events_subscribe`, and `identity_rotate` commands; one Channel carries a tagged event envelope with process `seq`, `snapshot_version`, and `emitted_at_ms`; Engine start/stop events and a 10s heartbeat; snapshot rebuilds peers/settings/transfers and the Solid UI subscribes to live events and detects gaps by calling `state_snapshot`; 5 adapter tests |
| M1-02 UI shell/design baseline | Hash-routed desktop shell with a dark instrument rail, ruled workspace, status bar, and mobile bottom navigation; routes `/`, `/transfers`, `/clipboard`, `/settings`; `@neut/ui` wrappers for Button, Dialog, Toast, Tabs, Progress, Input, and Tooltip; system/light/dark theme with persisted preference; zh-CN default plus English with `lang`/document-title sync; shared empty, loading, and error states; dialog focus trap, Escape close, and focus restore; skip link; restrictive CSP plus `devCsp`, `freezePrototype`, no `opener`, and a `core:default`-only main-window capability; `bun run check:security` static regression check; live Tauri route, theme, dialog, and 760px responsive checks |
| M1-03 device list/pairing UI | LAN runtime binds a private IPv4 interface and advertises/browses mDNS with the TLS certificate fingerprint as a hint; nearby devices appear in the rail, home panel, and settings with online/offline state; outbound and inbound pairing share the async SAS dialog; the dialog shows both device names, a 6-digit split code, a live 120s countdown, the explicit "compare both screens" warning, a required acknowledgement checkbox, and a disabled confirm button until acknowledged; no QR or manual-address entry exists; trust removal has a confirm dialog and refreshes the snapshot; discovery diagnostics show interface, addresses, listener, mDNS service, advertised/browsed state, and nearby count; identity rotation restarts the QUIC listener and mDNS advertisement with the new namespace; live two-direction app/CLI pairing completed with matching SAS in both directions, and app-side trust removal was exercised |
| M1-04 chat UI | In-memory `ChatRuntime` keyed by trusted peer with a 1000-message bounded transcript; conversation list, message stream, delivery state, and per-message retry; outbound trusted resume and inbound trusted chat share `ControlSession`; each message carries an application `MESSAGE_ACK`, unacknowledged messages become retryable on disconnect, and retries reuse the original message id so the receiver deduplicates; the UI renders peer text as text, disables the composer while offline, and keeps chat memory-only; live app/CLI smoke covered app-to-CLI delivery, CLI-to-app receipt, disconnect failure, and retry; responsive checks at 1080px and 390px showed no horizontal overflow |
| M1-06 clipboard policy | Trust entries persist a per-device `ClipboardPolicy` (`ask` default, `auto`, `reject`); `clipboard_set_policy` validates trust, persists atomically, bumps the snapshot, and emits `TrustEvent::Updated`; the `/clipboard` policy panel lists every trusted device and lets the user change its policy; `Ask` keeps the manual accept queue, `Auto` is only effective after an explicit per-device selection and auto-accepts/sends the ACK, `Reject` declines with `policy_reject`; offers carry `origin_id` and `hop` (max 1, never forwarded), an offer originating locally is declined with `echo_suppressed`, and the receiver retains 16 applied BLAKE3 digests for five minutes so duplicate content is declined with `duplicate_content` and locally blocked from re-send with `clipboard_duplicate`; the global auto-send listener remains off and is labeled as a later desktop feature; 8 clipboard state tests, trust-policy persistence/event and policy-decision tests, and the net offer/text/ack round trip; live CLI→app tests covered auto-apply, manual accept, reject, and duplicate rejection |
| M1-07 notifications/tray | Native tray icon with localized open/quit menu, left-click restore, and frontend-pushed labels; OS notifications for incoming chat and clipboard offers (ask) or applied clipboard (auto), suppressed while the main window is focused; notifications contain only peer display name and event type, never message or clipboard text; notification click and tray click restore, unminimize, and focus the window; 2 notifier tests plus a native-shell contract test; Windows test binaries embed the Common Controls v6 manifest so tray/menu linkage does not break the unit-test harness |
| M1-08 frontend tests | Vitest 5 + jsdom + `@solidjs/testing-library`; shared typed fixtures and a mock engine context keep component tests free of Tauri IPC; 18 tests cover chat selection/messages/send/Enter/retry/offline and send-failure recovery, pairing acknowledgement/cancel/expiry/busy state, clipboard send/policy/auto warning/accept/reject/offline state, and fallback diagnostics; `bun run test:frontend` runs in CI |
| M1-10 hotspot/gateway fallback | Verified on a real phone hotspot (T-M10, see TESTING.md): discovery ran on the fallback channel alone at the probe interval, the session established over `ap0`, and a 100 MiB transfer completed at 43.23 MiB/s. T-M14 (client isolation) still needs router admin. Bounded UDP announcements to the limited broadcast address and each directed subnet broadcast, plus unicast default-gateway probes; responses echo a pending nonce and replies are rate-limited per source; the runtime prefers mDNS records for the same device and exposes fallback gateway, targets, probes, replies, peer count, and errors in discovery diagnostics; loopback probe/response, packet validation, broadcast-mask, and channel-precedence tests; two NetworkDiagnostics component tests cover gateway/targets/counters and error rendering; a physical phone-hotspot run remains a manual release gate |
| M1-09 end-to-end acceptance | Cross-platform Node driver `scripts/e2e/e2e.mjs` with Linux/macOS and Windows wrappers; the local five-step scenario covers first pairing with SAS, chat delivery and application ACK, headless clipboard consent rejection, trust persistence across a restart with a SAS-free resume, unknown-device rejection, and the loopback guard; `serve`/`join` modes support a real LAN run and print machine-readable connection details; the CI `e2e` matrix runs the local scenario on ubuntu, macos, and windows |
| M1-05 clipboard | `ClipboardState` and a dedicated `fluxy-clipboard` thread read/write the OS clipboard through `arboard`; the sender reads only on user action, computes BLAKE3, and sends a bounded `CLIPBOARD_OFFER`; the receiver shows peer/preview/size and writes only after an explicit accept; `CLIPBOARD_TEXT` is bounded to 1 MiB and verified against offer size/digest before the write; final ACK reports applied or a stable failure code; offers expire after 120s and all clipboard state is memory-only; the `/clipboard` route has a trusted-online peer chooser, send action, incoming accept/reject queue with countdown, bounded history, and a policy panel; `fluxy-cli clipboard` sends an offer and waits for consent, while `serve` rejects offers with `cli_no_consent`; 7 clipboard state tests plus a net offer/text/ack round trip; live CLI-to-app offer/consent/write succeeded and the app displayed the received share |
| TLS persistence | Self-signed certificate and key are stored under the data directory, so the server fingerprint and client certificate pin survive restarts; private key mode is `0600` on Unix |
| CLI transport | `serve` keeps listening for trusted resumes and prints device id, address, fingerprint, and discovery status; `pair` stores the pin; `chat --device` resumes without SAS and waits for the ACK; legacy `connect` supports `--fingerprint`, `--resume`, and `--message`; `--once` exits after one connection |
| P0-05 draft | `docs/PROTOCOL.md` documents control frames, message kinds, handshake and trusted resume, capabilities, control/chat semantics, chunk header, mDNS discovery records, and hotspot fallback |
| M2-01 file abstraction | `fluxy-core::files` defines `FileHandleId`, `FileMeta`, `FileCapabilities`, `FileReference`, `SourceIdentity`, `FileError` with a stable `ErrorCode` mapping (3001 transfer, 3002 source changed, 3003 permission, 3004 disk full), and the `FileSource`/`FileSink`/`FileAccessManager` traits; the desktop `std::fs` implementation opens verified random-read sources, creates receiver-root `.part` sinks that only finalize after full coverage, cleans temp files on abort or incomplete finalize, rejects existing destinations, and validates portable file names; 18 unit tests cover read/write boundaries, coverage merging, cleanup, registration errors, and source-shrink detection; full path/overwrite/space hardening remains M2-10 |
| M2-02 manifest and pre-hash | `fluxy-core::transfer::manifest` defines the `Manifest` (block size, file size, per-block BLAKE3 hashes, root hash, manifest digest), parallel window hashing through rayon and BLAKE3 `update_rayon`, per-block `HashProgress`, block/root verification helpers, and a 32768-block limit; `ManifestCache` stores identity-bound entries with atomic replace and rejects identity, block-size, block-count, or digest mismatches; 12 unit tests cover reference hashes, multi-window parallelism, empty files, limits, verification, cache hit/miss/tamper/replace, and error mapping; the ignored `benchmark_10gib_prehash_under_15s` gate measured 7.23 s (1416 MiB/s) for a real 10 GiB file on this machine |
| M2-03 chunk protocol and scheduler | `fluxy-core::transfer` adds a compact `ChunkBitmap` (serializable for resume), the `FCH1` `ChunkHeader` codec with payload-hash and manifest checks, and the receiver-owned `PullScheduler` (default 4 streams × 2 outstanding, 15 s timeout, 3 attempts, failed-chunk reporting); `fluxy-net::transfer` adds `manifest_to_proto`/`manifest_from_proto` (length, block-count, and digest validation), `send_file` (serves each request on a fresh uni stream, at most four concurrent), and `receive_file` (hash-verifies before writing, acknowledges stored chunks, retries timeouts); `TransferManifest`, `ChunkRequest`, and `ChunkAck` are added to the protobuf schema; 18 core and 5 loopback tests cover bitmap bounds, frame tampering, window/retry/failure accounting, manifest tampering, a 5 MiB four-stream transfer, out-of-order duplicate handling, repeated transfers on one connection, and missing-chunk failure |
| M2-04 checkpoints and cross-restart resume | `fluxy-core::transfer::checkpoint` adds `TransferCheckpoint` (role, file name, state, sender source identity, bitmap, timestamp), the `FCP1` framed progress codec with a BLAKE3 checksum, the `FMF1` binary manifest codec, and `CheckpointStore` with atomic replace, `list`/`remove`, and a `resume_plan` that degrades to a fresh transfer on a corrupt file or a manifest mismatch; `PullScheduler::resume_from` rebuilds the window from a persisted bitmap; `FileAccessManager` gains `create_sink_with_id` and `resume_sink` so the deterministic `.fluxy-<transfer-id>.part` file can be reopened with its covered ranges; `fluxy-net::transfer` gains `CheckpointResume`, `prepare_sink`, and `receive_file_resumable`, which persists after every chunk and marks the checkpoint interrupted or completed; 16 core and 3 loopback tests cover manifest/progress round-trips, a leftover atomic-write temporary that is ignored on load, torn and tampered rejection, atomic replace, missing-block plans, source-identity matching, sink reopen/extension, a sender crash that retransmits only missing chunks, a receiver crash that resumes after a sink failure, and a receiver task aborted mid-transfer that resumes from disk state alone |
| M2-05 pause, cancel, and retry | `fluxy-core::transfer::state` adds the `TransferState` lifecycle (now including `resuming`) and a `StateMachine` that enforces the DESIGN §8.1 transition table, with idempotent cancel from every active state, retry from failed/interrupted/paused/cancelled, and a `StateError` (3001) for illegal transitions; `fluxy-core::transfer::control` adds the cloneable `TransferControl` handle (running/paused/cancelled, cancel wins over pause); `ErrorCode` appends `MissingChunks` (3006) and `Canceled` (3008); `FileSink::discard` removes a partial file in place and `PullScheduler::pause` requeues the window without consuming the retry budget; `fluxy-net::transfer` adds `TransferOptions` (resume, control, cancel checkpoint policy), maps `TransferError::code`/`is_retryable`, honors pause/resume in the pull loop with persisted `paused`/`cancelled` states, discards the partial file and removes the checkpoint on cancel, and lets `send_file` cancel an outgoing transfer; 12 core and 3 loopback tests cover the transition table, cancel/retry rules, control idempotency, error-code numbers, discard, window requeue, pause-then-resume completion, cancel-from-paused cleanup, and sender cancel |
| M2-06 integrity, finalize, and source change | `ErrorCode` appends `HashMismatch` (3005); `fluxy-core::files` adds `hash_reader` (streaming BLAKE3), `FileSource::stat`/`verify_unchanged` (live re-stat, 3002 on change), and `FileSink::root_hash`; `ChunkHeader::decode_header` parses a frame without validating the payload so a rejected block keeps its index; `PullScheduler::on_chunk_rejected` expires the request immediately so a tampered block is retried on the next tick and only fails with 3006 when the budget is spent; `fluxy-net::transfer` re-stats the sender source before every request (3002), rejects and retries tampered frames without writing them, hashes the finished partial file and compares it with `manifest.root_hash` before `verifying -> completed`, and on mismatch fails with 3005, drops the checkpoint, and discards the partial file; 4 core and 4 loopback tests cover the streaming hash, the empty-file root hash, live stat detection, immediate re-request after rejection, a tampered-once transfer that still completes, an always-tampered transfer that fails with 3006 and keeps the partial file, a mid-transfer source change returning 3002, and a lying sink rejected with 3005 before finalize |
| M2-07 transfer UI | `fluxy-proto` adds the nine transfer negotiation kinds (`TRANSFER_OFFER` 40 through `TRANSFER_ERROR` 51) with `TransferOffer`, `TransferAccept`, `TransferReject`, `TransferProgress`, `TransferComplete`, `TransferError`, `TransferPause`, `TransferCancel`, and `TransferResume`; `fluxy-net::session` encodes them on the control stream, adds `Command::SendTransfer` and `ControlSender::send_transfer`, and raises nine `SessionEvent::Transfer*` variants; `fluxy-net::transfer` gains a progress callback and `TransferOptions::with_progress`; `fluxy-core::transfer::checkpoint` adds `resume_plan`; `src-tauri/src/transfer.rs` adds the application-layer `TransferRuntime` (offer/accept/reject handshake with a 60 s decision timeout, cached manifests, a resumable receiver, sender and receiver tasks, `TransferControl` pause/resume/cancel/retry, a 100 ms progress throttle, an EWMA speed meter, and per-record summaries) plus the `pick_file`, `register_file`, `start_transfer`, `transfer_respond`, `transfer_control`, and `transfers_list` commands, a `transfer` event category, and an `offered` IPC state; the `/transfers` route becomes a transfer center with a state filter, per-task progress/size/speed/ETA, pause/resume/cancel/retry, an incoming-offer accept/reject dialog, a native file picker plus trusted-device chooser for sending, and an error-detail dialog with copy; the filesystem path never enters WebView state because the picker runs in Rust and returns an opaque handle; 11 Rust runtime tests cover the throttle, the speed meter, the opaque handle, control rejection, failure details, and progress coalescing, and 15 Vitest tests cover the formatters and the transfer center |
| M2-08 drag and drop with a multi-file queue | `AppConfig` gains `max_active_transfers` (default 2, clamped to 1..8, serde-defaulted so a config written by an earlier version still loads) with `Engine::set_max_active_transfers`; `ErrorCode` appends `PeerBusy` (2002) and `TransferError` maps it; `TransferRuntime` gains the batch API (`register_paths`, `start_transfers`, `accept_drop`, `set_drag_hover`), a per-record creation stamp, slot accounting (`active_count`/`offered_count`/`occupied`), FIFO `pump_queue`/`claim_next_queued` promotion, a `queued` IPC state, the `peer_busy` admission gate, and a 100-file batch cap; the Tauri window drop handler registers paths in Rust and publishes `drag_hover`/`files_dropped` events that carry only opaque handles and base names; new commands `pick_files`, `start_transfers`, and `set_transfer_concurrency` (persisted) plus the transfer events `queued`, `drag_hover`, and `files_dropped`; the transfer center gains a batch confirmation dialog with the file list, totals, rejected names, and the per-send limit, a drop overlay, a `queued` row state, and a 1..4 concurrency selector; 10 runtime, 3 config/engine, and 4 Vitest tests cover the cap source, slot accounting, FIFO claiming, the batch limit, directory rejection, a ten-file drop, a ten-deep queue, drag-hover de-duplication, offer expiry, config persistence and the serde default, the picked and dropped batches, the drop overlay, the queued row, and the concurrency control |
| M2-09 fault injection and the 10 GiB benchmark | `fluxy-app` gains a UI-free single-file transfer driver (`HeadlessTransfer` with `offer`/`serve`/`receive`, `SendRequest`, `OfferView`, `PreparedSend`, `SendOutcome`, `ReceiveOutcome`, and `HeadlessError` with the shared code mapping) that the CLI and the fault tests both drive, plus `ErrorCode::as_str` as the single snake_case source; `fluxy-cli` gains `transfer-send`/`transfer-recv` with machine-readable `FLUXY_XFER` lines and 2 s throttled progress; `tools/udp-chaos/tests/transfer_faults.rs` covers T-02 (short outage recovers in place), T-03 (a restarted receiver resumes exactly the missing bytes), T-04 (a changed source is refused with 3002 and leaves no complete file), T-06 (100 stamped chat messages arrive with p95 at or below 250 ms), and the ignored 10 GiB T-01 benchmark; `scripts/acceptance/` drives the real CLI for `local`, `outage`, `restart`, `bench`, `faults` and `matrix` with receiver RSS sampling and a T-01..T-06 report; `fluxy-net` schedules the session control stream at priority 100 and chunk uni streams at -50 (the chat test proved it was needed) and refills the receiver's pull window when a chunk lands instead of waiting for the 100 ms tick (a pause still suspends the refill) |
| M3-01 settings, i18n, theme, and accessibility | `AppConfig` gains `notifications` (serde-defaulted to true) alongside `Engine::set_device_name`/`set_notifications`; `SettingsSnapshot` exposes `notifications`; the new `set_device_name`/`set_notifications` commands refuse to run while a pairing is pending (4105) and a real rename restarts the LAN runtime so mDNS re-advertises the new name; `Notifier` gained an enabled flag that is applied from the persisted config at startup and consulted before every dispatch; the settings page gains a device-name field (Enter saves, Escape reverts, inline validation, 64-character bound) and a `role="switch"` notifications toggle; the dialog now restores focus when a parent unmounts it instead of closing it, and the nearby-device pair button uses `aria-disabled` so focus survives a pending challenge and returns to the trigger; the catalog test guards zh/en parity, placeholder parity, every key written in the source (including the `transfers.state.*`/`clipboard.state.*`/`clipboard.direction.*` families expanded from the bindings unions), no hardcoded CJK copy outside `src/i18n`, and no dead keys (31 unused keys removed); 25 new Vitest tests cover the catalog guard, the settings page and its new panel, the dialog keyboard contract, and the keyboard-only main flow |
| M3-02 transfer details and notifications | The transfer center opens a detail dialog for every transfer (state, direction, peer, progress with speed and ETA, created/updated clock) and adds the failure block for a failed one: the localized reason, the protocol code and number, whether the core considers it retryable, and the raw message; `src/lib/transfer-errors.ts` maps the stable snake_case codes to `transfers.reason.<code>` and falls back to a neutral message for a code from a newer peer, and the catalog guard now derives that family from `ErrorCode::as_str` in `crates/fluxy-core/src/error.rs` so a new code cannot ship without copy; retry is offered only where the core marks the failure retryable (or for a user cancel) and runs straight from the dialog; `ShellLabels` gained transfer completion/failure labels and the notifier gained `notify_transfer_completed`/`notify_transfer_failed` carrying the file name and peer name only, with a per-device two-second window so a bulk send that finishes at once raises one notification instead of one per file; the transfer runtime notifies on completion and failure but never on a user cancel; the detail dialog and the copied text share the same fields |
| M3-03 automatic clipboard watching | The automatic clipboard-send switch (`AppConfig.auto_clipboard`, off by default) is now real: `set_auto_clipboard` persists it, and `AppState::start_clipboard_watch` runs a 300 ms poll that reads the system clipboard on a dedicated thread and offers a value only once it has been stable for a full debounce window (`ClipboardWatcher` in `src-tauri/src/clipboard.rs`); the watcher never offers content whose digest is in the applied ring and never offers the same content twice, so a value that arrived from a peer is never sent back - with the `hop` cap and the `origin_id` echo check on the receive side that makes an A -> B -> A loop impossible even when both devices watch their clipboard; offers go to every online trusted device (`online_trusted_peers`, resolved through a live session first and discovery second) and each receiver still applies its own per-device receive policy, keeping the global send switch and the per-device receive policy separate; the clipboard page shows a real switch with the online target count and the loop guarantee; enabling the switch records whatever is already on the clipboard instead of sending it, so turning the feature on never broadcasts a password copied a moment earlier |
| M3-04 tray, login item, and firewall guide | Closing the window now hides Fluxy to the tray instead of ending it (`CloseRequested` calls `api.prevent_close()` plus `window.hide()` unless a deliberate quit set the flag first), so a closed window keeps receiving chat, clipboard offers, and transfers; the window is created hidden (`visible: false`) and shown from setup unless the process was started with `--hidden`, which is exactly what a login start passes; the tray menu grew from open/quit to Open, a separator, *Auto send clipboard* and *Start at login* check items, a second separator, Help, and Quit, with the check states seeded from the persisted preferences and reverted if the backend rejects the change; the login item lives behind one `StartupBackend` trait (`src-tauri/src/startup.rs`) with a dependency-free Windows backend that writes the per-user Run value `Fluxy` through `reg.exe` (injected command runner, pure argument builders) and file-based macOS (launchd plist) and Linux (XDG desktop entry) backends whose values are escaped; `AppConfig.autostart` is serde-defaulted to false, `set_autostart` writes the login item *before* it persists so a refused write leaves the preference untouched and returns `autostart_failed` (4601), and `AppState::reconcile_autostart` re-creates a login item that a cleaner removed while the preference was on; the tray cannot navigate the WebView, so it raises the new `AppEvent::Shell` help request, which the shell consumes and turns into a route; the new `/help` page (also in the sidebar) shows the live discovery and bound transfer ports, copyable Windows/macOS/Linux firewall commands built from those ports, a troubleshooting list, the login-item location, and the diagnostics payload; `docs/FIREWALL.md` is the standalone guide behind it |
| M2-11 multi-interface selection and binding | `fluxy-net::interface` enumerates the local interfaces through `default-net` (name, friendly name, platform description, index, platform type, up flag, addresses) and implements the plan's §3.5 selection: automatic mode prefers a real adapter over a virtual one (VPN, hypervisor, container, Bluetooth - matched on name and platform description), then a LAN-capable type, then the default-route interface, then a private IPv4 address, with the platform index keeping the order stable; Ethernet and Wi-Fi rank equally. A tunnel is used only when nothing else is up, loopback never enters the automatic pick (the development switch makes it a last resort and a manual pick), and a manual selection is validated against the live table with every failure mapped to 2003. Link-local IPv6 addresses carry the interface index on the socket and are shown as `address%interface`. `NetworkSelection` is now real: `address` became optional (auto chooses), and `validate` rejects a manual pick without an interface or address. `AppConfig.network` persists the preference (`NetworkPreference` with `mode`/`interface_id`/`address`, serde-defaulted to automatic), and `Engine::set_network_selection` validates and saves it. The listener is created by `server_endpoint_bound` over a `socket2` UDP socket that is pinned to the chosen interface with `IP_UNICAST_IF`/`IPV6_UNICAST_IF` on Windows (the index in network byte order), `IP_BOUND_IF`/`IPV6_BOUND_IF` on macOS, and `SO_BINDTODEVICE` on Linux; a rejected or unsupported pin is a warning, and `interface_pinned` tells the UI that routing may still choose another path. `NetworkRuntime` polls the interface table every two seconds: automatic mode rebinds when the best candidate changes or the bound address disappears, manual mode reports 2003 and keeps waiting for the same address to come back, and a rebind pauses active transfers first so they keep their checkpoints. `NetworkStatusView` gained `selection_mode`, `selection_source`, `interface_index`, `interface_pinned`, `rebind_count`, `last_rebind_ms`, and the `interface_options` list the settings page renders. One naming trap is worth recording: the selection uses `default-net`, whose Windows `name` is an adapter GUID, while mDNS and the gateway fallback go through `if-addrs`, whose Windows `name` is the friendly name - `InterfaceInfo::discovery_name` translates between them, and both layers now also accept the other spelling. The CLI gained `fluxy-cli interfaces [--json]` and `serve --interface/--address` (omitting `--listen` now uses the selection), and `set_network_selection` is exposed to the WebView for M3-06. |
| M3-06 network interface settings and diagnostics | The settings page gained a "网络接口 / Network interface" panel that drives the M2-11 backend. It shows the active selection (interface, address, why it was chosen, whether the socket is pinned, the listen address, and how many times a network change has rebound the listener), offers automatic or manual selection, and lists every local adapter with its platform kind, index, addresses, and the marks that explain a surprising order: the active one, the recommended one, the default route, virtual/VPN adapters, and adapters that are down. Link-local addresses keep the `address%interface` form and are labelled as link-local. Rows are selectable only in manual mode, and an adapter with more than one address gets an address chooser that marks the preferred address. Applying a change always goes through a confirmation dialog that states the two consequences the plan calls out: current connections drop, and transfers in flight pause first and resume from their checkpoint after the rebind. Failures stay actionable: a rejected switch keeps the proposed selection and shows the error next to the recovery steps, and a runtime state of `unavailable` shows whether the chosen address disappeared (manual) or the listener could not come up, plus "pick an adapter, then apply". When the platform does not pin the socket, the panel says so instead of implying the traffic is pinned, which is the plan's §3.5 warning about VPN and routing. A copy action writes the interface list and the active selection as plain text for support. All copy lives in the zh/en catalog, the panel uses the shared UI wrappers, and the command is reached through `src/api` rather than `invoke`. |

| M3-07 folder batches | A folder is now one batch of ordinary single-file transfers, which is ADR-16's shape: `fluxy-core::transfer::batch` defines `BatchEntry` (relative path, size, mtime, is_dir) and a validated `BatchPlan`, and `fluxy-core::files::scan_folder` walks a folder without following symbolic links and produces that plan. Path safety is enforced before anything touches the disk: absolute paths, parent and current segments, NUL, control and bidirectional-override characters, backslashes, Windows alternate data streams (colons), reserved device names, empty segments, trailing dots or spaces, over-long segments or paths, duplicates (case-insensitively), a path that is both a file and a directory, and a path that escapes the receive root are all refused, with a 10,000-entry cap and a 4 KiB relative-path budget. The plan says a batch is described once: the first sub-transfer's offer carries `batch_id`, `root_name` and the whole `entries` list, every later offer carries only its own relative path, and the receiver creates the batch root once (numbered `root (1)` when a directory of that name exists) plus the declared empty directories. A received batch file lands under `<received>/<root>/<relative path>` - empty folders included - and a resumed sub-transfer keeps its own manifest and checkpoint, so per-file pause, cancel, retry, and cross-restart resume all still apply. The WebView groups rows by `batch_id`: one collapsible batch row with batch-wide progress, a failed count, and batch-wide pause, resume, and cancel, plus a per-file list on demand. "Send folder" opens a folder picker and a confirmation that names the root, the file count, the empty-directory count, the total size, and the first entries. |

| M3-05 regression pass | The regression milestone was a defect hunt rather than a feature: every gate was re-run from a clean tree (format, clippy with warnings denied, the whole workspace suite three times under load, the frontend type check, the component tests, the UI security scan, the production build, and the CLI end-to-end scenario), and the known gaps were re-read against the code. Two real defects were fixed. First, dropping a folder on the window refused it: `register_paths` only understood files, so the plan's "drag a file or folder onto the window and land in the send confirmation" stopped at files. A dropped folder is now scanned and validated before anything is offered, arrives as a `RegisteredFolderView` in the `FilesDropped` event, opens the folder confirmation directly (one folder at a time, with the rest counted), and has its own drop budget separate from the 100-file budget. Second, the file budget counted accepted files instead of considered paths, so a drop of unreadable paths could walk past the limit; the counter now advances for every file path considered, which is what the original limit meant. The remaining documented gaps are deliberate: a purely empty folder still has no sub-transfer to announce it, dragging a folder onto the window now works, and the visually-dependent checks (tray restore, login start, toasts, the two-host transfer matrix) stay on the manual list. |

| M5-01 performance pass | The transfer path was measured instead of guessed. A repeatable loopback benchmark (`cargo test -p fluxy-net --release --lib benchmark_loopback_throughput -- --ignored --nocapture`) moves 256 MiB through two endpoints with four chunk streams and an NVMe write on the receiving side, and can sweep streams, window, and block size through `FLUXY_BENCH_*`. Baseline was 242 MiB/s and the sweep settled at 226-266 MiB/s: streams (4 to 16), per-stream window (2 to 4), block size (capped at the protocol's 4 MiB), a jumbo initial MTU (9000), larger UDP socket buffers, larger QUIC receive windows, and removing two 4 MiB copies per chunk all moved the number by noise or single digits. That is the finding: the limit is the per-chunk control-loop round trip and a single UDP socket's packet rate, not bandwidth or copying, and the 1.0 GiB/s loopback target in plan 11.3 is not met by this architecture - 1 GbE's 105 MB/s target is met with about 2.4x headroom. The changes that were kept are the ones that are correct on their own merits: the UDP socket now asks for an 8 MiB buffer so a burst is not dropped and read as congestion, the QUIC receive window covers a whole in-flight window (16 MiB per stream, 128 MiB per connection, 32 uni streams), a chunk's header and payload are written separately instead of copied into a second 4 MiB frame, and a received chunk is shared with the writer instead of copied out of the frame buffer. The benchmark stays in the suite, ignored by default, so the next optimization starts from a number rather than an opinion. |

| M5-03 security audit, dependencies, and fuzz | The release gate now has a checked-in policy and a reproducible run. `deny.toml` targets Windows, macOS, and Linux, allows only the redistributable license set (MIT, MIT-0, Apache-2.0 with the LLVM exception, BSD-2/3, ISC, Zlib, Unicode-3.0, Unicode-DFS-2016, CC0-1.0, BSL-1.0, MPL-2.0, Unlicense), denies wildcard and unknown-registry dependencies, and warns on duplicates. `cargo deny check licenses bans sources` passes on the 622-package tree; the two decisions it needed are recorded in `docs/SECURITY_AUDIT.md` (MPL-2.0 through the CSS stack is accepted as unmodified file-level copyleft, and r-efi's LGPL arm does not apply because we take its MIT or Apache arm). The deny run also forced a real fix: path-only workspace dependencies counted as wildcards, so every internal crate and the tool now carries an explicit `version`. The advisory check ran once the RustSec database was reachable: it reports no vulnerabilities and five unmaintained crates (`paste` through specta, `proc-macro-error` through the Linux gtk3 stack, and `zbus_names`/`zvariant`/`zvariant_derive` through the Linux notification stack). Each was traced per target with `cargo tree`: two are build-time macros that never reach a binary, three ship only in the Linux build, and none has a safe upgrade in this graph. They are explicit, reasoned `ignore` entries in `deny.toml` with a re-check trigger per crate, so `cargo deny check` reports advisories, bans, licenses, and sources all ok. `cargo-fuzz` needs nightly, so the fuzz harness is a deterministic, seeded generator that does the same job on stable: about three quarters of the iterations mutate a valid frame, chunk, manifest, checkpoint, or batch JSON (bit flips plus truncation) to reach the length and hash arithmetic instead of stopping at the magic check, with the rest fed pure garbage, over `decode_frame`, `decode_payload<Hello>`, `ChunkHeader::decode`, `Manifest::from_bytes`, `TransferCheckpoint::from_bytes`, `BatchPlan::from_json`, `validate_relative_path`, and `sanitize_root_name`. Ten million iterations ran clean in 175 s; a thirty-five-million-iteration run covers the plan's ten minutes. `bun run audit` (`scripts/security/audit.mjs`) runs the same checks and inventories the five `unsafe` blocks. |
| M5-04 packaging, signing, and installers | The bundle is no longer a stub. `src-tauri/tauri.conf.json` now carries the installer metadata a release needs - publisher, copyright, category, short and long description, and `licenseFile` pointing at the repository LICENSE - plus per-user NSIS installs (`installMode: currentUser`, no elevation) and both UI locales for NSIS and WiX (`English`/`SimpChinese`, `en-US`/`zh-CN`). A release build produces `target/release/fluxy.exe` (10.5 MB) with FileVersion 0.2.0 and the configured company and copyright, and `bun x tauri build --bundles nsis` produces `Fluxy_0.2.0_x64-setup.exe` (3.4 MB) while `--bundles msi` produces one MSI per configured language (`en-US` 4.65 MiB, `zh-CN` 4.64 MiB); an administrative install of the MSI extracts the same 10.5 MB executable, so both formats carry the same payload. The package was installed silently into a temp directory and uninstalled again: install exited 0 and produced the executable and an uninstaller, uninstall exited 0 and removed the directory. It is unsigned, which is recorded rather than hidden: `docs/PACKAGING.md` documents the two Windows signing paths (store thumbprint or a sign command), the macOS certificate and notarization variables, the Linux checksum convention, the WebView2 bootstrapper behaviour, and the clean-machine checklist the plan's acceptance asks for. `scripts/package/windows.ps1` wraps the build and prints the version resource and artifact sizes so a release log carries the numbers. |
| M5-02 cross-platform regression matrix | The pipeline used to run the Rust gates on Linux only, which left the two platforms that ship a different installer largely unverified. `.github/workflows/ci.yml` now runs `cargo fmt --check`, `clippy -D warnings`, the whole test suite, and the decoder fuzz harness on `ubuntu-latest`, `macos-latest` and `windows-latest`; the web job stays on Linux because the frontend is platform-independent; and a new on-demand `bundle` job runs `tauri build --no-bundle` on each platform so the packaging chain is exercised without certificates or installer toolchains. One ordering detail was needed for the matrix to be meaningful: `tauri-build` embeds the frontend during the Rust build, so every leg installs Bun and builds the frontend before the first cargo command. `docs/RELEASE_MATRIX.md` is the matrix itself - one row per check, one column per platform, each marked local, CI, or manual, with the command to reproduce it - plus the platform-specific areas to watch (listener pinning, tray, login item, clipboard, notifications) and the release gate. On this Windows host the matrix rows are green locally, and `cargo check -p fluxy-proto -p fluxy-core --target x86_64-unknown-linux-gnu` passes, which type-checks the protocol, file, transfer, batch, and path-safety layers against the Linux standard library; `fluxy-net` cannot be cross-checked from here because its `ring` dependency needs a Linux C toolchain, which is why the CI Linux leg exists. |
| M5-05 documentation and release notes | The release documentation is now written for its three audiences instead of one. A user guide (`docs/USER_GUIDE.md`) walks through first run, pairing with the six-digit code, chat, the three clipboard policies and the auto-send switch, files and folder batches, the transfer center, every settings section including the network-interface panel, tray behaviour, a troubleshooting table keyed on symptoms and error codes, and what Fluxy does with data. `docs/KNOWN_ISSUES.md` separates what blocks a signed release (unsigned installers, the advisory check that needs network) from product limits (a folder of only directories cannot be sent, the 10,000-entry cap, symbolic links refused, one listener at a time, a fixed receive directory, no automatic message retry, a non-persisted pairing lockout), the loopback performance gap, and the platform notes, and it names the checks that need a second machine. `docs/RELEASE_CHECKLIST.md` is 30 numbered gates in five phases - code, build, manual verification, documentation, tag - each with its command and expected evidence, plus a sign-off table. `docs/RELEASE_NOTES_0.2.0.md` is the public-facing note: highlights, install, known issues, compatibility, and the implementation table. The README gained a documentation index that links all fifteen documents, and a new `bun run check:docs` step (`scripts/check-docs-links.mjs`) parses every relative Markdown link in the README and docs and fails on a broken target, so the index cannot rot; it runs in CI next to the security scan. |
| M5-06 release candidate soak and release | The soak harness (`scripts/soak/soak.mjs`, `bun run soak`) pairs once through the real CLI, then loops chat plus an 8 MiB transfer, verifying every received file by SHA-256 and sampling the server's resident memory; it writes one JSON object per round to `.soak/log.jsonl`, a `summary.json` at the end, and stops after three consecutive failures. Its first real run found two defects in the CLI receiver it drives, which is exactly what a soak is for: `serve` printed incoming offers but never accepted them, so every send timed out after the offer window (now `serve --accept-files`, with the desktop app still asking the user), and sessions outlived their peer because the accept loop only takes the next connection after the current session ends while a scripted client that exits right after `close` never sends the QUIC close frame (now a 10 s idle session timeout on the server and a 150 ms wait after `close` on the client; rounds went from an occasional 9-21 s to a steady 0.3-0.6 s). Release hygiene is scripted as well: `scripts/release/verify-versions.mjs` (`bun run check:versions`) asserts that the workspace version, the four internal dependency versions, `package.json` and `tauri.conf.json` agree - they do, at 0.2.0 - and the installer build writes `target/release/bundle/SHA256SUMS.txt` next to the artifacts. This build produced `Fluxy_0.2.0_x64-setup.exe` (3,448,395 bytes), `Fluxy_0.2.0_x64_en-US.msi` (4,874,240) and `Fluxy_0.2.0_x64_zh-CN.msi` (4,870,144), with SHA-256 values recorded in the release notes. |
| Receive directory setting | The plan's settings page lists a download location, and until now it was fixed at `<data_dir>/received`. `AppConfig.receive_dir` now persists a chosen directory (serde-defaulted, so an older config keeps the default), `Engine::set_receive_dir` creates it and refuses a path that is not a directory, and `TransferRuntime` keeps the root behind an `RwLock` so the next transfer uses the new directory while a transfer already writing keeps its open sink. `set_receive_dir` and `pick_receive_dir` are exposed to the WebView, the settings panel shows the current directory with a folder picker and a reset action, and `SettingsSnapshot.receive_dir` carries the value the runtime is actually using rather than the stored preference. |
| A-01 Android build, signing, 16 KB pages | The Android track now has a build. `bun x tauri android init` generated `src-tauri/gen/android` against JDK 25 (Android Studio's JBR), Android SDK 37.2 with build-tools 36 and NDK 27.0, and `scripts/android/build-apk.mjs` (Node, so the same command works on Windows, macOS and Linux) builds, then verifies the two release-critical properties: every shipped `.so` has 16 KB-aligned LOAD segments and the APK's signer is printed. Four blockers had to be fixed first, all of them real cross-platform defects rather than tooling noise: `arboard` has no Android backend (the desktop dependency is now target-gated and the clipboard returns a clear error instead of failing to compile), the tray/menu wiring and the two folder pickers are desktop-only APIs (now behind `cfg(desktop)`, with the pickers answering 4601 on mobile), and `build.rs` embedded a Windows resource `.lib` whenever the *host* was Windows, which the Android linker refused (it now keys on `CARGO_CFG_TARGET_OS`). The first build produced 4 KB-aligned libraries, so `.cargo/config.toml` now passes `-Wl,-z,max-page-size=16384` for every Android target and the check reads `LOAD align 0x4000`. Result: `app-universal-debug.apk` (272,068,744 bytes, arm64-v8a, signed with the Android debug certificate) with `libfluxy_lib.so` reporting `0x4000` on all four LOAD segments. What is still manual: installing on a device or emulator, and a release keystore. |
| A-02 plugin skeleton, Keystore, and identity | The mobile plugin exists as a workspace crate, `plugins/tauri-plugin-fluxy-mobile`, wired the way Tauri 2 expects: `build.rs` declares the commands and the `android` path, and `setup` registers the Kotlin class with `api.register_android_plugin("com.thep0y.fluxy.mobile", "FluxyMobilePlugin")` so the Rust commands have a handle. `KeystoreSecrets.kt` keeps an AES-256-GCM key in the Android Keystore and writes `[version][iv][ciphertext]` to `dataDir/fluxy-identity.bin` with an atomic rename; `secretGet`, `secretSet` and `keystoreAvailable` are `@Command` methods, and the desktop build returns `None` or a clear unsupported error rather than pretending. The identity now actually goes through it: path resolution moved into the setup hook (Android only knows its files directory through the activity, and the old `AppPaths::resolve()` panicked with "no OS data directory" on device), the Kotlin constructor decrypts the sealed copy to `identity.json` before Rust reads it synchronously, and a post-start task seals the JSON back into the Keystore and deletes the plain file. Verified on an Android 37 x86_64 emulator with two cold starts: `identity sealed device_id=01a17795-cf51-75f5-b309-f95b79494632` both times, `fluxy-identity.bin` present and `identity.json` gone after each launch. One Android finding for A-05: SELinux denies `default-net` its `/proc/net/arp` and `sysfs_net` reads, so interface and gateway discovery needs the NsdManager port rather than the desktop path. |
| A-03 SAF descriptor bridge (mostly done) | The send side works end to end in code. `fluxy-core` adds `files/android.rs`: `register_descriptor(uri, file, name, mime)` takes a descriptor the platform opened, every read is a positional `read_at` so workers never share a cursor, `fstat` drives `identity()`/`stat()` so a changed source still returns 3002, a hostile display name falls back to `file-<size>`, and receiving keeps the desktop sink because the sandbox is a real path. Four tests cover offset reads and bounds, the name fallback, identity drift, and a reopened descriptor resuming from a saved range. The trait grew a default `register_descriptor` so a `dyn FileAccessManager` can still be asked; `TransferRuntime` now holds `RwLock<Arc<dyn FileAccessManager>>` and `set_manager` installs the Android implementation from the setup hook. On the plugin side `SourceBridge.kt` opens the URI with `openFileDescriptor`, persists the grant with `takePersistableUriPermission`, queries `OpenableColumns`, and detaches the descriptor; `fs_open_source(uri)` is the resume entry point. Verified on the Android 37 emulator: `android file manager installed` and `identity sealed ... device_id=01a17795-cf51-75f5-b309-f95b79494632` on every launch, so the manager swap did not disturb identity. Still open: the picker (needs the AndroidX Activity Result contract, which lands with A-07), a UI path to call `fs_open_source` from, and a real 10 GiB resume run. |
| A-04 receive location and MediaStore export (partial) | Receiving now has an Android shape. The plugin computes the preferred receive directory (`getExternalFilesDir(DIRECTORY_DOWNLOADS)/Fluxy`, falling back to `filesDir`), writes it to `receive-dir.txt` before the setup hook runs, and the shell points `TransferRuntime` at it: `android receive directory set path=/storage/emulated/0/Android/data/com.thep0y.fluxy/files/Download/Fluxy` in the emulator log, with the marker and the directory both present. `ExportBridge.kt` copies a finished sandbox file into `MediaStore.Downloads` inside an `IS_PENDING` entry and only clears pending after the copy succeeds, deleting the entry on failure so no half-written download is visible; it also reports `StatFs` free bytes for the pre-accept check. Both are exposed as `fs_export(path, name, mime)` and `storage_available()`. Not done: the shell does not yet expose export to the WebView, exports are not queued in `exports/[transfer_id].json`, there is no cleanup of exported sandbox files, the `StatFs` result is not enforced before an offer is accepted, and no 10 GiB receive-and-export run has happened. |
| Mobile document picker (A-07 blocker) | The picker was the one thing standing between A-03's bridge and a usable send path, and it now works. Tauri's `Plugin` base class has no `onActivityResult` hook, so the plugin registers `ActivityResultContracts.OpenDocument` with `activityResultRegistry.register(...)` instead - a key-based registration that needs no lifecycle owner and therefore survives being set up after the activity was created. `fs_pick_source` launches it, the callback persists the read grant, opens the descriptor, and resolves the same `OpenedSource` shape the resume path returns. Two defects surfaced while proving it on the emulator: the plugin module was missing `androidx.activity` (the compiler reported the package as an unresolved reference, which read like a Kotlin scoping bug), and a dismissed picker sends only `{cancelled: true}`, which failed to deserialize into a struct with required fields, so every field is now `#[serde(default)]`. Verified headlessly: a debug-only probe opens the picker when `probe-picker.txt` exists, the emulator log shows DocumentsUI as the resumed activity, and cancelling ends with `picker probe: cancelled`. The last hop is closed too: registration now happens once, at pick time, and the send path never touches a path again. `RegisteredFile` carries a `FileHandleId` plus an opaque `source_key` (the path on desktop, the document URI on Android), `SourcePlan` carries the same pair, `build_manifest` opens the handle and keys the manifest cache through `ManifestCache::key_for_source`, and batch entries register their handle when they are queued. `register_descriptor_source` takes a descriptor plus URI and produces the same view, and `pick_source_file` is the Android command that turns a picker result into a sendable file (ownership of the descriptor passes to Rust through `File::from_raw_fd`). Desktop behaviour is unchanged - 308 tests still pass, including the transfer suites - and the emulator still starts cleanly with the manager and receive directory installed. Remaining: the WebView has no mobile entry point yet, so the command has no caller. |
| A-07 first slice: Android listener, manual pairing, mobile send page | Verified end to end on the emulator: the page renders as the Android home, a tap on "Choose file" opens the system picker. The mobile track now has an entry point, and the emulator can bind. `choose_binding` returns a wildcard binding on Android (the desktop interface table is unreadable there), `reconcile` short-circuits so the listener never moves, and the emulator really listens: `/proc/net/udp` showed uid 10235 on `0.0.0.0:44109`, with pin failure reported as a warning rather than an error. Discovery cannot cross the emulator NAT, so pairing gained a manual path: `NearbyPeer::manual(address, fingerprint)` plus `pair_manual` let a user type an address and a 64-hex fingerprint; the rest of the handshake is unchanged. The WebView gained `EngineStatusView.platform` so the UI can pick a layout, and a new `MobilePage` at `/send` that pairs by address, shows the six-digit code for comparison, picks a file through the platform picker, and queues a send to a trusted device; `pick_source_file` lost its Android-only gate so the bindings are stable across platforms. Frontend checks pass (91 tests), and the Rust core crates pass (183 tests); the shell crate's own tests could not run because the user's running desktop app holds `target/debug/fluxy.exe`. |
| Mobile shell polish: real port and system insets | Two things a phone noticed immediately. The status now reports the port the listener actually bound: `Engine` keeps a `bound_port`, the network runtime records it right after `server_endpoint_bound` and `start_network` publishes a snapshot once the bind settles, so the page reads `Fluxy device · 60918` instead of `· 0`. System bars went through the platform, not CSS: Android 15 draws edge to edge, and the fix is to let the activity content view consume `WindowInsets` and pad itself (`activity.findViewById(android.R.id.content)` plus a `setOnApplyWindowInsetsListener` that calls `setPadding`), so the page lays out in a smaller viewport and needs no safe-area variables at all. A first attempt pushed the heights into CSS variables from the WebView; it was the wrong layer, and padding the WebView alone left its parent edge to edge. Verified on the emulator: the topbar now sits below the status bar, the bottom nav above the gesture bar, and the port reads 60918. |
| Status bar matches the page | The status bar used to be a themed strip in a colour the page never uses. The window behind it is now painted from the stylesheet's own `--fx-bg`: `Chrome.kt` sets the window background and derives the icon contrast from the same colour (relative luminance, so light backgrounds get dark icons and the reverse), and a `setChromeColor` command carries the value from the page. The colour is read through `getComputedStyle` rather than hard-coded, so a theme change moves the page and the bar together; the shell triggers it from `theme.resolved()`, with a microtask delay because the stylesheet applies after the effect runs. Verified on the emulator in both themes: light shows the pale page colour with dark icons, dark shows `#0b1418` with light icons, and the port reads 37062 in both. |
| A-05 first block: platform network facts | Android hides the interface table and the default route from apps (the SELinux denials recorded earlier), which is why the fallback probe ran with `gateway=None`. `NetworkInfo.kt` now answers the question the OS refuses: the active network's default-route gateway, its interface name, whether the transport is Wi-Fi, and the link addresses, gated on `ACCESS_NETWORK_STATE`/`ACCESS_WIFI_STATE` in the plugin manifest. `NetworkRuntime` holds the plugin handle behind a `OnceLock` and asks it for the gateway on Android before starting `FallbackDiscovery`; desktop keeps `default_gateway()`. The emulator still binds (0.0.0.0:50586 in `/proc/net/udp`), and the app starts clean. Not verified: whether the gateway value actually reaches the probe - the log line that used to say `gateway=None` no longer appears at all, so the next step is a device log or a temporary trace. Not done: the NsdManager side of A-05 (service registration and browsing) and the local-network permission prompt. |
| A-05 discovery: gateway verified, NsdManager wired | Two things moved. First the gateway: the plugin was returning `fe80::2` (a link-local IPv6 default route), which the IPv4-only fallback probe could not use, so `NetworkInfo.kt` now prefers an IPv4 default route. A trace proves the whole path: `platform network facts gateway=Some("10.0.2.2") interface=Some("wlan0") wifi=true` followed by `starting the fallback probe gateway=Some(10.0.2.2)` - the probe finally has somewhere to broadcast. Second the NsdManager half: `Discovery.kt` registers `_fluxy._udp.` with the same TXT records the desktop advertises (device, fingerprint, platform, name), browses, resolves, and exposes the peers; the shell polls it every two seconds and feeds `upsert_nearby` through a new `DiscoverySource::Nsd`, with fingerprints parsed and malformed ones dropped rather than shown with a broken pin. On the emulator it reports `NsdManager refused to start discovery`: the emulator's virtual Wi-Fi does not give the app a working mDNS resolver, so that half needs a phone. |
| Cross-platform build scripts | The Android build and the security audit were PowerShell scripts, which made `package.json` unusable on macOS and Linux even though the product targets all three. Both are now Node: `scripts/android/build-apk.mjs` discovers the JDK, the SDK, the NDK, the LLVM toolchain and a `build-tools` version that actually ships a signer (37.0.0 does not), builds through `bun x tauri android build`, checks every shipped `.so` for `0x4000` LOAD alignment, and prints the signer; `scripts/security/audit.mjs` runs the same deny/fuzz/unsafe inventory with `--fuzz-iterations` and `--skip-advisories` flags. The data comes from cargo's own output, not from unzipping the APK, so there is no platform-specific archive step left. `package.json` now reads `node scripts/...` for both, `bun run android:build` works on any OS, and the two `.ps1` files are gone. Verified end to end: 277,808,357 bytes, `LOAD align 0x4000 OK (16 KB)`, `CN=Android Debug` signer printed. |
| Android release signing | A release APK used to install nowhere: the Gradle project Tauri generates has no `signingConfigs`, so `tauri android build --apk` emitted `app-universal-release-unsigned.apk` and a phone answered that the package is invalid. The fix has three parts. `bun run android:keystore` (`scripts/android/keystore.mjs`) generates a 2048-bit RSA key valid for 10,000 days into `.android-secrets/fluxy-release.jks` and writes `src-tauri/gen/android/keystore.properties` with `storeFile`, `password` and `keyAlias`; both are gitignored and the password can come from `FLUXY_KEYSTORE_PASSWORD`. `app/build.gradle.kts` gained a `signingConfigs` block that reads those keys and a release build type that uses it only when the file exists, so a checkout without a keystore still builds debug. `android:build --release` refuses to start when the properties file is missing and refuses to finish if the APK still comes out unsigned. Verified end to end: the x86_64 release APK (13,252,981 bytes, `CN=Fluxy, OU=Release, O=Fluxy, C=US`) installed on the emulator with `adb install` and started, and the arm64 build reports the same signer. |
| A-05 discovery works, and the phone stops asking for an address | Three separate defects kept the nearby list empty, each enough on its own. First the permission: Android 16 gates the LAN behind `ACCESS_LOCAL_NETWORK`, a runtime permission, and without it `NsdManager.registerService` throws `"Missing local network permission"` - the plugin now declares it, asks for it when the webview loads, and the runtime retries `discovery_start` every five seconds so an answer that arrives after the task started still counts. Second the keys: the Kotlin side was looking for `device`/`fingerprint`/`platform`, but `discovery.rs` publishes `id`/`fp`/`os`/`v`/`features`, so every peer resolved and was then discarded for having no id. Third the UI: the page only ever listed `trusted` peers, so a discovered device could not be seen or acted on. `nearby_peers` now returns the discovery table with a `trusted` flag, and the page shows `附近的设备` with a 配对 button, polls it every 2.5 s, and keeps the address form collapsed behind `找不到设备？手动输入`. Verified on the emulator: `NsdManager discovery started` and `browsing _fluxy._udp.` after granting the permission, and the page renders the nearby section with the manual fallback folded away. Still to confirm on two real devices: that a phone and a desktop actually see each other, which the emulator's NAT cannot demonstrate. |
| A-05 verified on a phone: discovery, pairing request | With a real device on the same Wi-Fi the remaining layers showed up. The permission check was first: `ACCESS_LOCAL_NETWORK` does not exist on every API 36 build, and `checkSelfPermission` reports DENIED for a permission the platform never defined, so discovery refused to start forever; `LocalNetwork.granted` now asks the package manager whether the permission exists and lets an undefined one through. Second the address: the desktop advertised its link-local `fe80::` address, NsdManager prefers the AAAA record, and a phone resolved `fe80::4cea:662b:45e:bf26%wlan0` - an address it cannot dial and cannot even parse. `is_link_local` now keeps link-local IPv4 and IPv6 out of the advertisement (falling back only when nothing else exists), and the plugin strips the `%scope` suffix. Verified on a Titan_2 running Android 16: the page lists `THEPOY` with `windows` and a 配对 button, and tapping it reports 已发送配对请求，等待对端确认. A unit test covers the filter. Remaining: the desktop side of that pairing still needs a person to compare the six-digit code, which is the whole point of the check. |
| ~~Reverse discovery: phone to desktop does not work yet~~ | With the phone visible to itself the other direction was tested and does not work: the desktop logs neither an mDNS nor a UDP fallback discovery for the phone, while the phone finds the desktop. The asymmetry is in the network, not in the code: `ping` from the desktop to the phone (192.168.1.10) succeeds 3/3 while the phone to the desktop (192.168.1.8) loses 100% (ICMP being blocked is expected, so that alone proves nothing), and the phone's pairing request never reaches the desktop. Windows Firewall has `fluxy` inbound rules for this exact executable, profile Public, action Allow, UDP/TCP any port, and the network is Public, and no third-party security product is installed. Two changes were made on the app side while investigating: the plugin now holds a `WifiManager.MulticastLock` for the lifetime of discovery (without it the Wi-Fi chip drops multicast and broadcast frames addressed to the app, which the UDP fallback needs), and both discovery channels log what they find. What still has to be ruled out is a Windows-side block that the rule list does not reveal, or client isolation on the access point. |
| Reverse discovery fixed: the phone never broadcast | The desktop could not see the phone because the phone's UDP fallback never started, and the reason was a single wrong value: `FallbackConfig.interface` was set to the interface name from the selection, and on Android that selection is a wildcard whose name is `any` - not a device. `FallbackDiscovery` tried to bind to `any`, failed, and the phone advertised nothing, while its own mDNS half lives in NsdManager and is not visible to the desktop's `mdns-sd` socket (Windows also has `MacmDNSResponder` and Brave holding 5353 on different addresses, so the multicast path is crowded). Android now passes `interface: None` and lets the OS choose. Both directions were then verified in one run: the desktop logs `UDP fallback discovered a peer peer=Fluxy device platform=android` and the phone logs `UDP fallback discovered a peer peer=THEPOY platform=windows` within seconds of starting. Two diagnostic gaps were closed too: `mDNS discovery is unavailable` and `UDP fallback discovery could not start` are now logged, where before the errors were stored and never printed. |
| Mobile pairing: stable address, a deadline, and it reaches the desktop | Three reports fixed together. The address flipped between IPv4 and IPv6 because `upsert_nearby` replaced a device's record wholesale: mDNS and the UDP fallback each saw a different subset of addresses, so whichever reported last became the one on screen, and when that was the IPv6 one the pairing connection went out on a path that did not work. The record is now merged (union, sorted, deduplicated) before it is stored, so the first address is the same on every refresh and `nearby_peers` always picks the IPv4 one first. The pairing request that never arrived was the same defect: with a stable IPv4 address the QUIC connection succeeds, and the phone shows the six-digit code. The waiting state now has both ways out it lacked: the existing server-side TTL renders as `01:53 后失效`, and the page cancels an unanswered request after 30 seconds with a 取消配对 button for the rest. Verified on the phone: the page lists `THEPOY` at `192.168.1.8:61004`, and tapping 配对 opens the code dialog with `622 654`, the countdown, and the acknowledgement checkbox. |
| Pairing reaches the desktop, but only after the initiator confirms | Tracing the request end to end showed the desktop does receive it: `incoming pairing connection accepted` followed by `pairing code is ready; waiting for the user challenge_id=01a17bbc-…`. The order in `server_pair_attempt` explains the confusion: the server reads `PairRequest` (which carries the initiator's acknowledgement) before it calls the confirm callback, so a desktop that has not been confirmed shows nothing at all while the phone already says 请在另一台设备上确认. Two real defects were fixed along the way. The port drifted: a resolver caches the port a previous instance advertised, and that stale value overwrote the port the UDP probe hears every two seconds, so the phone dialled 61765 while the desktop listened on 61007 and the connection timed out; the probe's port now wins when the sources disagree, and the page showed `192.168.1.8:61438` matching the desktop exactly. The waiting state also gained a 30-second deadline and a 取消配对 button, so an unanswered request no longer sits forever. Fixed in the protocol layer: `server_pair_async` now takes an `on_code` observer that runs right after the SAS is derived and before `PairRequest` is read, so the responder raises `PairingEvent::SasReady` at the same moment the initiator shows its six digits. The runtime creates the challenge in that observer and only awaits the user's answer in `confirm`, which keeps a single challenge per attempt. Verified on the phone and the desktop in one run: `incoming pairing connection accepted` at 05:12:29.465 and `pairing code is ready` at 05:12:29.488, 23 ms later, with the phone showing 866 243 at the same time. |
| Mobile messages tab | The phone had no way to read a message sent from the desktop: the bottom bar only offered 设备/传输/设置, and the chat workspace lived on the desktop home page. `mobileNav` now has a fourth tab 消息 pointing at `/chat`, rendered by a new `ChatPage` that reuses `ChatWorkspace` unchanged - the same conversation list, thread, acknowledgement marks and composer the desktop uses. On a narrow screen the workspace stacks instead of using two columns: the conversation list takes 38vh and scrolls, the thread and composer share the rest, and the composer and list rows get the 44px touch height. Verified on the phone: the tab bar reads 设备/消息/传输/设置, the 消息 tab lists THEPOY as 已连接, and a message typed there shows 已送达 in the thread, so the chat session is live in both directions. Messages are in-memory by design (the page says 消息只在内存中保留), so anything sent before the session was established is not replayed. |
| Chat carries files, and the thread scrolls | Two complaints, one about where files live and one about the layout. The composer now has an 附件 button next to 发送: it opens the platform picker (desktop `pickFile`, phone `pickSourceFile`) and queues the transfer for the open conversation, so sending text and a file no longer means changing pages. On the phone the picker opens (verified: `documentsui/.picker.PickActivity` becomes the resumed activity) and the transfer then appears in 传输 as before. The layout needed three attempts: the card had no bounded height, so a long thread was clipped instead of scrolled, the conversation list was pinned to a fixed share of the screen and cut its own row in half, and a flexible composer ate the thread. The card now takes `calc(100dvh - 300px)` explicitly, the list sizes to its content with a 32vh cap, the thread is the only scrolling region, and the composer is pinned to one 44px line. |
| Attachment picker on Android: the platform decides, not the IPC bridge | Choosing a file in the phone's conversation failed with `URL is not a valid path`. The cause was the branch in `ChatWorkspace.attach`: it asked `engine.desktop`, which is `\"__TAURI_INTERNALS__\" in window` - and that bridge exists in the Android WebView too, so the phone took the desktop path and asked the desktop-only `pick_file` command, which resolves a paths-only dialog the platform does not have. The branch now keys on `status.platform === \"android\"`, the same signal the router uses. Traced on the phone: `launching the document picker` -> `picker returned uri=content://…` -> `registering a picked document name=头酒案.md size=1130` -> `document registered file_id=01a17c44-…` -> `transfers queued count=1`. The plugin and the shell also log each step now, which is how the wrong branch became visible. |
| Transfer speed display and throughput measurement | The number in the UI was wrong for a mechanical reason: `SpeedMeter::sample` ran on every progress tick, progress arrives once per 4 MiB chunk, and two chunks on a fast LAN can land a couple of milliseconds apart - so a 4 MiB chunk divided by 2 ms reported gigabytes per second, then dropped as the smoothing caught up. Samples closer than `MIN_SAMPLE_SECONDS` (250 ms) now keep the previous rate instead of inventing a new one. `complete` also logs `file, bytes, elapsed_ms, mib_per_s`, so the real figure can be read from a log rather than estimated from an animation. What the log has not shown yet is a completed phone-to-desktop run: the phone queues the transfer (`transfers queued count=1`) but the desktop logged no incoming request during this session, so the 200 MB / 10 s figure is still unmeasured on my side and the throughput half of this item stays open. |
| Peer address drift breaks transfers after either side restarts | A 200 MB send from the phone failed with `timeout / 2001` and zero bytes moved. The desktop rebinds its QUIC socket on every start, so the address a chat session was created with goes stale while discovery keeps learning the current one. Two places kept the dead address. `ChatRuntime::update_peer_addr` now rewrites the session entry whenever discovery reports a new port (observed: `peer moved old=192.168.1.8:63085 new=192.168.1.8:50736` on the phone and the mirror image on the desktop), and `run_outbound` reads the address back from the registry instead of trusting the copy handed to it at session creation. `SESSION_TIMEOUT` also went from 10 s to 30 s, because a session recovering from a dropped connection could not re-establish inside ten. The send still does not complete: the phone queues the transfer (`transfers queued count=1`) and the desktop logs no inbound connection at all, so the QUIC dial itself is not arriving. `end_session` now logs its reason (`chat session ended`) to make the next run diagnosable. Throughput remains unmeasured. |
| A pinned listener port, and the handshake that still fails | The QUIC listener asked for port 0, so every restart moved it and invalidated every address a peer had learned; a transfer started right after a restart therefore dialled a socket nobody was listening on. `DEFAULT_LISTEN_PORT = 49784` pins it, and `load_or_default` upgrades a saved 0 to that value without a version bump. With the port stable the desktop now logs inbound connections again (`incoming pairing connection accepted` five times inside 200 ms, because the phone retries immediately). That exposed the next layer: the phone ends with `chat connect failed: control stream failed: connection lost` and the desktop with `chat session ended reason=control stream closed` - the TLS and resume handshake complete, then both sides wait for the control stream and the connection is torn down. Reusing a dead session sender is also handled now (`ensure_session` drops an entry whose sender is closed instead of handing it out), and `IDLE_TIMEOUT` went to 60 s so one late heartbeat cannot end a session. Throughput is still unmeasured: no byte has crossed yet. |
| The inbound accept loop can stop without saying so | With the port pinned at 49784 the phone completes the QUIC handshake (`dialling the peer -> quic handshake completed`, 25 ms) while the desktop logs no inbound connection at all - the two observations only reconcile if `spawn_accept` is no longer looping, so handshakes complete at the endpoint level and nothing ever calls `handle_incoming`. The phone then fails in `client_handshake` with `connection lost` and retries every 30 ms. Instrumented this round: `connect_trusted` logs dial / quic handshake / resume handshake / control stream so the exact stop point is visible, and `end_session` already reports its reason. Verified this round: fixed port 49784 is actually the listener (`Get-NetUDPEndpoint` shows it owned by fluxy), tests pass at 205, and the speed meter fix from round 1 still holds. Throughput remains unmeasured. |
| The accept loop is alive; the resume handshake is where it dies | Adding a start-line to `spawn_accept` settled the previous round's contradiction: the loop is running and accepts connections again (`the accept loop is running`, then `incoming pairing connection accepted` within three seconds, and five on a retry burst). It also stops instead of spinning when the endpoint closes, which is how it would have gone silent before. The dial now gets further than the logs suggested: `dialling the peer` -> `quic handshake completed` (25 ms), the handshake stream opens (no `could not open the handshake stream` warning, which also reports `close_reason`), and the failure is inside `client_handshake` on a subsequent read or write - the `resume handshake completed` line never prints and the phone reports `control stream failed: connection lost`. So a connection exists, a bidirectional stream opens on it, and then the stream itself dies before the resume exchange finishes. Throughput is still unmeasured. |
| A dead session could veto every reconnect forever | `close_reason` finally named the killer: `ApplicationClosed { reason: b"chat-session-active" }`. The tie-break in `ChatRuntime::accept` lets the lexicographically smaller device keep its outbound session and reject inbound ones - but it only checked `sessions.contains_key`, not whether that session was alive. After a heartbeat failure the desktop still held the entry, so it rejected the phone's inbound connection while its own session was already dead: neither side could ever reconnect. The check now requires an incumbent whose worker is still running (`!entry.sender.is_closed()`). With that, the phone gets through the whole exchange for the first time: `hello acknowledged` -> `capabilities exchanged` -> `control stream open addr=192.168.1.8:49784`. What remains is the other side of the same handshake: the desktop logs `chat accept failed: control stream failed: timed out` 22 s later, so its `ControlSession::accept` never sees the stream the phone opened. Throughput still unmeasured. |
| Both directions connect now, and the control stream is slow | After the dead-session veto was fixed, both sides complete a session: the phone accepts the desktop's stream (`waiting for the peer control stream remote=192.168.1.8:52664` then `peer control stream accepted`) while the desktop opens its own (`control stream open addr=192.168.1.10:49784`). Two observations from the same run. First, accepting the stream took ten seconds (18:29:24 -> 18:29:34) for a connection that was already established, which points at stream delivery rather than the network. Second, the phone queues the 200 MB send (`transfers queued count=1`) and the desktop records no offer, so `send_inner` is still stuck before the offer goes out - either `wait_for_session` is waiting on a session entry that never flips to connected, or `build_manifest` is still hashing with nothing logged around it. The logging added this round narrows the next step to those two. Throughput still unmeasured. |
| The send path is not slow; the offer dies on the wire | Instrumenting the sender ruled out the two suspects from last round. `wait_for_session` returns in under a second (`waiting for a chat session` at 18:35:16.355, `chat session acquired` at 18:35:17.066) and `build_manifest` takes 1-2 ms for 200 MB because `ManifestCache` already holds the digest - hashing is not the bottleneck. The offer then goes out: `manifest ready; offering the transfer bytes=209715200 elapsed_ms=2` followed by `offer sent; waiting for the peer decision`. The desktop logs six `incoming pairing connection accepted` lines in the same window and not one transfer event, so `TransferMessage::Offer` is written to a control stream the desktop never reads - the sender and the receiver are looking at different connections. That is now the single remaining gap before any byte is measured. |
| The pinned port works; the client still holds the old one | The listener is genuinely fixed: `Get-NetUDPEndpoint -LocalPort 49784` resolves to the running fluxy process, and the 62492 seen earlier was a client-side socket, not the server. The client is the problem: `peer moved ... new=192.168.1.8:49784` is logged at 18:48:01, yet the session that comes up at 18:48:11 reports `remote=192.168.1.8:55049` - the address from before the move. `run_outbound` now reads the registry copy instead of its captured peer, and `update_peer_addr` closes the live connection when the address changes (`peer-moved`), but a dial already in flight still lands on the stale port and the offer goes with it. Also worth recording: `received/头酒案.md` and `received/app-universal-debug.apk` exist on the desktop, so a transfer did complete at some point in this session - the path works, the address bookkeeping is what breaks it. |
| Redialling on a move, and the address race that survives it | `run_outbound` now re-reads the registry address, dials, and after the handshake compares the connection's peer with what the registry says; on a mismatch it closes the connection and redials (up to `REDIAL_ATTEMPTS` = 3), because a connection to the port a peer just left accepts writes and silently drops them. The redial never fired in the run that followed: `peer moved new=192.168.1.8:49784` at 18:53:06, `chat session live remote=192.168.1.8:51861` at 18:53:15, and no `the peer moved while dialling` line - meaning the registry entry the dial read already carried the stale address and the post-handshake comparison agreed with it. The entry is recreated from `known_peer`, which returns the peer stored on the session, so a recreated session inherits the address it was created with instead of the one discovery last reported. That inheritance is the remaining defect, and it is the reason nine rounds of tracing have not produced a single measured throughput. |
| Chat asks discovery for the address, and the last race left | `ChatRuntime` now owns an `AddrResolver` that the network runtime installs: it shares the `nearby` map (now an `Arc<Mutex<BTreeMap>>`) and returns the first discovered address plus its port. `ensure_session` rewrites the peer it is handed through that resolver, so a recreated session can no longer inherit the address it was first created with. The nearby list now reports the pinned port (`THEPOY windows 192.168.1.8:49784`, matching the listener) and both peers agree on it. What defeated this run is a narrower race: the session came up at 18:59:33 pointing at 55502 and the offer went out in the same millisecond, so the connection was established before the broadcast carrying 49784 arrived. `peer moved` then closes it, but the offer had already been written into the dead stream. Closing that window means not offering until the session's remote address matches the resolver's answer, which is the next concrete change. |
| The offer now reaches the right connection | `wait_for_session` compares the session's remote address with what `chat.current_addr` reports and refuses to send on a stale one: it logs `session address is stale; closing it` and closes the connection, because nothing else retires a session whose address stopped changing. With that, a run finally produced `chat session live remote=192.168.1.8:49784` followed by `offer sent; waiting for the peer decision` on the correct port. The transfer then ended at `timeout / 2001` after one minute: `OFFER_TIMEOUT` expires because the desktop never decides. The receiving side has no automatic acceptance - `pending_accept` waits for an explicit answer from the UI - and the desktop window could not be driven from this session. Historical evidence that the receiving path works: `received/app-universal-debug.apk` (277 MB, 17:17:50) and `received/头酒案.md` were delivered by this same code earlier today. Throughput remains unmeasured, now for a reason outside the transport: nobody accepted the offer. |
| Unattended receiving, and the address that still will not settle | `auto_accept_transfers` is a new config flag (default off, `#[serde(default)]` so older configs load) surfaced through `TransferRuntime::auto_accept`: when it is on, an offer is emitted for the UI and accepted immediately instead of waiting for a click, and the desktop config has it enabled. That removes the manual step that blocked the last round. The run then failed earlier: `session address is stale; closing it` repeated with `expected=192.168.1.8:49784` against `actual=192.168.1.8:54829`, then 61340, then 61012 - a new port on every retry while the desktop was verifiably bound to 49784 (`Get-NetUDPEndpoint -LocalPort 49784` -> fluxy). So the resolver reports the right address and the dial still lands somewhere else; the entry the dial reads is not the entry the resolver updates. Throughput remains unmeasured. |
| Transfers work, and the number is bad: 7.4 MiB/s | The stale-address loop was my own bug. `wait_for_session` compared `session.connection.remote_address()` with the peer's listening address, but an accepted connection reports the peer's ephemeral source port (55483, 55493, 55499 on successive attempts), so every healthy session looked stale and was closed. The check now looks at `close_reason` instead of the address, which is what actually matters. With that, the first measured end-to-end run: phone `offer sent` -> desktop `auto-accepting the offered transfer` -> desktop `transfer finished file=bigtest.bin bytes=209715200 elapsed_ms=26872 mib_per_s=7.44`, phone `elapsed_ms=39085 mib_per_s=5.12` (the phone counts from queue time, so it includes the wait). 200 MB in 27 s is about 62 Mbit/s - far below what a LAN should do, and consistent with what the user reported. The transport is now measurable, which is what the last twelve rounds were missing; the throughput work starts from a real number. |
| Bottleneck found: it is not the file read | Per-chunk timing on the sender settles the question this objective started with. `read_ms` is 1-17 ms for a 4 MiB block, so `source.read_at` on the Android document is not the limit; `total_ms` for the same block is 873-6616 ms, so nearly all of the time is spent in `stream.write_all` and `finish` - the QUIC data path. With `DEFAULT_STREAMS = 4` and `DEFAULT_WINDOW_PER_STREAM = 2` the scheduler keeps eight 4 MiB blocks in flight, yet the run still lands at 7.5 MiB/s (`elapsed_ms=26499 mib_per_s=7.55`), which works out to roughly 3 MiB/s per stream - about 24 Mbit/s - far below what cubic on a 1452-byte MTU should reach on a LAN. So the remaining suspects are the transport itself (congestion control, MTU probing, or the link's real capacity) rather than scheduling, chunk size, or hashing, all three of which are now measured and cleared. |
| The link is not the limit; a single stream is | `cmd wifi status` settles the baseline question: `Wi-Fi standard: 11ax`, `Frequency: 5220MHz`, `Link speed: 600Mbps`, `RSSI: -45` - a healthy 5 GHz Wi-Fi 6 link with roughly 71 MiB/s of theoretical capacity, so 7.5 MiB/s was never the radio. Two experiments followed. BBR instead of cubic (`congestion_controller_factory(BbrConfig)`) changed nothing: 7.13 MiB/s, within noise of the cubic run. Raising `DEFAULT_STREAMS` from 4 to 8 - the ceiling the protocol already negotiates - moved it to `elapsed_ms=21028 mib_per_s=9.51`, a 33% gain that scales with stream count rather than link capacity. The picture is now consistent: one QUIC stream sustains about 3 MiB/s on this path regardless of congestion control, and total throughput is that number multiplied by the number of streams. Streams are capped at 8 by `max_streams`, so the next gain has to come from the per-stream rate itself - plausible causes are the 1452-byte initial MTU not being probed upward, or the platform's UDP receive buffer. |
| The path is healthy; the congestion window is the ceiling | `connection.stats().path` on the sending side gives the missing number: `rtt_ms=6`, `lost_packets=0`, `congestion_events=0`, `cwnd=244221`. So the radio is fine, the RTT is a LAN RTT, and nothing is being retransmitted - but the congestion window is 244 KB for the whole connection, and with eight streams in flight that is roughly 30 KB per stream. At a 6 ms RTT that bounds a single stream to a few MiB/s, which is exactly the per-stream rate measured last round. Total throughput is therefore (cwnd / RTT) split across streams, not (link speed): 244 KB / 6 ms is about 40 MiB/s in theory, and the run reached 9.5 MiB/s at best (6.7 in this one). The remaining lever is the congestion window itself - a larger initial window, or fewer streams so each gets a bigger share - and that is now a targeted change rather than a hunt. |
| Release mode on the desktop: +3%, so the build profile is not the answer | `bun run dev -- --release` runs the desktop through the release profile while still using the Vite dev server, which is the cheap way to answer "is the debug build the reason". Same 200 MB file, same phone build: `elapsed_ms=20376 mib_per_s=9.82` against 9.51 in debug - inside noise. The desktop's receive path therefore costs almost nothing in debug, and the congestion window measurement from the previous round stands as the real ceiling. The sender (the phone) is still a debug build; testing that would need a signed release APK, and installing one over the debug build changes the signing key, which wipes the trust store and forces a re-pair that cannot be completed without someone clicking the desktop dialog. |
| One dev server for both ends | `beforeDevCommand` now runs `bun run dev:web`, a small probe that connects to 8563 on 127.0.0.1 and ::1 and reuses the server if either answers, starting Vite only when nothing is there. Before this, running `tauri android dev` while the desktop dev server was up meant a second Vite on the same port and a failed start - and the phone build is supposed to load the desktop's bundle over the LAN anyway. Verified: with Vite already listening the script prints `[dev-web] port 8563 is already serving; reusing it` and exits 0. Two more things surfaced while trying a release-profile phone build: `tauri android dev --release` failed to install because the release keystore signs with a different certificate than the debug build (`INSTALL_FAILED_UPDATE_INCOMPATIBLE`), and simply removing the keystore produced an unsigned APK instead, so `build.gradle.kts` now falls back to the debug key when no release keystore is configured. With signing fixed the optimised release build crashed its WebView renderer (`Renderer process (12794) crash detected`), so `optimization.enable` is off for dev runs; the Rust half is still compiled with the release profile. |
| Release on both ends: 30 MiB/s, a 3.2x change - the debug build was the answer | The question "is the slow speed the unoptimised build?" is settled. Same 200 MB file, same link, only the build profile changed: `elapsed_ms=6485 mib_per_s=30.84` and `elapsed_ms=6578 mib_per_s=30.40` on two consecutive runs, against 9.51 MiB/s with both sides in debug and 9.82 with only the desktop in release. The profile comparison was worth taking seriously: an unoptimised BLAKE3 and ring keep the sender's queue shallow enough that the congestion window never opens up. The path state tells the same story - `cwnd=244221` in debug versus `cwnd=4425399` in release (with `lost_packets=25485`, `congestion_events=82`), so the release build sustains enough in-flight data for the window to grow to its real size. 30 MiB/s is about 240 Mbit/s on a 600 Mbit/s link, which is a credible LAN number. Note the phone still runs with `optimization.enable = false` (R8 crashes its WebView renderer), so the gain comes from the Rust release profile alone. |
| A-04: the receive path now exports, records, and cleans up | The bridge existed but nothing reached it. `TransferRuntime` now holds an `AppHandle` (installed from the setup hook), and `complete` hands a finished receive to `export_received` on Android: it resolves the file's path from the offer, copies it into `MediaStore.Downloads` through `FluxyMobile::export`, writes one `exports/<transfer_id>.json` entry recording the URI and timestamp, then removes the sandbox copy. Every step is best effort - a failed export logs and leaves the received file in place rather than turning a completed transfer into a failed one. On the WebView side `exportFile` and `storageAvailable` are exposed through `src/api` (invoking `plugin:fluxy-mobile|fs_export` and `storage_available`) and forwarded by the engine store, with the test mock extended. Still open in A-04: the `StatFs` result is reported but not enforced before an offer is accepted, and no 10 GiB receive-and-export run has happened. |
| The window that vanished overnight | The desktop was left running through `bun run dev -- --release` and was gone by morning. The event log dates it precisely: `Microsoft-Windows-Kernel-Power` recorded 506 (entering modern standby) at 00:01:10 and 507 (waking) at 08:32:49, and the last application log line is 00:33:14 - a peer connection while still in connected standby. There is no crash entry in the Application log, so nothing panicked; `tauri dev` drives the app through the CLI's own pipe, and that pipe does not survive the standby transition, so the child is torn down with it. A standalone build has no such dependency: `bun run build` produced `target/release/fluxy.exe` plus MSI and NSIS installers, and launching the binary directly starts it on the same port (`49784`) with a real window handle. The lesson belongs in the notes rather than the code: run an overnight instance from the release binary, not from the dev command. |
| Received files land where the user can actually open them | The receive directory used to be `getExternalFilesDir(DIRECTORY_DOWNLOADS)/Fluxy`, which is `Android/data/<pkg>/files` - unreachable from a file manager on Android 11+, so every received file needed a second manual copy before it was usable. The app now declares `MANAGE_EXTERNAL_STORAGE` and writes into `/storage/emulated/0/Download/Fluxy` instead: `ExportBridge.receiveDir` prefers the public folder once `Environment.isExternalStorageManager()` is true and falls back to the private directory otherwise, so a declined grant still leaves a working app. The plugin requests the grant once (a marker file stops repeat redirects) via `ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION`. Verified on the phone: `android receive directory set path=/storage/emulated/0/Download/Fluxy` and the directory exists. Because the file is already visible there, `export_received` now skips the MediaStore copy when the path is under `/storage/emulated`, which would otherwise duplicate every transfer. Note for release: Play policy restricts `MANAGE_EXTERNAL_STORAGE` to file managers and backup tools, so a Play distribution still needs the SAF directory picker that the mobile plan describes; sideloaded builds can use this path directly. |
| The file still landed in Download/ - my skip shipped after the APK | After switching the receive root to `Download/Fluxy`, a real transfer still produced `/sdcard/Download/1.webp` with `Download/Fluxy` empty. The chain explains it: the receive did write into the public folder, then `export_received` copied it into `MediaStore.Downloads` - whose default relative path is `Download/`, not a subfolder - and finally deleted the source, so only the export copy was left. The skip I added was correct but the APK in hand predated it. Rebuilt and installed with the check in place; the guard now also distinguishes the private fallback (`/storage/emulated/0/Android/`) so a device without the all-files grant still exports to MediaStore rather than leaving an unreachable file. Verified: directory listing before the fix showed `/sdcard/Download/1.webp` and an empty `Download/Fluxy`; the APK now skips the copy for public paths. |
| A-06: a dataSync foreground service brackets every transfer | Without one the OS is free to freeze the app the moment the screen turns off, which kills a long transfer. `TransferService.kt` runs as a `dataSync` foreground service with an ongoing progress notification - its own channel, low importance, tapping it reopens the app - and stops itself after `TIMEOUT_MS` (six hours), just inside the Android 15 budget, so the platform never kills it mid-write. The manifest declares `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC` and `POST_NOTIFICATIONS` plus the `<service android:foregroundServiceType=\"dataSync\">` entry; Android 13+ refuses `startForeground` without the notification grant, so `foregroundStart` requests it the first time a transfer needs the service. On the Rust side `enter_transfer`/`leave_transfer` keep a refcount around `run_sender` and `run_receiver`, raising the service on the first active transfer and lowering it on the last, so overlapping transfers share one notification. Not yet verified on hardware: the phone dropped off wireless adb mid-run, so the install and the `dumpsys` check could not be completed. |
| A-06: the service is in the APK, the run is not | Wireless adb reconnected and the build installed. One defect surfaced and was fixed: the manifest declared `android:name=\".TransferService\"`, which resolves to `com.thep0y.fluxy.TransferService`, while the class lives in `com.thep0y.fluxy.mobile` - a name the system silently ignores, so nothing was registered. After correcting it to the full class name, `aapt2 dump xmltree` on the installed APK shows `<service name=\"com.thep0y.fluxy.mobile.TransferService\" foregroundServiceType=0x00000001>`, and `dumpsys package` reports `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC` and `POST_NOTIFICATIONS` all granted. The runtime half is unverified: the phone screen was off during the check, so the scripted taps never reached the transfer page and no service ever started. |
| A-06 runtime check blocked by test-file handling, not by the code | The service is correctly declared and installed (`aapt2 dump xmltree` shows `com.thep0y.fluxy.mobile.TransferService` with `foregroundServiceType=0x00000001`, and all three permissions are granted), and transfers do run: `transfers queued count=1` followed by `transfer finished`. What could not be observed is the service itself. The scripted picker taps kept selecting a 139 KB screenshot instead of the 105 MB archive, and a 139 KB transfer finishes in 119-167 ms - far too short for `dumpsys activity services` to catch a running foreground service, so every check returned empty. A 100 MB `.bin` was invisible to the picker entirely (MediaStore does not index unknown types); switching to `.zip` made it appear, but the tap coordinates landed on the row above it. The next attempt should select the large file by hand rather than by coordinates. |
| A-06 works: the service brackets every transfer exactly | Two manual transfers (105 MB each) show the foreground service tracking the transfer window precisely: `transfers queued count=1` at 22:26:54, `Foreground service start for UID: 10209` at 22:26:56, `transfer finished elapsed_ms=6664 mib_per_s=15.0` at 22:27:01 and `FGS stop call for: 10209` at the same second; the second run repeats the pattern (start 22:27:16, finish 4.7 s later, stop). Nothing crashed. One warning remains: the system logs `does not have any types` on both start and stop, meaning `startForeground` ran without a service type even though `FOREGROUND_SERVICE_TYPE_DATA_SYNC` is `1 << 0` (confirmed in the android-37.2 sources) and the manifest declares `foregroundServiceType=\"dataSync\"`. Functionally harmless today, but Android 15 tightened this and a future API level may reject it, so the branch in `onStartCommand` still needs a check. |
| The foreground type reaches the call and the system still says otherwise | Instrumenting `onStartCommand` settles where the value goes: `FluxyService: startForeground sdk=36 type=1` - the third argument is the dataSync bit and it is passed on the path that runs (SDK 36 clears the `Build.VERSION_CODES.Q` branch). The system's own logger then reports `Foreground service start for UID: 10209 does not have any types` 3 ms later, and `FOREGROUND_SERVICE_TYPE_DATA_SYNC` is `1 << 0` in the android-37.2 sources while the manifest carries the same `dataSync` flag (aapt2: `0x00000001`). Everything the app controls is consistent, so the mismatch is on the platform side of that logger - most likely how it derives the type for a `dataSync` service on this API level. Functionally the service is verified (start/stop bracket each transfer, transfers survive a locked screen), so this stays a warning to revisit if a future API starts rejecting it rather than a defect to chase now. |
| A-07: files can be shared into the app | The manifest now advertises `ACTION_SEND` and `ACTION_SEND_MULTIPLE` with `mimeType=\"*/*\"`, so Fluxy appears in the system share sheet. `MainActivity` captures the incoming URIs in `onCreate` and `onNewIntent` (the activity is `singleTask`, so a second share arrives through the latter) and leaves them in a process-wide `PendingShare` holder. The plugin drains that through `shared_files`, the shell re-exports it, and the send page registers each URI on mount via `openSourceFile`, appending it to the file list so the user only has to pick a peer - the same `register_descriptor_source` path the picker uses, so nothing new happens on the transfer side. Not verified on hardware yet: the build has to be reinstalled and a share issued from another app. |
| Share reached the plugin but three separate gaps stopped it | Simulating the intent with `am start -a android.intent.action.SEND --eu android.intent.extra.STREAM content://media/external/images/media/1000000221` made the debugging concrete. First the capability: `src-tauri/capabilities/default.json` granted only `core:default`, so every plugin command answered `fluxy-mobile.shared_files not allowed. Command not found` - the bridge had never been reachable from the WebView at all, which also means the earlier export and foreground calls were never exercised. Second, `build.rs` listed only five commands to `tauri_plugin::Builder::new`, so the permission files for `fs_export`, `shared_files`, `foreground_start` and the rest did not exist; all sixteen are listed now and `permissions/default.toml` ties them into one set. Third, `OpenedSource.cancelled` had no default, so `fsOpenSource` - which builds its payload field by field - failed to deserialize with `missing field cancelled`; the field is `#[serde(default)]` and the command also sets it. The security gate then rejected the widened capability, so `check-ui-security.mjs` now allows exactly `core:default` plus `fluxy-mobile:default` and still fails on anything else. Verified end to end: `[share] drained 1 uri(s)` then `[share] registered content://media/external/images/media/1000000221 -> IMG_20260203_140650_532.jpg`, and the file appears in the send list with the paired peer offered. Visible defect: the list renders name and size with no separator (`IMG_20260203_140650_532.jpg2635388`). |
| The send list prints a file's name and size as one word | The row rendered `<strong>{file.name}</strong>` next to a raw byte count, so `IMG_20260203_140650_532.jpg` and `2635388` ran together as a single string. Two changes: the size goes through `formatBytes` (2.5 MB) like the transfers page already does, and the row is a flex line with the name truncating and the size pinned to the trailing edge, so they cannot be misread as one token however long the name is. Verified on the phone: the row now reads `IMG_20260203_140650_532.jpg` on the left and `2.5 MB` on the right. |
| A-07 clipboard: the stubs are deliberate, the port does not exist | Checked what the mobile side has for the clipboard, and the answer is "nothing beyond a well-behaved refusal". On Android `clipboard.rs` routes every request to `run_unsupported_clipboard`, which answers `the system clipboard is not available on this platform` for both read and write - correct behaviour (a silent success would be worse) but no capability. `arboard` is only a dependency for `cfg(not(target_os = "android"))`, the Kotlin plugin has no `ClipboardManager` code at all, and the `ClipboardPort` trait the mobile plan designs (`docs/MOBILE_PLAN.md`) was never written. The route `/clipboard` and `ClipboardPage` do exist, but `mobileNav` lists only devices, chat, transfers and settings, so nothing on a phone links to the page. Net: the clipboard is a desktop-only feature today, and reaching it on Android needs three pieces - a Kotlin bridge for read/write, the port that `clipboard.rs` would consult before falling back to the refusal, and a nav entry. |
| A-07: the phone clipboard works, on demand only | `ClipboardBridge.kt` reads through `ClipboardManager` and reports an empty string for an empty clip; `clipboard_read` and `clipboard_write` are plugin commands, listed in `build.rs` so their permissions exist. On the Rust side `clipboard.rs` no longer refuses on Android: `run_unsupported_clipboard` became `run_mobile_clipboard`, which forwards each request to the plugin through `block_on` and still answers `the system clipboard is not available on this platform` until a handle is installed, so an early request fails loudly instead of pretending. The handle arrives once from the setup hook through a `OnceLock`, because the clipboard thread is spawned before the app exists. Deliberately manual: nothing polls, and `auto_clipboard` stays false by default - a phone clipboard holds passwords and one-time codes, and Android 10+ only allows a foreground app to read it anyway. The clipboard page is reachable from Settings through an ordinary anchor rather than `<A>`, which cannot render outside a `<Route>` and broke the settings test. |
| The clipboard section was invisible because the APK was six hours old | Following the phone to the bottom of Settings showed diagnostics and identity rotation but no clipboard panel. The cause was not the markup: `bun run android:build` had been failing since `lib.rs` called `fluxy::clipboard::set_app_handle` from inside the `fluxy` crate itself (`error[E0433]: cannot find module or crate `fluxy``). Fixed to `crate::clipboard::set_app_handle` and the APK timestamp moved from 09:42 to 11:50. The install step kept reporting `Success` throughout, because it was happily reinstalling the last good APK - so several earlier checks in this stretch were reading a stale build. Lesson recorded: after a build step, compare the APK timestamp, not just the install result. One open item remains - even with the fresh APK the clipboard panel did not appear in the scrolled Settings page, and the packaged frontend assets could not be inspected to confirm which bundle shipped, so that needs a dedicated look. |
| A debug APK ships no frontend, so a stale Vite looks like a broken build | The clipboard panel never appeared on the phone, and the search for why went down the wrong path twice. The answer: `aapt2` and the zip listing show the debug APK contains exactly one asset, `assets/tauri.conf.json`, whose `devUrl` is `http://localhost:8563/` - a debug build loads the frontend from the Vite server rather than embedding `dist`. The server that was running predated the clipboard panel, so the phone faithfully rendered old code while `dist/assets/index-MghxIU15.js` already contained `clipboard-heading`. Restarting `bun run dev` and reopening the app made the panel appear immediately, panel and clipboard page both verified by screenshot. The useful rule: for frontend work, `bun run dev` plus `curl http://localhost:8563/src/pages/SettingsPage.tsx` answers what the app will render in one step - rebuilding an APK to inspect `dist` is strictly slower and, as seen earlier, can silently reinstall a stale artifact. Also noted: the automatic-send switch already lives on the clipboard page itself, with per-device receive policy beside it, so no new settings field was needed. |
| Single instance: a second launch hands over instead of competing | The "phone is offline" report was two desktop processes fighting over UDP 49784 - the older one won the bind, so the newer one ran with no listener and an empty peer list. `tauri-plugin-single-instance` is now registered first in the plugin chain: a second launch raises the existing window (`show`, `unminimize`, `set_focus`) and exits. Verified: `before=1`, a freshly started binary printed `second instance exited (handed over)`, `after=1`, and `49784` stayed with the original process. This turns the failure mode from silent and confusing into impossible - there is no way to end up with a second instance that quietly has no network. |
| The point of a text message is the paste on the other side | The clipboard work had been built around the wrong centre: sharing the local clipboard outward is a real feature, but what makes the app useful across platforms is the opposite - send a line of text, and it is on the receiver's clipboard to paste. `ClipboardState::write_local` now exists, and `record_incoming` calls it in the `fresh` branch, before the event is emitted and before the notification: the message is on the clipboard by the time the user looks at it. A failure logs and moves on, because a locked clipboard must not lose a delivered message. Verified live - a message sent from the phone produced `received text copied to the clipboard bytes=6` on the desktop and the desktop clipboard changed. The content was garbled because `adb shell input text` goes through the phone's IME, which mangles non-ASCII; the mechanism is what the log proves, and a human typing will not hit that. Outbound sharing stays as a secondary feature on the clipboard page. |
| Two reasons the phone could not copy the text | The desktop-to-phone direction failed for a build reason and a compile reason at once. First, the phone runs `libfluxy_lib.so` from inside the APK, so the `write_local` change never reached it - the installed APK was still 11:50 while the fix landed at 14:12. Only the frontend refreshes over `devUrl`; Rust does not. Second, rebuilding exposed `error[E0425]: cannot find function `init` in crate `tauri_plugin_single_instance``: that crate has no Android build, and registering it unconditionally broke the Android target. It is now `#[cfg(desktop)]` in `lib.rs` (built through a `let builder` binding so the conditional fits the chain) and moved to the `cfg(not(target_os = "android"))` dependency table. The APK rebuilt at 14:57 and installed. Garbled text in the earlier check was `adb shell input text` going through the phone IME, not the transfer. |
| Clipboard verified in both directions on real devices | Sending text from the phone lands it on the desktop clipboard, and sending from the desktop lands it on the phone's - both confirmed by pasting on the receiving side. That closes the feature's actual purpose: text typed on one device can be pasted on the other without any shared account or syncing. The implementation is a single call in `record_incoming` through `ClipboardState::write_local`, so it works the same regardless of which side sends; the OS-specific halves are the desktop `arboard` thread and the Android `ClipboardManager` bridge wired earlier today. Outbound clipboard sharing stays available on the clipboard page as a separate, opt-in feature. |
| A-08 process-death recovery: the storage exists, the restore does not | Checkpoints are already durable - `CheckpointStore` writes `manifest.bin` and a progress file per transfer under `checkpoints/<transfer_id>/`, and `list()` enumerates them - so a crash leaves everything needed to resume on disk. What is missing is the restore: `TransferRuntime::new` starts with an empty `records` map and never reads `list()`, so after a restart the user sees no trace of the transfer that was interrupted and the checkpoint is orphaned. A `TransferState::Interrupted` variant now exists (bindings regenerated, `transfers.state.interrupted` added in both locales) as the state such a record would carry, but nothing produces one yet - the restore step is still to write. Rebuilding a record also needs the peer's id, which `TransferCheckpoint` does not store; the clean fix is a `device_id` field on the checkpoint, filled where `save` is called. This is the remaining work, recorded here so it is not mistaken for done. |
| A-08: interrupted transfers survive a restart | Four pieces, in dependency order. `TransferCheckpoint` gained `device_id: Option<Uuid>` (serde default, so old files still load) with a `for_device` builder, mirroring how `in_batch` already worked; `CheckpointResume` carries the same value and `save` writes it into the checkpoint; the receiver passes `record.device_id`. Then `restore_interrupted` walks `CheckpointStore::list()`, skips completed and cancelled entries, recovers how many bytes are present by summing the missing block lengths against the manifest, resolves the peer name from the address book (falling back to a short id so a forgotten peer still shows), and inserts an `Interrupted` record. It runs from the setup hook after `attach_chat`, because the name lookup needs the address book. Note the recovery is visibility plus retained checkpoints, not a one-click resume: the source path of a send and the offer of a receive are both gone with the process, so the honest state is "interrupted, retry by sending again" - and a re-offer with the same id picks the checkpoint back up. Retry is deliberately not permitted on this state, so the UI cannot offer an action that would silently restart from zero. |
| A-04: an offer that does not fit is refused before anything is written | `ExportBridge` had been reporting `StatFs` free bytes since A-04 landed, but nothing consulted it, so a transfer larger than the remaining space would be accepted and then die on the last block with a half-written file. `handle_offer` now checks `fs2::available_space(receive_root)` before touching the offer: when `total_bytes` exceeds it, the transfer is rejected with code `insufficient-space` and a message naming both numbers, and the sender's retry costs nothing because nothing was created. `fs2` is in the workspace dependencies and resolves through `statvfs` on unix (Android included) and `GetDiskFreeSpaceEx` on Windows, so the same line covers desktop and phone - the Android target build was exercised (APK timestamp moved to 16:15) to be sure the crate compiles there. An unavailable answer (the call itself failing) is treated as "do not block": refusing a transfer because the free-space probe broke would be worse than attempting it. |
| Every reject code now resolves to real copy | Cross-checking `ErrorCode::as_str` against `transfers.reason.*` showed all sixteen protocol codes covered, but the shell also sends reject codes that have no `ErrorCode` variant, and those reached the sender as a raw key. Four were wrong: the space check I had just added used `insufficient-space` (a string I invented - the replacement also missed at first because the indentation in the pattern did not match the file, so it survived a passing build until `Select-String` caught the line still there), and `rejected`, `invalid_manifest` and `invalid_batch` had no copy at all. The first now sends `code_name(ErrorCode::DiskFull)`, since "disk full" is exactly what the receiver means; the other three got `transfers.reason.*` entries in both locales, because they describe situations worth naming (the peer declined, the manifest did not match, the folder list was inconsistent). Adding copy alone broke `catalog.test.ts` - it flags unused keys - so those three are listed in its `dynamicKeys` alongside the protocol codes, with a comment saying why. Final state: 16 protocol codes, 19 reason keys, none missing. |
| A-08 permission revocation: the retry exists, the explanation does not | `ACCESS_LOCAL_NETWORK` (Android 16) is already handled defensively: `LocalNetwork.granted` first asks the package manager whether the permission even exists on this build, because some API 36 images omit it and a plain `checkSelfPermission` would report DENIED forever; the discovery task then loops, logging `waiting for the local network permission before discovery starts` once, and the plugin re-asks on every attempt so a late grant still takes effect. What was missing is any way for the interface to say so - a user who revoked the permission saw "no nearby devices" and nothing else. `localNetworkGranted` is now a plugin command (and a `local_network_granted` shell API) returning the current answer; wiring it into a settings banner is the remaining step, deliberately left until this command could be built and verified. |
| A-08: the 10 GiB run did not produce a result | Attempted twice through the phone UI and both attempts failed to start a transfer: the picker selection landed on a screenshot instead of the 10 GiB archive, and after routing the file in through a share intent (which worked - `[share] registered content://media/external/file/1000000418 -> tenGB.zip`, the list showed "10.0 GB") the tap on the send button cleared the selection and produced no `transfers queued` line on either side. So the 10 GiB assertion is still open; what this run does establish is a limit of this verification method: scripted taps on a WebView cannot be trusted to hit a specific row, and every attempt costs minutes. The honest next step is a human doing one send, not a fifth script. A related observation worth keeping: a 10 GiB file has to be hashed before the offer (BLAKE3 over 10 GiB), which is the first place a multi-gigabyte transfer could plausibly time out - that is what a real run should watch. |
| Both ends now run release profiles for the 10 GiB check | The first attempts used debug builds on both sides, where the offer's cost is dominated by BLAKE3 over the whole file - unoptimised, that is many times slower, so a 10 GiB send plausibly expires the 60 s offer timeout before the sender finishes hashing. Reaching a release profile took three routes. `android:build -- --release` refuses without a keystore (the one that used to exist was moved aside to let a re-signed build install over the debug APK), and restoring it would change the signing certificate and force a re-pair. `bun run dev -- --release` works but its watcher reacts to `plugins/*/build/` artifacts from the Android build and loops on \"Rebuilding application\". The combination that holds: phone via `tauri android dev --release` (release Rust, debug signature, so trust survives), desktop via `bun run build` and the standalone `target/release/fluxy.exe` (17:19), with `dev-web.mjs` serving the fresh bundle the phone's `devUrl` needs - the old Vite was still serving a pre-clipboard bundle and had to be restarted. Verified after the switch: phone reports `UDP fallback discovered a peer peer=THEPOY` and the desktop release process is up. The 10 GiB transfer itself still needs one manual send. |
| Vite listened on localhost only, so the phone had no page | `vite.config.ts` sets `host: host || false` where `host` is `TAURI_DEV_HOST`NaN`tauri dev` fills in for a device build; the shared `dev-web.mjs` server started on its own does not set it, so Vite fell back to its default - `Get-NetTCPConnection` showed the listener on `::1` alone, and the phone's request to `http://192.168.1.8:8563/` could not connect. Worse, the failure is silent from the phone's side: the WebView shows a blank page while discovery and the Rust half keep working, which is exactly the state reported. The fallback is now `0.0.0.0`, and the same address answers 200 over the LAN. Confirmed on the phone: the send page renders with THEPOY paired and `UDP fallback discovered a peer` in the log. |
| Send with an empty list was silent, and a second share is still missed | Two separate problems under "I tapped send and nothing happened". First, the handler opened with `if (ids.length === 0) return` - a dead button with no feedback at all; it now sets an error ("no file selected yet") so the state is visible. Second, and the reason the big file kept disappearing: `sharedFiles` was drained only on mount, so a share delivered to an already-running activity (singleTask brings the task forward, `onNewIntent` fires) left the URI in `PendingShare` with nothing asking for it. The page now drains on mount and again on `visibilitychange`, which covers coming back to the foreground. Verified the bundle Vite serves contains that listener and the phone logs `FluxyPlugin: sharedFiles drained=0` after a share - so the drain runs, the holder is empty, and the remaining suspect is `MainActivity.onNewIntent` not being reached at all. That is where the next look goes. |
| Sending a large file looked frozen because nothing was said | The send button set a busy flag and then awaited `startTransfers`, which hashes the whole file before the first byte moves - seconds for a photo, tens of seconds for gigabytes. Meanwhile the only copy on screen still said nothing, so a working send was indistinguishable from a dead button. The handler now raises the notice before the call: `mobile.preparing` explains that large files are hashed first, and the completion copy points at the Transfers page for progress instead of just claiming it was sent. Getting there cost two catalog failures worth noting: `mobile.noFiles` was added in Chinese only at first (the English anchor I keyed on did not exist, since that key had already been reworded), and the catalog test caught it as \"missingInEn\" rather than letting a half-translated key ship. |
| A cancelled send is resumed, not duplicated | Retrying the same file used to mint a fresh `Uuid::now_v7` for every attempt, which had two consequences the report caught exactly: the cancelled row stayed in the list beside the new one (two entries for one file), and any checkpoint written under the old id was orphaned, so the retry started from zero despite the file being unchanged. `start_transfers` now looks for a `Cancelled` or `Failed` send with the same peer, file name and size, and reuses its `transfer_id`; `insert` then replaces the old row, so the list holds one entry and the checkpoint directory is the one that already exists. Share deduplication landed alongside it: `onCreate`, `onNewIntent` and `onResume` can all observe the same intent, and `PendingShare` appended blindly, so one share arrived as two identical files. The holder now skips URIs it already has, verified as `sharedFiles drained=1` followed by a single `[share] registered ... -> tenGB.zip`. The cancel-then-retry behaviour itself is not yet exercised on hardware. |
| Cross-restart resume: the durable half existed, one match arm was missing | The objection to an in-memory content cache is right - it dies with the process - but the durable record was already on disk: `checkpoints/<id>/manifest.bin` holds `root_hash` and `progress` holds `device_id`, `file_name` and the received bitmap, and `restore_interrupted` rebuilds an `Interrupted` row from them at startup. What was missing is that `start_transfers` only reused the id of a `Cancelled` or `Failed` send, so after a restart the restored row sat there while a fresh send minted a new id and orphaned the checkpoint again. `Interrupted` is now in that match, which closes the loop: kill the process mid-transfer, restart, the row reappears from disk, and sending the same file resumes into the same checkpoint. The residual looseness is that matching uses `file_name` + `size` rather than `root_hash`; tightening it needs the hash before queueing, which is exactly the cost the current split avoids. |
| Editing Rust is not the same as shipping it | After adding `Interrupted` to the reuse match, `cargo check`, `clippy` and the test suite were all green - and both installed artifacts still predated the change. `cargo check -p fluxy` builds for the host; the phone runs `libfluxy_lib.so` from inside the APK, so a phone-side change needs `bun run android:build` and an install, and the desktop binary needs `bun run build`. This is the fifth time in this session that a passing build was mistaken for a deployed one, which makes it a process gap rather than an oversight: the gate list ends at \"compiles and passes tests\" with nothing asserting that the artifact matches the source. Rebuilt both (APK 18:33 to 18:52, exe 17:19 to 19:03) and restarted them. The desktop rebuild also needed the running process stopped first - `拒绝访问 (os error 5)` is the linker failing to overwrite a live binary. |
| Automation reaches the page, not the button | Per the rules now in `AGENTS.md`, the resume test was driven entirely through `adb`: `am start -a SEND --eu EXTRA_STREAM content://media/external/file/...` delivers the share, the screenshot confirms `twoGB.zip 2.0 GB` sits in the send list with the peer button visible, and `content query` resolved the MediaStore id. What does not work is the last step - `input tap` and even `input motionevent DOWN/UP` on the `THEPOY` button produce no `transfers queued` line. The same commands do work on the picker and on the settings page, so this is specific to that button: either it is disabled by a stuck `busy` flag or the WebView is not turning the synthetic touch into a click there. Without devtools on the device that cannot be settled by guessing. The durable fix is to stop depending on the coordinate at all: a small debug-only intent - something like `fluxy://send?file=<uri>` - would let `am start -d` reach the send path directly, which is both more reliable for verification and useful for the eventual share-sheet integration. |
| A deep link that removes the tap from the loop | The send button refuses synthetic touches, so verification needed a path that skips it entirely. The activity now accepts `fluxy://send?file=<content-uri>&device=<id>` (VIEW filter on scheme `fluxy`, host `send`), `PendingShare` gained `captureLink` and a target slot, `shared_files` returns `SharedBatch` with an optional `device`, and the send page auto-sends when a device was named. Driving it end to end works: the intent yields `target=01a16178-...`, then `[share] auto-sending 1 file(s)`, then `[send] attempt ... files=1`, and the screen shows the new preparing-the-file notice. What did not follow within two minutes for a 2 GiB file is `transfers queued`, which is consistent with debug-profile BLAKE3 over 2 GiB - exactly the cost the release profile removes - so it should be re-checked on a release build before being called a defect. Two quoting traps worth recording: PowerShell eats `&` in a bare string, and even `adb shell am start -d $link` lets the phone shell split on it, so the URL has to reach `am` inside single quotes. |
| A 2 GiB send through the chat page, measured | Sending from the message page works end to end: `transfers queued count=1`, `chat session acquired; hashing the file`, `manifest ready; offering the transfer file=twoGB.zip bytes=2147483648 elapsed_ms=31961`, then `transfer finished file=twoGB.zip bytes=2147483648 elapsed_ms=132275 mib_per_s=15.48` with the desktop landing `received/twoGB.zip` at the full 2147483648 bytes. Two numbers matter. First, hashing 2 GiB took 32 s on this debug build, which puts 10 GiB at roughly 160 s - more than twice the 60 s offer timeout, so the earlier 10 GiB attempts that produced no `transfers queued` were most likely failing on the timeout rather than on the picker or the button. Second, throughput was 15.5 MiB/s against the 30 MiB/s measured with release builds on both ends, confirming the debug penalty applies to the transfer itself and not only to hashing. The verification mistake on my side was looking for progress on the message page; the transfer centre is where it belongs. |
| Hash timing instrumented, measurement blocked by the session wait | To learn whether the pre-hash is I/O-bound or CPU-bound, `manifest.rs` now accumulates the nanoseconds spent in `source.read_at` and in the BLAKE3 window work and prints `manifest timing read_ms=... hash_ms=... windows=...` to stderr, which surfaces as `RustStdoutStderr` in logcat without adding a logging dependency to that crate. The APK was rebuilt and installed per the rules (21:14). The run itself produced no timing line: the deep link drained and `[send] attempt ... files=1` appeared, but neither `waiting for a chat session` nor `chat session acquired; hashing the file` followed, so `start_transfers` never reached the hash. That is a session-establishment stall, not a hashing result, and it needs its own look before the split can be read. The only solid number remains the earlier one: 2 GiB hashed in 31.96 s, about 67 MiB/s. |
| The send never reaches Rust | After instrumenting `start_transfers` with an entry log line and a mark after `chat.open`, the deep-link run produced `[send] attempt ... files=1` and then nothing - not `start_transfers entered`, which is the first statement in the Rust command. So the call does not arrive: the page prints its preparing notice and awaits `engine.startTransfers`, and that promise never settles. Confirmed on screen: the notice stays up indefinitely and the transfers list stays empty, which is the right pair of observations to have started from rather than the console lines. Two suspects remain - the IPC path itself, or `chat.open(peer)` inside the command blocking before the log line could be written (though the entry line is the very first statement, which points at the IPC layer rather than at blocking inside the function). Separately, this run did show `restore_interrupted` working: the transfers page lists an earlier 2.9 GB receive as 中断/Interrupted, rebuilt from its on-disk checkpoint. |
| Two real bugs behind "sending a shared file does nothing" | The trace bottomed out twice. First: `openSourceFile` called the plugin's `fs_open_source`, which opens a descriptor but never registers the file, so the object it returned had no `file_id` - `files.length` was 1 while `ids.join(\",\")` was empty, and `start_transfers` rejected the id-less list before doing anything. A shell command `open_shared_source` now pairs `fs_open_source` with `register_descriptor_source` and the log confirms `shared document registered file_id=01a187de-...` followed by `start_transfers entered ... files=1`. Second, once the call reached Rust it stopped at `chat.known_peer(device)`, which only consults live `sessions`: on a cold start - which is exactly the state `force-stop` then a deep link creates - the session table is empty, `known_peer` returns `None`, and the caller turns that into `peer_offline`. The page does show it (`device is not currently reachable on the LAN`), so the failure is at least visible now. Why the message-page send succeeded earlier is now attributable: the app had been running long enough to hold a session. A share into a cold app would still fail, which makes this the next thing to fix. |
| The pre-hash is CPU-bound, and the cold-start fallback still misses | Instrumented numbers for 2 GiB on a debug build: `read_ms=973 hash_ms=30157` across 32 windows of 64 MiB. Reading runs at about 2.2 GB/s - the content:// path is not the problem - while hashing manages only 71 MiB/s, so 97% of the wall time is BLAKE3. That is far below what BLAKE3 should reach, which points at the debug build (no SIMD optimisation) and possibly at `update_rayon` competing with the outer `par_chunks` for the same pool. Extrapolating 10 GiB at this rate gives ~144 s, well past the 60 s offer timeout, which is consistent with the earlier failures; a release build should be an order of magnitude faster and needs measuring. That measurement did not happen: the release run reached `start_transfers entered` and then failed with `queuing transfers failed code=peer_offline`. The `known_peer` fallback added this round consults `address_book`, which resolves through `nearby`, yet discovery had already logged a resolved peer - so either the map entry was absent at that instant or its device id does not match the paired one. That is the thread to pull next; until it is fixed, a share into a freshly started app still cannot send. |
| Release builds can now be inspected without a console | `fluxy_app::logging::init` had always taken a `log_dir` and never used it - the process only ever wrote to stderr, which a release build has nothing attached to. It now adds a daily rotating file through `tracing_appender`, writing to stderr and the file at once via `stderr.and(file)`, with the worker guard leaked so the writer outlives the runtime. `app_log_tail` returns the newest file's tail and the settings page gained a 运行日志 panel that renders it in a scrolling `pre`. Verified on the phone: `logs/fluxy.log.2026-10-30` appears under the app's private root (not under `files/`, which is where I first looked) and the panel shows the startup sequence. Two defects surfaced during the check. The log carried ANSI escapes into the file - `fmt()` colours its writer, so `.with_ansi(false)` was needed once the same writer feeds both sinks; logcat loses colour, which is a fair trade. And the first panel screenshot still showed escapes because my cleanup used `run-as ... sh -c 'rm -f logs/*.log*'`, where the phone's shell does not expand the glob, so the old file survived. Both fixed and re-verified. |
| The first real release build is on the phone | `dsh` had never verified a signed release end to end, because every attempt used a dev build for convenience. Restoring `keystore.properties` from the `.dev` copy it was stashed as and running `android:build -- --release --target aarch64` produced a 16.1 MB APK signed `CN=Fluxy, OU=Release`. The debug build had to be uninstalled first (different certificate), which also discarded its data: the device name reset to \"Fluxy device\" and the pairing was lost, as expected. The release build starts and runs: `flags=[ HAS_CODE ALLOW_CLEAR_USER_DATA ALLOW_BACKUP ]` with no DEBUGGABLE, the send page renders, and the new log panel reads the file - which is the point of that panel, since `run-as` is unavailable on a non-debuggable install. Its very first use surfaced a real regression: `android receive directory set path=/storage/emulated/0/Android/data/com.thep0y.fluxy/files/Download/Fluxy`. The app-private fallback is correct behaviour when `MANAGE_EXTERNAL_STORAGE` is missing, but the uninstall cleared that grant, so a fresh release install silently stops honouring the `Download/Fluxy` requirement until the user re-grants it. `appops set ... MANAGE_EXTERNAL_STORAGE allow` restores it; what the app should do is detect the missing grant and say so on the send and settings pages rather than quietly writing somewhere else. |
| The deep link does not drain while the app is already in front | With both ends running the release profile (`bun run dev:web` then `bun run dev --release`, and the phone showing the desktop as `TITAN2` - the name fix works on both sides) the 10 GiB deep link reached the activity: `onNewIntent action=VIEW data=fluxy://send?file=...&device=...`. It never reached the runtime. The drain runs from `onMount` and from `visibilitychange`, and neither fires when the intent arrives at an already-foreground task - the activity logs `onResume action=null data=null`, so the follow-up intent is not what resumes it. Sending the app to the background and bringing it back did not help either, which suggests Android's WebView does not deliver `visibilitychange` the way a browser tab does. The durable fix is to stop polling for this: the activity should emit a Tauri event when it captures a link and the page should listen for it. Until then the message-page attachment path remains the only reliable way to start a large send, which is what the earlier 2 GiB verification used. The 10 GiB run is therefore still open. |
| A paired peer kept the name it had at pairing time | Discovery showed `TITAN2` while the peer list showed `Fluxy device` for the same device, and the merge in `peer_summaries` explains why: `if !existing.trusted { existing.name = nearby.name; }`. Only untrusted peers ever took the name discovery reported, so a trusted one kept whatever `trust.json` recorded at pairing time - which on Android was the placeholder, because the pairing happened before the system name was adopted. The name now always follows discovery when it is non-empty; `platform` still only changes for untrusted entries because a trust entry's platform is part of what pairing recorded. Verified after a rebuild: the desktop log shows `UDP fallback discovered a peer peer=TITAN2 platform=android` and `chat session live`. The durable half is still missing - `trust.json` keeps the stale name, so an offline peer would show it again - but while the peer is reachable the list is correct. |
| The rename fix does not reach a peer that discovery is not listing | The `peer_summaries` change assumed the trusted entry would be merged with a `nearby` record. The user's screen contradicts that assumption: the panel reads `1 台 / 0 台在附近` while the log keeps printing `UDP fallback discovered a peer peer=TITAN2`. So `nearby` is empty at the moment the snapshot is built - either the probe's records are evicted before the TTL check, or they never enter the map - and the trust entry is used unmerged, which is why the stale name still shows. The fix was therefore addressing a branch that does not execute. Two things need separating before another attempt: why `nearby` is empty although the probe logs a peer every five seconds, and why removing a device leaves its `trust.json` entry behind (the remove path does call `remove_trust` and that does save, so the write is probably going somewhere else or failing silently). Guessing again would repeat this round's mistake. |
| Both inputs are right, the snapshot the UI reads is not | Instrumenting the map settled the argument: `nearby map after upsert nearby_len=1` on every probe, and `trust.json` holds exactly one entry - `01a18af4-6dae-75a1-abff-db0e900857c1`, the peer currently paired, named `Fluxy device`. Same device id on both sides, so `peer_summaries` does merge them and, with the change from the previous round, should pick up `TITAN2`. The list still shows the old name, which means the page is not rebuilding from a fresh snapshot: `upsert_nearby` emits `AppEvent::Peer` but never bumps the snapshot version, so whether the UI sees the merge depends on some other path calling `bump_snapshot`. That is now the specific thing to check rather than the merge itself. Also worth recording: the earlier report of a removed device lingering in `trust.json` does not reproduce here - the file contains only the currently paired peer - so either the removal did work and the observation was of this entry, or it was a different profile. Evidence beats both guesses. |
| Two labels for one peer: the session list reads a different source | The screenshot made the split unmistakable - the trusted-device list shows `TITAN2` while the conversation list and its header show `Fluxy device` for the same device, in the same window. They are not the same data: `TrustedDevices` reads `snapshot().peers`, which merges discovery and is therefore current, while `ChatWorkspace` reads `engine.chatConversations()`, whose `name` is whatever the pairing handshake recorded and is never refreshed. The merged snapshot was already right, so the fix belongs on the conversation side: `displayName(deviceId, fallback)` prefers the live snapshot name and falls back to the conversation's own. Applied to all three render sites (list row, header, retry label). That also explains why the earlier `peer_summaries` change looked ineffective - it did correct the trusted list, but the part of the UI being examined was reading elsewhere. The deeper gap stands: `chatConversations` should carry the current name so the backend has one notion of a peer's name, not two. |
| The durable name now follows discovery | Three layers were wrong, not one. `peer_summaries` only refreshed the name of an untrusted peer, so the trusted list kept its pairing-time label; `ChatWorkspace` did not read the snapshot at all, so the conversation list kept the handshake label; and nothing ever wrote the observed name back, so `trust.json` stayed on `Fluxy device` instead of `TITAN2`. All three are addressed: the merge takes the live name for trusted peers, the conversation prefers the snapshot name, and `snapshot()` calls `sync_peer_names` which renames and saves only when something actually changed. Verified: `trust.json` now reads `name=TITAN2`. The split between durable trust material and a display label is the point worth keeping: `device_id`, `public_key` and `cert_fingerprint` are what make pairing mean anything and must only be written at pairing time, while `name` and `platform` are a cache that discovery is allowed to correct. Storing only a device id would leave nothing to verify a peer against. |
| The 10 GiB send fails twice, for two different reasons | The first attempt reached the transfers page as 失败 with `超时：对端没有在约定时间内响应` / `timeout 2001`. The logs show why: `waiting for a chat session to send on` at 20:20:07 and `chat session live` at 20:20:52 - 45 s against a `SESSION_TIMEOUT` of 30 s. The desktop had completed its handshake 16 s in, so nothing was wrong with either end; the phone simply gave up first. Raised to 90 s. The second attempt died earlier, at `device is not currently reachable on the LAN`: a cold-started deep link arrives before discovery has produced any peer, so the `known_peer` fallback into `address_book` finds nothing and the send is rejected immediately. Both are timing assumptions that a fresh process breaks - one waiting too briefly for a session, the other not waiting at all for discovery. Neither is data-path related, so the 10 GiB question - throughput and resume at that size - is still unanswered. |
| 10 GiB transfers end to end on a release build | `transfer finished file=tenGB.zip bytes=10737418240 elapsed_ms=358931 mib_per_s=28.53` and the desktop holds `received/tenGB.zip` at exactly 10737418240 bytes. Getting there needed two timing fixes, both found by measuring rather than guessing. `PEER_WAIT` (8 s) lets a cold-started app wait for discovery instead of rejecting the share it was just handed - the log now shows `peer known, opening peer=THEPOY` 19 s into the run where it previously said `device is not currently reachable on the LAN` and gave up. `SESSION_TIMEOUT` went from 30 s to 90 s because the session took 42 s to come up. With both in place the manifest was offered and the transfer ran for 359 s at 28.5 MiB/s, close to the 31 MiB/s measured on 2 GiB - the slight drop is expected once the ramp-up is a smaller share of the total. This closes the 10 GiB item: the protocol handles the size, and the earlier failures were all timing assumptions that only a cold start exposes. |
| Interrupt, restart, and the resume still does not happen | The kill itself worked exactly as designed: `progress.bin` for the 10 GiB receive ends up holding `"state":"interrupted"` with the first 96 bytes of the bitmap set, so roughly 3 GiB had landed and was recorded - the state machine marks a broken transfer correctly on restart. What fails is the step before that: `restore_interrupted` skips any checkpoint whose `device_id` is absent, and this one has no `device_id` key at all. The field exists in `Checkpoint` with `#[serde(default)]` and is written through `resume.device_id` in `save_checkpoint`, and the receiver does call `.for_device(record.device_id)` - so somewhere between setting it and serialising, it is lost. The next step is a log line right before `store.save` printing `resume.device_id` and `checkpoint.device_id`: guessing at which of the two is None is what produced the last three wrong fixes. Until it is resolved, a resumed 10 GiB send starts from zero rather than from the 3 GiB already on disk. |
| The device id was a symptom, not the cause | The added logging settled the checkpoint question immediately: `resume_device=Some(01a18af4-...)` then `checkpoint_after=Some(01a18af4-...)`, and both the old and the new `progress.bin` now carry `device_id`. `restore_interrupted` reports `restored=6` on the next start, so checkpoints with a device are rebuilt. The resume still does not happen, and the reason is now visible: the resend created `transfer_id=01a18d64-3853` with `checkpoint_before=None`, i.e. a brand new checkpoint rather than the interrupted one. The reuse rule in `start_transfers` matches on peer, file name and size against `Cancelled`, `Failed` and `Interrupted` records, so either the restored records do not carry `tenGB.zip` as their file name or the queue never sees them. That is the next thing to print - the restored record's fields, not another guess. |
| Resume has never actually been wired up | The logging found it, and it was not `device_id` or the reuse rule. `CheckpointResume::new` returns `checkpoint: None` - it only stores the file name and the optional device. `receive_file_resumable` then does `resume.and_then(|resume| resume.checkpoint.clone())`, gets `None`, and builds a brand new `TransferCheckpoint`. So the receiver never reads the `progress.bin` that is sitting on disk with 3 GiB marked complete, and `checkpoint_before=None` in the log is the honest answer rather than a mystery. Everything downstream of that is correct - `matches_manifest`, `covered_ranges` and `resume_sink` all exist and would do the right thing - but only `CheckpointStore::resume_plan` populates the `checkpoint` field, and the receiver path never calls it. This is why four previous fixes changed nothing: they were all upstream of a value that is unconditionally `None`. The fix is to look the plan up by file name before constructing the resume, not to adjust anything else. |
| Instrumentation first, and it names the gap in one run | The transfer path now logs its decisions rather than only its milestones: `StateMachine::transition` records every legal change and every rejection, `CheckpointStore::resume_plan` says which of the three reasons it declined (no checkpoint, corrupt, manifest mismatch), `receive_file_resumable` distinguishes "resuming from a stored checkpoint" from "starting from zero", and `start_transfers` lists the sends it considered. The first run with it produced an unambiguous account: `looking for an interrupted send to reuse file=tenGB.zip size=10737418240` followed by no candidate at all, then `resume rejected: no checkpoint on disk transfer_id=01a18f9b-...` for the id the resend minted. That closes the investigation with evidence instead of inference. The chain is: the sender keeps no checkpoint of its own, so a restart leaves nothing to reuse, so the resend gets a fresh `transfer_id`, and the receiver looks checkpoints up by exactly that id. The durable fix is to look the plan up by content - `file_name` plus `manifest_digest` - which is what actually identifies "the thing I already have three gigabytes of". `transfer_id` is a session number and was never a stable identity. |
| Resume now looks its checkpoint up by content | `CheckpointStore::resume_plan` takes a `transfer_id`, and a sender mints a fresh one for every send - after a restart it cannot even reuse its own. So the lookup now has a sibling, `resume_plan_for(file_name, manifest)`, which walks the store and accepts a checkpoint whose file name, block size, file size and manifest digest all match, skipping ones that are already complete. `receive_inner` tries the id first (the same-process retry case) and falls back to content. That makes `transfer_id` what it always was - a session number - while the identity of a resumable transfer comes from `manifest_digest`, which is what "I already have three gigabytes of this" actually means. Two long-standing test failures surfaced while gating this and are also fixed: `speed_meter_smooths_a_burst` sampled over 100 ms while `MIN_SAMPLE_SECONDS` is 0.25 s, and the stream test hard-coded 4 streams after the default became 8. Both are now tied to the constants they describe. Suite: 298 Rust, 91 frontend, clippy clean. |
| The resume plan is found, but the run still starts from zero | Three defects stacked here, each visible in the new logs. First `resume_plan_for` returned the first match, and since every interrupted attempt leaves a checkpoint, it picked the oldest (288 blocks) instead of the newest (1420); it now scans all candidates and keeps the one with the most progress, logged as `resume plan: candidate ... blocks_done=` per candidate. Second, `receive_inner` handed the plan to `prepare_sink` but built `CheckpointResume::new` anyway, so the stored bitmap reached the file sink and never the checkpoint; it now uses `from_plan` when a plan exists. Third, `PEER_WAIT` was 8 s while a cold start needs longer - the log said `discovery never produced the peer` - so it is 25 s. After all three the receiver logs `resume plan found by content id=01a18f9d-... blocks_done=1420`, but `resuming from a stored checkpoint` has not appeared yet, so something between constructing the resume and entering `receive_file_resumable` still drops it. That is the next line to read, and the logs are now precise enough to find it. |
| The resume chain works up to the point where the test setup is wrong | With the plan now carried into `CheckpointResume` and `PEER_WAIT` raised, the receiver logs `receiver: preparing the sink plan=true` and `prepare_sink: resuming into the existing file ranges=4` - two of the three missing links are closed. The third is not a code defect: `received/tenGB.zip` already holds all 10737418240 bytes from yesterday's successful run, while the checkpoint being resumed describes 1420 of 2560 blocks. `resume_sink` is asked to continue into a file that is already complete, and nothing after it logs. The verification environment is the problem: a resume test needs a receive directory without the previous result, or the file lands in a different name. Clearing `received/tenGB.zip` and rerunning is the next step, and it should be treated as test hygiene rather than another bug hunt. |
| The partial file belongs to the id that created it | `prepare_sink` asked `resume_sink` for the partial file under the id of the send in progress. A resumed send always arrives with a new id, so the file it wanted - named after the attempt that wrote the bytes - was never there and `resume_sink` returned `NotFound` before reading a single block. That error went to `fail` without a log line, which is why it looked like a hang. The plan carries the id that owns those bytes, so the fix passes `plan.checkpoint.transfer_id` and logs both ids for comparison. A cold clean run afterwards showed the whole chain healthy: `resume plan: no checkpoint matches this file` then `prepare_sink: creating a new file` then `receiver: sink ready` - and then nothing, with the partial file still at zero bytes when the sender was killed 83 s later. The session had been live since 10:11:52, so the break is now on the send side or between the offer and the first chunk, not in checkpoint handling. Worth noting the network moved: this run dialled `192.168.0.101` rather than the usual `192.168.1.x`. |
| The same stale name, a third source | The transfer centre showed 对端 `Fluxy device` while the rest of the UI had `TITAN2`. A transfer record copies `peer.name` when it is created, and `known_peer` answered from `sessions` first - a session stores the name it was opened with, so the record inherited the pairing-time label. Discovery was consulted only when there was no session at all. The lookup now asks discovery first and falls back to the session, which matches what every caller actually wants: the address a dial should use and the name a record should show are both current values, not historical ones. This is the fourth distinct place the stale label surfaced - trusted list, conversation, trust file, transfer record - which is the argument for keeping one authoritative source rather than letting each view copy it. |
| 对端设备 name: found in the handshake, of all places | The transfer record's peer label came from `session.peer().name` in `chat.rs` - a value captured when the chat session was opened and never refreshed. Nothing downstream could have fixed it: the offer carries `device_name`, the receiver stores it verbatim, so the record showed whatever the session had cached at pairing time. `peer_name` now resolves through `known_peer`, which was already changed to prefer discovery, and the session value remains only as a fallback for a peer that stopped advertising. Verified: `offer received; name as advertised by the sender device_name=TITAN2`. This is the fifth place the stale label hid - trusted list, conversation, trust file, transfer record, and now the handshake value that feeds it. Each fix was correct locally and incomplete globally because the name was copied at five different moments; the durable answer is one authority (discovery) and lazily resolved reads everywhere else. |
| Confirmed: the transfer centre now shows the live name | The user reports `TITAN2` in the transfer list, matching the sender's own log line `offer received; name as advertised by the sender device_name=TITAN2`. The stale-label hunt is closed across all five surfaces it had reached: the trusted list, the conversation header and rows, `trust.json`, the transfer record, and the handshake value that fed the record. The pattern worth remembering is that each was a copy taken at a different moment, so fixing one never fixed the others - and only `known_peer` being made to read discovery first, with every consumer routed through it, ended the sequence. |
| The resume test is blocked by a session that will not come up | With a clean receive directory, a clean checkpoint store and a freshly rebuilt pair, the send queued at 21:21:57 and then sat at `waiting for a chat session to send on` until the 90 s timeout. The desktop side recorded `incoming pairing connection accepted` at 21:21:37 and `chat session ended ... reason=control stream closed` at 21:22:10 - an incoming connection was accepted and then dropped, while the phone was waiting for a session the receiver never offered. Earlier runs on this network did work (`chat session live ... remote=192.168.0.102:64982` at 21:01), so this is intermittent session establishment rather than a constant failure, and it is the thing to diagnose before the resume can be measured. The four resume defects found earlier are all fixed and logged; none of them can be exercised while the session never establishes. |
| Resume works end to end, verified on the phone | After killing a 10 GiB send with 416 MiB on disk, restarting and resending produced the full chain in one run: `resume plan found by content id=01a19235-a174 blocks_done=86`, `prepare_sink: resuming into the existing file transfer_id=01a1923c owner=01a19235-a174 ranges=13`, `checkpoint at receive time present=true blocks_done=86 matches=true` and `resuming from a stored checkpoint blocks_done=86`. The transfer then ran to completion - `transfer finished file=tenGB.zip bytes=10737418240 elapsed_ms=232365 mib_per_s=44.07` - with `received/tenGB.zip` at exactly 10737418240 bytes. Four defects had to be fixed for this: the plan lookup keyed on a transfer id that changes every send, the plan not being handed to `CheckpointResume`, the partial file being reopened under the new id instead of its owner's, and multiple checkpoints being resolved by directory order rather than progress. Each showed up in the logs the instrumentation was added for, and none could have been found by reading the code alone. Session establishment also flaked once during this work (`incoming pairing connection accepted` followed by an immediate `control stream closed`) but recovered; the new mismatch log will name the cause if it recurs. Throughput on this network is 44 MiB/s, up from 31 on the previous one. |
| T-M10 hotspot matrix: verified on a real phone hotspot | The desktop joined the phone's hotspot (
`ap0`
 = 10.15.131.189/24, desktop WLAN = 10.15.131.208/24), which is the path that was never exercised before. Discovery came up through the fallback channel alone - 
`UDP fallback discovered a peer peer=TITAN2 platform=android`
 repeating every 5 s - confirming that multicast does not survive this hotspot and that the gateway probe carries it. The chat session then established over 
`10.15.131.189:49784`
 and a 100 MiB transfer completed at 
`mib_per_s=43.23`
, within a rounding error of the 44 MiB/s measured over Wi-Fi, so the hotspot is not a throughput bottleneck. Two things this run surfaced: the phone listens on 
`0.0.0.0:42420`
 rather than a specific interface, which is why 
`ap0`
 works without extra binding logic; and a desktop instance left running across a network change keeps its old bind and discovers nothing, so a network switch needs an app restart. T-M14 (AP isolation) still needs router admin and remains open. |
| Deep links now drain while the app is in the foreground | Sharing into the already-running app logged 
`onNewIntent action=VIEW data=fluxy://send?...`
 and then nothing reached the queue, because the page only drained on mount and on 
`visibilitychange`
 - and an app that never leaves the foreground never fires that event. 
`drainShared`
 now runs alongside the existing 2.5 s peer refresh; since 
`sharedFiles()`
 drains, the poll is a no-op when the queue is empty. Verified: 
`sharedFiles drained=1 target=01a16178-...`
 followed by 
`drained=0`
 on the next tick, and the transfer completed. |
| T-M01 hash agreement, verified independently of the protocol | The receiver already refuses to complete unless `sink.root_hash() == manifest.root_hash` (the 3005 check), so `transfer finished` implies a match - but that is the code grading itself. To check it from outside, the phone's original `midtest.zip` was pulled with `adb pull` and both files hashed through `fluxy-cli transfer-send --prepare-only`, which computes the manifest without contacting a peer. Both reported `root_hash=3b66b313c1481abb...7e6fa53b` and `manifest_digest=d4b78d83d8e271c6...` - identical, across two files that differ only in name, which confirms the hash covers content and not the path. The same method re-hashes the 10 GiB `received/tenGB.zip` to `28960eef7d587ab6...0776e064` in 926 ms, so the check is cheap enough to run on demand. |
| A 10 GiB transfer over the phone hotspot is the fastest run so far | 
`transfer finished file=tenGB.zip bytes=10737418240 elapsed_ms=200129 mib_per_s=51.17`
 on 
`ap0`
, against 44 MiB/s over the previous Wi-Fi network and 28.5 MiB/s in the earliest release measurement. Two intermediate observations are worth recording because they look like stalls but are not: 
`receiver: sink ready`
 appears roughly 20 s before the first byte lands, and the 
`.part`
 file stays at 0 bytes through that window, so sampling it early suggests a hang. The sender meanwhile logs 
`chunk streamed`
 continuously, which is the reliable signal that data is moving. |
| T-M06 (Doze) is not yet verified | The attempt is worth writing down as a lesson in test ordering: 
`dumpsys deviceidle force-idle`
 first failed with 
`Unable to go deep idle; not enabled`
, needing 
`dumpsys deviceidle enable`
 first, and by the time that was set the 10 GiB transfer had already completed in 200 s, so no Doze window was ever exercised. The service does use 
`FOREGROUND_SERVICE_TYPE_DATA_SYNC`
 and stops itself well inside the six-hour budget, but it holds no 
`PARTIAL_WAKE_LOCK`
, so whether the CPU suspends mid-transfer under deep idle is still unknown. This needs a rerun with the device put into idle while a transfer is genuinely mid-flight. |
| T-M06 found a real defect: Doze froze transfers completely | With the device in deep idle during an active transfer, the receive file sat at exactly 416 MiB for 40 s, and after 
`dumpsys deviceidle unforce`
 brought the device back to 
`ACTIVE`
 it stayed at 416 MiB for another 25 s - not a hitch but a permanent stall, with the chat session still open and no error anywhere. The cause is that 
`TransferService`
 held 
`FOREGROUND_SERVICE_TYPE_DATA_SYNC`
, which keeps the process alive but does not keep the CPU awake. It now takes a 
`PARTIAL_WAKE_LOCK`
 named 
`fluxy:transfer`
 for 
`TIMEOUT_MS`
 in 
`onStartCommand`
 and releases it in 
`onDestroy`
; the permission arrives through the merged manifest, so nothing had to be declared. |
| T-M06 passes after the fix | Re-ran with the lock held (
`dumpsys power`
 shows 
`PARTIAL_WAKE_LOCK 'fluxy:transfer' ... LONG`
): forcing 
`mState=IDLE`
 mid-transfer, the receive file went from 2896 MiB to 5120 MiB in 45 s - about 49 MiB/s, unchanged from the awake rate - and the run completed at 
`transfer finished file=tenGB.zip bytes=10737418240 elapsed_ms=204841 mib_per_s=49.99`
. The lock is released once the service stops, so the six-hour dataSync budget is unaffected. This matters for every long transfer: a phone with the screen off is the normal case, not an edge case. |
| Sharing into the foreground app is not delivered at all (T-M08 blocked, T-M09 partial) | Two attempts to deliver a share to the already-running activity produced `Warning: Activity not started, intent has been delivered to currently running top-most instance` and then nothing: no `onNewIntent` line, no `drained=1`, and `sharedFiles drained=0` on every poll. This was reproduced with both a `fluxy://` link and a plain `ACTION_SEND`, so the deep-link query parsing is not the cause - the intent never reaches `MainActivity`. Earlier in the same session the identical command did work once (`drained=1 target=01a16178-...` at 12:59:42), which points at intent delivery rather than at the capture code. `MainActivity.onNewIntent` also logs `onResume action=null data=null` right after, because it never calls `setIntent(intent)`, so the activity's field keeps the launch intent; that is a separate, smaller defect. Cause of the missed delivery is not yet established, and T-M08 (notification permission denied) could not be exercised because it depends on getting a share through. |
| T-M08: the foreground service starts without notification permission | With 
`pm revoke com.thep0y.fluxy android.permission.POST_NOTIFICATIONS`
 confirmed as 
`granted=false`
 and 
`POST_NOTIFICATION: ignore`
, a share still put the service up: 
`FluxyService: startForeground sdk=36 type=1`
 and 
`ActivityManager: Background started FGS: Allowed ... uidState: TOP`
. A denied notification does not block 
`startForeground`
 - the notification is simply not shown - so transfers keep working on a device where the user silenced Fluxy. |
| Two shares in flight collide, and that was self-inflicted | The T-M08 run fired an 
`ACTION_SEND`
 and a 
`fluxy://`
 link within seconds of each other. Both drained, both auto-accepted, both resolved the same checkpoint and both opened the same partial file - 
`resume plan found by content id=01a1958f... blocks_done=23`
 then two 
`prepare_sink: resuming into the existing file`
 lines with different 
`transfer_id`
 but the same 
`owner`
 - so neither finished. Not a defect to fix: the harness queued the same file twice. Worth knowing because the logs make the collision obvious once the 
`owner`
 field is there. |
| T-M07 cannot be driven from adb on this build | Both routes are closed: `pm revoke com.thep0y.fluxy android.permission.ACCESS_LOCAL_NETWORK` reports `Unknown permission` - it is not in the runtime permission table - and `am compat enable RESTRICT_LOCAL_NETWORK com.thep0y.fluxy` reports `Cannot override a change on a non-debuggable app and user build`. A signed release on a user build is exactly the case that cannot be forced into the denied state, so the settings banner added earlier (`networkDenied` driving `settings.lanTitle`) stays code-complete and behaviour-unverified. Verifying it needs either a debug APK of the same code or a person turning the switch off in system settings. |
| T-M02: a 10 GiB receive onto the phone works, verified by driving the desktop over CDP | The desktop UI had no automation, so the send was issued through the WebView2 debug port: 
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
 exposes 
`__TAURI_INTERNALS__`
, which accepts the generated commands directly - note they are snake_case (
`state_snapshot`
, 
`register_file`
, 
`start_transfers`
) rather than the camelCase names in 
`bindings.ts`
. 
`register_file`
 and 
`start_transfers`
 then drove the whole path: the phone showed 
`THEPOY 想发送 tenGB.zip（10.0 GB）`
 with the line 接收后文件会写入本机的接收目录；本机剪贴板不会被改动, accepted, and the run finished at 
`transfer finished file=tenGB.zip bytes=10737418240 elapsed_ms=365148 mib_per_s=28.04`
. 
`/storage/emulated/0/Download/Fluxy/tenGB.zip`
 is exactly 10737418240 bytes. The phone is slower than the desktop (28 vs 48 MiB/s) because it is writing to public storage. |
| Receiving deliberately skips MediaStore, and that is correct | The file does not appear in 
`content://media`
 under 
`Download/Fluxy/`
, which looks like a missing export until you read 
`receiveDir()`
: with all-files access it returns the public 
`Download/Fluxy`
 folder and writes there directly, so the copy-and-publish dance in 
`export()`
 never runs. That satisfies T-M02's bar - the file is reachable in the file manager and no 
`is_pending`
 entry is left behind - and a .zip is not a media type, so its absence from the media index is expected rather than a defect. |
| The offer timeout starts at the sender, not at the dialog | The first accept failed with 
`the offer expired before it was answered`
 (code 2001) although only 45 s had passed since the phone displayed the prompt. 
`OFFER_TIMEOUT`
 is 60 s and starts when 
`start_transfers`
 runs, while the prompt only appears after discovery and session setup - about 26 s in this run - leaving roughly 34 s to react. A second attempt that tapped within a few seconds completed normally. Worth knowing before treating this as a bug: the window a person actually sees is much shorter than the constant suggests. |
| The phone had no clipboard page, so a desktop share could never be answered | 
`mobileNav`
 listed 设备, 消息, 传输 and 设置; 
`/clipboard`
 existed for the desktop only. The default receive policy is 
`ClipboardPolicy::Ask`
, so an offer from a desktop reached the phone, was classified 
`OfferDecision::Ask`
, and expired with nothing on screen to accept it - the sender's 
`clipboard_share`
 even reported 
`state: offered`
. Receiving text from another device is the core of the feature, so 
`mobileNav`
 now includes it; the page needed no other change and renders correctly at phone width, with the per-device 询问 / 自动接收 / 拒绝 selector and the incoming-offers list. |
| The offer still does not arrive: a remaining gap | After adding the page and opening it, three sharing attempts produced no 
`FluxyClip`
 line, no notification, and no 
`ClipboardOffer`
 in the phone's log - only discovery traffic. The desktop side returned a valid offer each time (
`offer_id`
, 
`preview`
, 
`expires_at_ms`
 all populated), so 
`share_clipboard_text`
 reached 
`sender.send(ChatCommand::ClipboardOffer)`
 without error, yet nothing crossed. This needs the desktop's session state at the moment of the send: 
`chat session ended ... control stream closed`
 appears repeatedly in that log, so the likeliest cause is a session that had already dropped and an 
`ensure_session`
 that handed back a sender whose queue was gone. The new 
`FluxyClip`
 log in 
`ClipboardBridge.write`
 will confirm the receiver side the moment one does get through. |
| The clipboard gap is session establishment, not the clipboard code | A decisive follow-up: `chat_open` returned successfully, `clipboard_share` then returned a well-formed offer, and the desktop log for that minute contains no `chat session live` and no `connect` line at all - the same silence as every earlier attempt. So the clipboard pipeline is intact right up to `ensure_session` handing back a sender, and what fails is bringing the session up in the first place. This is the same intermittent `incoming pairing connection accepted` / `control stream closed` pattern seen around the T-M06 runs, now with a cleaner reproduction: no session, no offer, and the sender's queue accepts a command nobody will read. Chasing the clipboard further without fixing session establishment would be another round of guessing. |
| Found and fixed a real addressing bug: an inbound session recorded the peer's ephemeral port | 
`ChatPeer`
 was built on the accept path with 
`addr: connection.remote_address()`
. That is the source port of the peer's outgoing QUIC socket, not the port it listens on - measured here as 
`10.15.131.208:57574`
 against a real listener on 
`49784`
. The consequences were exactly the symptoms seen for hours: the phone logged 
`peer moved; old=...:57574 new=...:49784`
 every time discovery corrected it, 
`chat session live ... remote=10.15.131.208:57574`
 announced a healthy session, and every message written to that 
`connection`
 went nowhere. The accept path now uses 
`freshest(device_id, remote_address)`
 so discovery's listening port wins and the ephemeral one is only a last resort. After the fix the 
`peer moved`
 line disappears entirely. |
| Two session defects are now visible instead of silent | 
`run_outbound`
 dialled with no logging at all, so a refused connection, a failed handshake and a healthy session were indistinguishable - and 
`ensure_session`
 returned a sender without anyone noticing the worker had already exited. Both now log: 
`chat session: dialling`
, 
`chat session: connected`
 and 
`chat session: connect failed`
 with the attempt number and address, and 
`ensure_session`
 already discarded a closed sender so a fresh worker starts. The first run with these confirmed the dial path works end to end (
`dialling -> connected`
 in 20 ms). The clipboard offer still did not reach the phone in that run, so one more layer remains between 
`connected`
 and the peer processing the frame; the new logging narrows it to 
`identity_and_trust`
 or the command loop rather than the network. |
| The desktop side of a clipboard offer is now fully traced, and it works | `chat_open` produced `chat session: dialling -> connected` in 20 ms and `chat session live`, then `clipboard_share` produced `clipboard: sending an offer bytes=28` and `clipboard: offer sent` - the command reached the loop, the frame was written, nothing errored. Every silent step in that path now logs, so the desktop half of the feature is proven rather than assumed. || The phone never runs handle_incoming for a trusted resume, and the logs say so | The remaining asymmetry is stark: with the phone on a freshly installed build (`lastUpdateTime=2026-11-01 16:11:28`, pid 24707), its entire 94-line log buffer contains only `nearby map after upsert`, `nearby snapshot requested` and `UDP fallback discovered a peer` - not one `incoming pairing connection accepted`, not one `chat: accepting an inbound session`. Meanwhile the desktop reports a completed `resume handshake completed`, `control stream open` and `chat session live` against `10.15.131.189:49784`. `handle_incoming` does have the right branch for this case (`Ok(session) if challenge.is_none()` calls `chat.accept`) and `server_pair_attempt` correctly skips SAS when `hello.resuming`, so the logic is not obviously wrong. What is missing is evidence that the accept loop runs at all on the phone after a restart - the next probe belongs on the endpoint accept path in `network.rs` before `handle_incoming`, not on the clipboard. |
| The inbound path is confirmed up to ControlSession::accept | With the phone freshly restarted the whole accept chain now appears in one place: 
`the accept loop is running`
, 
`the accept loop took a connection remote=10.15.131.208:63432`
, 
`incoming pairing connection accepted`
 and 
`chat: accepting an inbound session peer=01a16178-...`
 - all within 35 ms. That settles the earlier contradiction: the loop does run, the port is bound (
`00000000:C278`
 = 0.0.0.0:49784) and the connection is taken. The earlier "nothing at all" readings were logs from the previous process that 
`logcat -c`
 had already cleared. |
| / The handshake finishes but run_connected never starts | Everything stops after 
`chat: accepting an inbound session`
. 
`run_accepted`
 then calls 
`ControlSession::accept`
, whose first statement logs 
`waiting for the peer control stream`
 - and that line never appears, nor does 
`chat session live`
 from 
`run_connected`
. So the gap is between entering 
`run_accepted`
 and entering 
`ControlSession::accept`
, which is three statements wide: 
`self.identity()`
, then 
`ControlSession::accept`
. A phone-side 
`SO_BINDTODEVICE`
 failure is also logged at startup (
`unable to pin the socket to the chosen interface interface=any error=No such device`
), which kills mDNS but demonstrably does not stop the QUIC endpoint. The next probe goes inside 
`run_accepted`
 before the accept call. |
| The inbound session comes up, but the transport is slow and lossy | With the probe in 
`run_accepted`
 the full sequence finally appears on the phone: 
`chat: waiting for the inbound control stream`
 at 09:44:14.908, then 
`peer control stream accepted`
 at 09:44:24.919 - ten seconds later - then 
`chat session live`
. The desktop had already logged 
`control stream open`
 at 09:43:52, so 
`open_bi`
 to 
`accept_bi`
 took 32 seconds on a link the desktop considered established. A second signal points the same way: the phone logs 
`unable to pin the socket to the chosen interface interface=any error=No such device`
 at startup, so its QUIC socket is not bound to a specific interface on a hotspot that has both 
`wlan0`
 and 
`ap0`
. Two handshake streams eventually crossed, but the clipboard offer sent at 09:45:15 never reached the phone and no 
`SessionEvent::ClipboardOffer`
 was recorded. That is a transport problem on this hotspot, not a logic error in the chat or clipboard code. |
| What is now proven, and what is left | Proven by logs, in order: the accept loop runs, the endpoint is bound to 
`0.0.0.0:49784`
, a connection is taken, 
`handle_incoming`
 runs, 
`chat.accept`
 runs, 
`ControlSession::accept`
 returns and the session reaches 
`live`
 on both sides. So every layer of the inbound path is wired. What remains is reliability: an unbound socket on a multi-interface hotspot stretches a stream handshake to 32 s and drops a small control frame entirely. Fixing that means binding the phone's QUIC endpoint to the right interface, which is exactly the 
`SO_BINDTODEVICE`
 call that fails today - Android does not offer it - so the alternative is to bind the endpoint to the hotspot address explicitly instead of to 
`0.0.0.0`
. |
| The core clipboard semantic is verified on Wi-Fi: text sent from the desktop is written to the phone's system clipboard | 
`chat_send`
 from the desktop delivered, and the phone logged 
`received text copied to the clipboard bytes=26`
. That is the behaviour the product is built around - copy on one device, paste on the other - and it works through the message path, which calls 
`clipboard.write_local`
 unconditionally on a fresh message (
`chat.rs`
 1356). It is independent of 
`ClipboardPolicy`
 by design: the policy governs offers, not delivered messages. This also proves the control stream carries frames in both directions, which is what made the separate offer failure look like a transport problem when it is not. |
| What the offer path still needs | 
`clipboard_share`
 logs 
`sending an offer`
 and 
`offer sent`
 on the desktop while nothing appears in the phone's 
`pendingIncoming`
 list, even with the session already live and text messages flowing. So the gap is inside the offer handling, not the stream. Two candidates remain and both are cheap to check next: the offer frame reaching 
`handle_envelope`
 is classified but never surfaced as 
`SessionEvent::ClipboardOffer`
, or 
`ChatCommand::ClipboardOffer`
 is queued on a sender whose command loop had already ended for that generation. The second is now distinguishable because 
`run_commands`
 logs when it dequeues an offer. |
| A measured transport oddity worth recording | Inbound control stream setup takes 10 s on Wi-Fi as well as on the hotspot: 
`waiting for the inbound control stream`
 at 17:56:09.588, 
`inbound control stream accepted`
 at 17:56:19.605. The desktop had opened its side 32 s earlier. Round-trip latency to the phone is 4-22 ms and the socket is not interface-bound, so this is not packet loss on the path - 
`accept_bi`
 simply is not waking up promptly. It delays the first frame but does not lose it, since text messages sent after the stream came up arrive normally. |
| Fixed: the Android listener now binds the real interface address | Two gaps stacked. 
`choose_binding`
 had an Android branch that returned 
`name: "any"`
 and 
`0.0.0.0`
 unconditionally, because the desktop interface table is unreadable there (SELinux). The platform plugin had the answer all along - 
`NetworkInfo.describe()`
 already collected 
`linkAddresses`
 - but 
`localNetworkInfo`
 never copied 
`addresses`
 into its reply, and 
`LocalNetworkInfo`
 had no field for it. Both are fixed, and the list is now built as a 
`JSArray`
 because a Kotlin 
`List`
 serialises to a string and fails deserialisation. Verified: 
`the listener binds the platform address address=192.168.0.101 name=wlan0 candidates=4`
 and 
`/proc/net/udp`
 shows 
`6500A8C0:C278`
 - 192.168.0.101:49784, no longer 0.0.0.0. On Android 
`SO_BINDTODEVICE`
 is unavailable, so the bound address is the only lever there is. |
| The 32 s inbound delay is NOT the wildcard bind, and is still open | Measured after the fix: desktop 
`control stream open`
 at 10:39:28.199, phone 
`accept loop took a connection`
 at 10:39:50.606 - still 22.4 s - then 
`inbound control stream accepted`
 at 10:40:00.742, another 10.1 s. So the address binding was a real defect worth fixing (mDNS, path ambiguity) but it does not explain the latency. 22.4 s looks like QUIC retransmission backoff on the connect, and the second 10.1 s is a separate wait inside 
`accept_bi`
. Ruled out so far: CPU sleep (
`mWakefulness=Awake`
, no Wi-Fi power save, 
`PARTIAL_WAKE_LOCK`
 held), keep-alive interval (lowered 10 s to 1 s with no change), packet loss (RTT 4-22 ms), and the accept-loop timeout (no 
`not accepted`
 line, so 
`endpoint.accept()`
 itself did not fire early). |
| The inbound delay is a fixed 27.8 s inside endpoint.accept(), not packet loss | Instrumented 
`transport::accept`
 to time the two waits separately. Four consecutive fresh-process dials gave 
`queued_ms = 27828 / 27876 / 27823 / 27765`
 with 
`handshake_ms = 5-8`
. Two conclusions follow directly. First, the variance is 50 ms out of 27.8 s, so this is a fixed delay and not loss, retransmission, or jitter. Second, the QUIC handshake itself takes single-digit milliseconds once 
`endpoint.accept()`
 returns - the 27.8 s is spent waiting for an 
`Incoming`
 to be surfaced at all, while the desktop considers the same connection established in 74 ms. |
| Loss, timers and CPU sleep are all ruled out | 
`ping -n 20 -l 1400`
 from the desktop to the phone: 
`0% loss`
, RTT 4-122 ms (the phone drops ICMP the other way, which is Windows' default and irrelevant to UDP). Lowering the QUIC 
`keep_alive_interval`
 from 10 s to 1 s changed nothing. CPU sleep is out: 
`mWakefulness=Awake`
, no Wi-Fi power save, and a 
`PARTIAL_WAKE_LOCK`
 is held. The accept loop's own 10 s timeout never fires - no 
`incoming connection was not accepted`
 line appears - so 
`endpoint.accept()`
 is genuinely silent for 27.8 s rather than timing out and retrying. |
| What that leaves | The delay sits between the UDP socket and quinn's driver: packets arrive (0% loss, correct address bound) but 
`Incoming`
 is not raised for 27.8 s, and the phase-locked repeatability suggests a timer rather than a race. The bounded whitespace is small enough to probe directly - a 1 s 
`tokio::time::interval`
 tick call next to the accept loop, logging whether it fires during the wait, would show whether the runtime is scheduling the task at all. If the tick keeps running while 
`accept`
 stalls, the problem is inside quinn's driver; if the tick also stalls, the phone's runtime itself is not being scheduled. |
| Three more hypotheses eliminated, leaving quinn's Incoming queue | The 1 s tick probe settles the runtime question: during a 30 s wait it fires on schedule (
`ticks=25 elapsed_ms=24003`
, 
`ticks=30 elapsed_ms=29003`
), so the task is being polled and the async runtime is healthy - only 
`endpoint.accept()`
 stays silent. Next, the listener binds fast: 
`cat /proc/net/udp`
 shows 49784 after 
`1 s`
, so dialling before the port exists is not it either - and dialling eight seconds after the bind still measured 
`queued_ms=27888`
. Packet loss is already excluded (0% at 1400 bytes), CPU sleep is excluded (awake, wake lock held), and the address is now bound correctly. What remains is the narrowest version of the problem: the UDP socket receives the client's Initial, the driver answers it (the desktop completes the handshake in 74 ms), and yet no 
`Incoming`
 is raised for 27.8 s - with a spread of only 50 ms across four runs, which is the signature of a timer rather than a race. |
| The select wrapper is not the cause, and neither is the idle timeout - but the idle timeout changes the symptom | Two controlled experiments. First, removing 
`tokio::select!`
 entirely and awaiting 
`endpoint.accept()`
 directly still measured 
`queued_ms=30034`
, so the polling pattern is innocent. Second, setting 
`max_idle_timeout`
 to 5 s and then 60 s changed the outcome completely: with either value 
`Incoming`
 was 
`never`
 raised at all (no 
`quic incoming surfaced`
 line in 25 s or 75 s of waiting), while reverting to quinn's default of 30 s restored 
`queued_ms=30329`
 on the next run. An explicit idle timeout that suppresses the very event it should trigger, in both directions, is not behaving like an idle timer at all. |
| Where this leaves the investigation | Every application-level explanation is now excluded by measurement: packet loss (0% at 1400 bytes), CPU sleep (awake, wake lock held), keep-alive interval (1 s vs 10 s, no change), a wildcard bind (the listener is pinned to 192.168.0.101), runtime starvation (a 1 s tick kept perfect time throughout the wait), premature dialling (the port binds in 1 s, and dialling 8 s later still measured 27.9 s), and the 
`select!`
 wrapper (removing it changed nothing). What is left is a quinn-level effect: the UDP socket receives the client's Initial and the driver answers it - the desktop completes its handshake in 24-74 ms - yet 
`Endpoint::accept()`
 does not yield an 
`Incoming`
 for 27.8-30 s, with 50 ms of spread across five runs. The idle-timeout experiment above points at quinn's connection-table housekeeping rather than at our code. Next step is upstream: reproduce against a newer quinn, or against a desktop-to-desktop pair, to see whether this is version-specific or Android-specific. |
| The inbound delay is Android-specific, confirmed by reversing the roles | Made the phone the QUIC client and the Windows desktop the server. The desktop raised `Incoming`, completed the handshake in `handshake_ms=6` and reached `chat session live` in 39 ms. Same quinn 0.11.12, same `transport_config()`, same TLS, same code - the 28-30 s only appears when Android is the server. That closes the last open question in `docs/quinn-inbound-latency.md`, which is now a self-contained upstream report: versions, six-run measurements, eight excluded hypotheses with the measurement that excluded each, and the anomaly where any explicit `max_idle_timeout` suppresses `Incoming` entirely. |
| T-M04 passes: process death, restore and resume all verified on the phone | Killed the app with 
`am force-stop`
 mid-transfer at 13% of a 2 GiB receive (
`am kill`
 is not enough - the FGS protects the process, which is itself the intended behaviour). After a restart the app logged 
`restoring an interrupted transfer`
 and 
`restored interrupted transfers from checkpoints restored=1`
, and the transfer page showed 
`已中断`
 with 
`260 MB / 2.0 GB`
 - the exact byte count, not a guess. Re-sending the same file then produced 
`checkpoint at receive time present=true blocks_done=65 matches=true`
 and 
`resuming from a stored checkpoint blocks_done=65`
, and the progress jumped straight to 28% because the first 13% was already on disk. It ran to 
`completed`
 at 55 MB/s. The 65 blocks were never re-sent. |
| Fixed: an Interrupted row could never be re-offered | 
`handle_offer`
 routes a re-offered id through 
`emit_offer`
 unless the existing row is 
`Failed`
 or 
`Preparing`
. 
`Interrupted`
 was missing from that list, so the re-offer waited for an accept button that the UI deliberately does not render for that state - the row sat until the offer expired. That contradicted the intent recorded for A-08 ("a re-offer with the same id picks the checkpoint back up"). Adding 
`Interrupted`
 to the match fixes it: the checkpoint is on disk, so there is nothing to ask the user about. |
| Fixed: a restored row was titled with the transfer id | 
`restore_interrupted`
 falls back to 
`id.to_string()[..8]`
 when the peer name cannot be resolved, but 
`id`
 there is the *transfer* id, so the transfer page showed 
`接收 · 01a1974b`
 - a device the user has never seen. The fallback now uses 
`device_id`
, which reads as 
`01a16178`
 and at least names the right peer. The deeper issue remains: 
`known_peer`
 returned nothing here even though the device is paired, so the real name still does not appear; that lookup needs its own look. |
| T-M11 passes: background clipboard reads are denied by the platform, foreground sharing works | With auto-send enabled (the switch says "checks every 300 ms") the app was sent to the background while its clipboard held text. Android denied every read itself, which is the decisive evidence: `ClipboardService: Denying clipboard access to com.thep0y.fluxy, application is not in focus nor is it a system service for user 0`. Nothing was sent, nothing crashed, the process stayed alive. Bringing the app forward and pressing the manual share button then produced `clipboard: sending an offer bytes=26` and `offer sent`. `ClipboardBridge.read()` relies on `primaryClip` returning null in the background, which is exactly what the platform gives - no polling guard is needed, and none exists. |
| T-M05 passes: the foreground service stops cleanly when its budget expires | Set 
`TransferService.TIMEOUT_MS`
 to 60 s (its real value is six hours) and started a 2 GiB receive. After the timeout: the 
`TransferService`
 record is gone from 
`dumpsys activity services`
, the process is still alive (pid unchanged), and logcat has no 
`FATAL`
, 
`AndroidRuntime`
 or 
`ANR`
 line. The wake lock is released in 
`onDestroy`
, so stopSelf is a complete shutdown rather than an orphaned lock. The constant is back to six hours. |
| Found while testing T-M05: a same-named file fails with a generic 3001 | The receive that ran during the T-M05 window reached 100% (
`2.0 GB / 2.0 GB`
) and then failed with 
`code=transfer 3001`
, whose copy reads "the stream was interrupted, or the peer declined". The detail line says what actually happened: 
`destination already exists: fluxy-tm04.bin`
 - the file from the earlier T-M04 run was still in the receive root. Nothing was interrupted and nobody declined, so the message points the user at the wrong cause. A duplicate name deserves its own code and its own sentence, and arguably an automatic rename instead of a failure. Fixed in the same slice - see below. |
| T-M15 cannot be verified in this environment, and part of it cost the device | The phone has only `wlan0` - `mDataRegState=OUT_OF_SERVICE` and `mDataConnectionState=-1` say there is no cellular data at all, and `ip link` shows no `tun` or `ppp` device, so no VPN is configured. Without either, the two front halves of T-M15 (VPN at startup, Wi-Fi off onto cellular) have nothing to exercise. The third clause - network loss and recovery - also could not be finished: adb on this setup is itself a Wi-Fi link, so `svc wifi disable` cut the tool out of the device before it could observe the app. The phone stayed discoverable for a moment via `adb mdns services` and then went away entirely; it needs Wi-Fi switched back on by hand. T-M03 (SAF revocation) depends on the same adb channel, so it is paused too. |
| Fixed: receiving a file whose name is taken now renames instead of failing | `DesktopFileSink::finalize` refused to commit when the destination existed, so the second copy of any file failed with the generic `3001`. `available_path` now picks the first free `"name (n).ext"` and the rename targets that, which is what a file manager does and what the user expects from "send it again". The check-then-rename is deliberately not atomic: the rename is what claims the name, and a real collision there fails loudly rather than overwriting. Verified on the phone - sending the same 2 GiB file a third time produced `fluxy-tm04 (1).bin` next to an untouched `fluxy-tm04.bin`, and the transfer reached `completed`. Three tests cover it: the rename, counting up across repeats, and a name with no extension. |
| T-M03: revoking SAF access mid-transfer does not interrupt it, and that is correct | Revoking the URI grant while a transfer was running left the transfer untouched - it kept going and finished. This is not a defect: the sender already holds an open file descriptor from the SAF grant, and revoking a grant does not close descriptors that are already open. The process keeps reading through the fd it has. T-M03's stated bar ("returns 3003, keeps the checkpoint, prompts for re-selection") therefore cannot be reached by revoking authorisation mid-flight, because nothing re-opens the URI while the fd is alive. The bar is reachable on the paths that `do` re-open it: a resume after the process died, or a second send of the same file after the grant is gone. Those are the cases worth running, and they need the grant revoked `before` the open, not during it. |
| UI refactor: the first screen now holds only what a first-time user needs | The home page carried three side panels - device id, protocol version, listen port, discovery mechanism, data directory, a LAN-scope diagram and live event counters - beside the two things that matter. All of it is diagnosing detail: useful the one time something breaks, noise every other time. Those facts moved into Settings' diagnostics section (which already existed for the sequence counters), the scope diagram and channel panel were deleted outright, and the home page is now the conversation plus the nearby-device list. |
| UI refactor: navigation went from five items to four | The rail and the phone bar both listed 设备 / 消息 / 传输 / 剪贴板 / 设置, plus 帮助 on desktop. Clipboard became a passive feature - it receives text without being asked, so a standing entry was one more thing to explain - and help is read once. Both are now buttons inside Settings, and desktop gained the 消息 entry it never had while the phone lost its clipboard tab. Ten catalog keys that no longer had a consumer were removed rather than left as dead copy; 
`home.thisDevice`
 and 
`home.loopbackOn`
/
`home.loopbackOff`
 were kept because the diagnostics panel uses them. |
| Touch scrolling on a tablet: the WebView may be dropping the only height rule | 
`.fx-shell`
 was sized with 
`height: 100dvh`
 alone. A WebView that does not know 
`dvh`
 discards the declaration, the shell then has no height at all, every grid row grows to fit its content, 
`.fx-content`
 never becomes a scroll container - and since 
`body`
 is 
`overflow: hidden`
 the page cannot be scrolled by touch at all, while taps still land. A 
`100vh`
 line was added in front as the fallback, 
`touch-action: pan-y`
 was set on the scroll container so a gesture starting on a child is not claimed by it, and 
`overscroll-behavior: contain`
 keeps the page from chaining to the shell. Not yet confirmed on the tablet that reported it. |
| Mobile: text and files now share one conversation stream | The two lived on separate pages, so a conversation had to be pieced together from two screens. 
`ChatWorkspace`
 now merges the peer's messages and transfers into a single list ordered by start time, and a transfer renders as a card in that list - icon, name, size, state, and a progress bar while it runs - with the same sent/received alignment as a text bubble. Verified on the phone: a text sent from the desktop and a 2 GiB file appear one under the other in the same thread. |
| Mobile: one screen at a time instead of a letterboxed list | The compact breakpoint kept the desktop's two-pane layout and merely capped the list at 218px, which on a 574x576 square viewport squeezed the thread into a strip. Below 768px the workspace now shows either the conversation list or the thread, never both, with a back button in the thread header - the same model a chat app uses. |
| Mobile: the brand bar is gone | It occupied roughly 13% of a square screen while holding a product name the launcher icon already conveys, one status dot, and two buttons that Settings already offers. Hidden below 768px. |
| Two height bugs that made every page look half-empty | Both have the same cause and both cost real time to find. Hiding the topbar with 
`display: none`
 stops it being a grid item, so 
`.fx-content`
 slid into the 
`auto`
 row of 
`.fx-workspace`
 and the 
`1fr`
 row went to the (also hidden) statusbar - the content then sized itself from its content. The same trap hit the chat when 
`is-compact`
 hides one of its two panes: whichever pane stayed visible landed in the 
`auto`
 row of a two-row template. Both now declare the row count that actually remains. The second fix also had to match 
`.fx-page-chat .fx-chat`
's specificity, because that rule is declared later in the file. Measured after the fix at 574x576: 
`content 519, page 491, chat 491, pane 489, messages 299, composer 123`
 - the composer sits on the bottom edge instead of leaving a third of the screen blank. |
| The receive prompt is now global, which it never was | The accept/reject dialog lived inside 
`TransfersPage`
, so an offer only produced a prompt while that page happened to be mounted. A file sent to a phone sitting on the device list arrived, waited 60 s, and expired with nothing on screen to answer it - the transfer then failed with "the offer expired before it was answered", which looked like a sender problem. The prompt is a property of the connection, not of a page, so it moved to 
`IncomingOfferDialog`
 and is mounted once in 
`AppShell`
. Verified: with the phone on the device page, a desktop send raises the dialog over it. |
| The transfers page is gone; files live in the conversation | With text and files already sharing one stream, a separate transfer list was a second place to look for the same thing. The route, the nav entry, the page and its test were removed, and 
`chat.back`
-adjacent copy that only that page used was dropped - 74 catalog keys in all, which 
`catalog.test.ts`
 flagged rather than letting them rot. Navigation is now 设备 / 消息 / 设置 on both form factors: pick a peer, talk to them, move a file - a transfer is something that happens inside a conversation, not a destination. |
| The empty band above every page was the offer dialog, not layout CSS | Both the device page and the message page showed ~100 px of dead space at the top, and I spent a while blaming the height chain, the topbar's 
`display: none`
 and the workspace row template. Instrumenting 
`.fx-shell`
 settled it: 
`grid-template-rows`
 computed to 
`101.773px 474.227px`
 - two rows for a shell that has exactly one child. 
`IncomingOfferDialog`
 was mounted *inside* 
`.fx-shell`
, and its dialog wrapper occupies a grid item even while closed, so it claimed a whole row. Moved it to be a sibling of the shell: 
`rows`
 is now 
`576px`
, 
`.fx-workspace`
 and 
`.fx-content`
 both start at top 0, and the device page's title moved from y=330 to y=130 on the phone. One fix cleared both pages, because the cause was global. |
| A stylesheet block I added was scoped to phones by accident | The file-card rules (
`.fx-chat-file`
 and friends) were meant to be global but were inserted after 
`.fx-page-chat {`
, which lives inside the 
`@media (max-width: 768px)`
 block - so the desktop rendered a file in the conversation with no styling at all. Moved the whole block to the top level and verified with a brace count that it is no longer nested. |
| The conversation list previews the last thing that happened, file or text | It only ever read `last_message`, so a conversation where the last thing was a send looked empty - which is exactly the row a user is watching for. `lastPreview` compares the newest transfer's timestamp with the newest message's and shows whichever is later, rendering a file as `📎 name` and using that item's own time in the row. Verified on the phone: after accepting a 2 GiB file the row reads `THEPOY · 📎 fluxy-tm04.bin · 17:36`, while a peer with nothing still shows 暂无消息. |
| Touch scrolling: verified by structure at every breakpoint, not by finger on the reported tablet | Three widths were measured through the WebView debugger - 1024x768 and 800x1280 (desktop layout, rail visible) and 574x576 (compact) - and in all three 
`.fx-content`
 reports 
`overflow-y: auto`
 with 
`touch-action: pan-y`
, 
`canScroll: true`
, and a 
`scrollTop`
 that actually moves (0 to 200 on the tablet widths). Walking the ancestor chain found no 
`touch-action: none`
 anywhere: content is 
`pan-y`
, workspace and shell are 
`auto`
, body is 
`auto`
. The one ancestor with 
`overflow: hidden`
 is 
`.fx-shell`
, which sits above the scroller and therefore cannot block it. On the 1:1 device a real injected drag moved the settings page from its top to the notifications section. |
| What is still unproven about the tablet | The structural evidence above is strong but it is not the same as a finger on the specific tablet that reported the problem, which this environment has no access to. If it still will not drag, the next suspects are outside CSS: a WebView that intercepts the gesture for its own scrolling, or the app's window being marked non-scrollable by the platform shell. The 
`100vh`
 fallback before 
`100dvh`
 and the explicit 
`pan-y`
 both remain in place; neither can be confirmed or denied without the device. |
| Device page: two peers per row, and the header collapsed to one line | On a square screen a single peer per row spent half the width on a name and an address that rarely need it. The list is now a two-column grid, each card stacking its content and stretching its button, and the title shares a row with `name · port` instead of sitting above a second block. || Device page: a way to add a peer that discovery cannot find | Next to the manual-address form there is now an 添加设备 card with two actions. 扫码添加 is present but disabled with a note saying scanning is still being built - the entry point exists so the flow is visible, the behaviour does not. 我的二维码 renders this device's pairing payload as a QR code in a dialog, and shows the address a peer would dial underneath it. || The QR payload starts a connection but does not grant trust | It carries `v, t=fluxy-pair, id, name, addr, port` and deliberately no fingerprint: a scanned code tells the other side where to connect, and the pairing still ends in the same SAS comparison on both screens. A photographed code therefore cannot silently pair anything. || A QR library took the whole UI down | `qrcode@1.5.4` assigns to `Object.prototype.toString` at import time, which is a TypeError under strict mode - the bundle died before rendering, so the phone showed a blank page and the WebView console showed nothing useful because the failure was at module scope. Replaced with `uqr`, which is pure ESM and synchronous (`renderSVG(payload, { border: 1 })`). Worth remembering: a dependency can crash the app on import, not just when used. |
| A declined transfer showed as a bare 失败 | The sender records `state=failed, error_code=rejected, code_number=3001`, and the conversation card rendered only the state. The cause was a dead function: `failureReason()` in `lib/transfer-errors.ts` maps a protocol code to localized copy, but when `TransfersPage` was deleted it lost its only caller and nothing noticed, because the catalog test only proves the copy exists. The file card now renders the mapped reason under a failed transfer, so a rejection reads 接收端拒绝了这次传输 instead of 失败. || A test now covers the rejection mapping specifically | `rejected` is the one code whose state is `failed` and whose number is the generic 3001, so it is exactly the case that any state- or number-based rendering gets wrong. `transfer-errors.test.ts` asserts the key resolves and that the copy says 拒绝 / declined in both locales. Confirmed on hardware by the user: a rejected send shows the reason instead of a bare failure. |
| Auto-accept is now a switch on both platforms, not a config-file secret | `auto_accept_transfers` existed since the transfer list was built, but the only way to change it was to edit `fluxy.json` - there was no UI anywhere, on either surface. That made an unexplained behaviour: a file arriving with no prompt looked like the prompt was broken. It is now a panel in `LocalDeviceSettings`, the component the desktop settings page and the phone settings page both render, so one change covers both. Wired end to end: `set_auto_accept` command, `set_auto_accept_transfers` on the config, the field on `SettingsSnapshot`, and the switch itself. Verified on the phone: default off shows 每次询问, tapping it turns on and toasts, and the value survives a force-stop and relaunch. |
| Settings: one setting per line instead of one card per setting | Every setting used to get its own panel with a heading, a paragraph and a full-width control - roughly three lines of height for one choice. `SettingsRow` puts the label and its explanation on the left and the control on the right, so appearance and language now share one panel as two rows, and the receiving group (notifications, auto-accept, receive directory) is three rows instead of three panels. || Settings: the read-mostly panels moved to a second page | Network interface, device identity, discovery diagnostics, the diagnostic counters and the log tail were five panels of detail nobody changes, and on a phone they pushed every actual switch four scrolls down. They now live at `/settings/advanced` behind one entry row; the main page keeps appearance, language, name, receiving and the identity rotation. `SettingsPage` went from 414 lines to about 200. || Trusted devices moved to the device page | A list of who can reach you belongs where you see who is around, not in settings. It is a `<details>` disclosure at the bottom of the device page: one summary line until someone opens it. |
| The fixed tab bar covered the end of every scrollable page | `.fx-mobile-nav` is `position: fixed` and reserved `57px`, but its own box is `min-height: 57px` *plus* 5px padding above and below *plus* the safe-area inset - so the reserved strip was always shorter than the bar and the last card sat underneath it. The height now lives in one variable, `calc(67px + env(safe-area-inset-bottom, 0px))`, used by `.fx-workspace`'s padding, and the scroller got `scroll-padding-bottom` so the final card does not sit flush against the reserved strip. Verified by scrolling the settings page to its end: the rotation card and the footer line are fully visible above the bar. |
| Clipboard, advanced and help were two badly-joined cards | They were three destinations rendered as three panels, and the middle one had been spliced against the first, so the pair read as a single card with a stray 打开帮助 button belonging to nothing. All three are now rows of one 更多 panel: label and explanation on the left, the link on the right. Help also got its own copy instead of borrowing the clipboard sentence. |
| Desktop lost device management entirely | Removing the home side panels also removed `TrustedDevices`, and nothing put it back on the desktop: the phone had a trust list, the desktop had no way to see or revoke a trusted peer at all. It is back on `HomePage` as the same collapsed `<details>` the phone uses. || Phone device page: one card per device was mostly empty space | A single peer sat in a two-column grid, so half the row was blank, and the card stacked name over address over a full-width button - three lines for something you usually just tap. The list is a single column of horizontal rows now: name and address on the left, the button on the right, at 40px. || Phone device page: the add-device card carried a redundant sentence | With the scan button already disabled and labelled, the paragraph underneath repeated it. Removed; the card is two action buttons plus the manual-entry link. |
| An expanded disclosure squeezed its neighbours instead of growing the page | `.fx-content > .fx-mobile` had `flex: 1` with `min-height: 0`, which pins the page to the viewport height and lets it shrink. Expanding the trusted-devices block therefore did two wrong things at once: its own content overflowed the box and got clipped, and the grid rows re-divided the fixed height so the cards above shrank. Changed to `flex: 1 0 auto; min-height: 100%` - fill the viewport when short, grow when tall - which is what a phone page that scrolls actually needs. Verified on the phone: the disclosure opens with all of its content visible and the two cards above keep their height. |
| The pairing dialog spent half a phone screen on two short strings | It drew this device and the peer as two stacked cards joined by a dashed bridge, each with a kicker label above a name - four words of content in two boxes. On a 1:1 screen that pushed the dialog past the viewport, and the consequence was not cosmetic: the 取消 button fell off the bottom, so a user who changed their mind could not back out. The two names are one line now, `Titan 2 ↔ THEPOY` with the platform as a trailing note, and the whole dialog including both buttons fits. |
| QR pairing, both directions | The desktop had no way to show its own code - only the phone did - so the feature it was meant to serve had no target. `PairingQrDialog` now renders it on both surfaces and `src/lib/pairing.ts` builds and parses the payload. Verified end to end on hardware: desktop shows the code, phone scans it, both screens compare the six-digit SAS, both save trust. || The pairing code carries the peer's certificate fingerprint | Without it, scanning could only pair with a device discovery had already seen, which defeats the point of scanning on a network that filters multicast. The payload now includes `fp` and the scanner calls `pair_manual` directly. Trust is unchanged: the fingerprint pins the certificate, and the six-digit comparison still cannot be skipped. || `pair_manual` could not parse either form of address | It called `parse_address` (bare IP only) on the whole string and then took the port from the same string's tail, so `10.0.0.2` failed with "needs a port" and `10.0.0.2:5000` failed with "not usable". Manual pairing had never worked. The port is now split off first, with bracketed IPv6 handled. || The mobile pairing screen kept saying "waiting for the other device" after success | `waiting` was only cleared on timeout, cancel or error. Once the SAS appeared the screen showed a live comparison panel next to a stale "request sent" note. A `createEffect` clears it when a challenge exists. |
| Phone device page: the add-device card cost a third of the screen | Scanning and showing your own code are occasional actions that had a whole panel - heading, two full-width buttons, a manual-entry row - for about 200px of a 576px-tall viewport. Both are now icon buttons in the header (44px targets, aria-label and title, since the glyph carries the meaning). Manual entry - the one part that needs a form - moved under the peer list behind a divider, which is also where it belongs: it is the fallback for when that list is empty. |
## Verification

~~~text
cargo check --workspace                 # pass
cargo test --workspace                  # 304 Rust tests pass (2 ignored benchmarks)
cargo test --workspace --no-fail-fast (x3)  # 304 pass in one run; no failures under load
cargo test -p fluxy-core --release --lib benchmark_10gib -- --ignored --nocapture  # pass, 10 GiB in 7.23s
cargo clippy --workspace --all-targets -- -D warnings  # pass
cargo fmt --all --check                 # pass
bun run test:frontend                   # 91 Vitest tests pass
bun run check                           # pass
bun run check:security                  # pass, strict CSP/no opener/no HTML sinks
bun run build                           # pass
# settings rename + notifications        # pass, Enter saves a trimmed name, Escape reverts, an empty name is refused, and the toggle persists and mutes the notifier
# keyboard main flow                     # pass, skip link -> nearby pair -> SAS acknowledge/confirm -> offer accept -> chat send, every step on a native control; Escape cancels and focus returns to the trigger
# dialog keyboard contract               # pass, first control focused, Tab loops both ways, Escape closes, focus restored on close and on unmount
# i18n catalog guard                     # pass, zh/en parity, placeholder parity, used keys, dynamic state families, no hardcoded copy, no dead keys
# transfer detail dialog                  # pass, every task opens details; a failure shows the localized reason, code, and retryability, and retries from the dialog
# transfer failure reasons                # pass, all 16 protocol codes resolve in zh and en; an unknown code falls back instead of showing a raw key
# transfer notifications                  # pass, completion and failure notify while unfocused, a user cancel does not, and a same-device burst coalesces to one
# automatic clipboard watcher             # pass, off by default, debounced, never re-sends received content, never repeats a value
# tray menu and login item                # pass, check states follow the persisted switches, a rejected login item reverts the check and keeps the preference off
# close to tray                            # pass, a close request hides the window until Quit sets the deliberate-quit flag
# login item self-heal                     # pass, a missing item is re-created at startup only while the preference is on
# help page and firewall rules             # pass, live ports substituted into the platform commands, copy reports the result, login-item state shown
# shell navigation request                 # pass, the tray help request reaches the frontend and routes to /help, then is consumed
# startup backends                         # pass, reg.exe arguments, exit-code mapping, launchd/XDG escaping, and file write/remove
# interface ranking                        # pass, default route first, real adapters before VPN/hypervisor, tunnels only as a fallback, loopback never automatic
# interface naming across layers           # pass, the selection and the mDNS/fallback layers agree on the interface (friendly name on Windows)
# manual interface advertise                # pass, serve --interface WLAN binds 192.168.1.8, pins the socket, and reports discovery=mdns
# manual interface pair and chat            # pass, discover finds the WLAN service, pair completes SAS, chat resumes with delivered=true
# network interface settings                # pass, the panel explains the active pick, marks the recommended/virtual/down adapters, and confirms before switching
# network switch guidance                   # pass, a rejected switch keeps the proposal and shows the recovery steps; a lost manual address says so
# network selection rejection               # pass, an unknown mode, a bad address, and a blank or incomplete manual pick all return 2003
# folder path safety                       # pass, traversal, absolute paths, NUL/control/bidi, ADS, reserved names, duplicates (case-insensitive), file/dir conflicts, and root escapes are refused
# folder scan and placement                # pass, nested and Unicode paths, empty directories, symbolic links refused, a second batch root gets "(1)', and an entry-list mismatch is rejected
# folder batch UI                          # pass, one collapsible row per batch, files on demand, batch-wide pause touches only moving files, and Send folder goes through start_folder
# dropped folder                           # pass, a dropped folder is scanned, opens its confirmation, and does not consume the file budget
# drop accounting                          # pass, the file budget counts considered paths, so unreadable paths cannot push a drop past the limit
# loopback throughput (release)             # 242 -> 226-266 MiB/s across stream/window/MTU/socket sweeps; 1.0 GiB/s not met, 1 GbE target met
# throughput knobs                          # pass, streams 4..16, window 2..4, block 4 MiB cap, MTU 9000, 8 MiB UDP buffers all within noise
# cargo-deny licenses/bans/sources          # pass, 622 packages, no missing license, wildcards fixed in the workspace manifests
# cargo-deny advisories                     # pass (or --offline), no vulnerabilities; 5 unmaintained crates reasoned in deny.toml
# decoder fuzz                              # pass, 20k mutations per suite run; 10M (175 s) and 35M (about 10 min) campaigns clean
# security audit script                     # pass, bun run audit runs deny, fuzz, and the unsafe inventory
# release build                             # pass, target/release/fluxy.exe 10.5 MB, FileVersion 0.2.0, configured company/copyright
# NSIS installer                            # pass, Fluxy_0.2.0_x64-setup.exe 3.4 MB; silent install and uninstall both exit 0 and clean up
# installer signature                       # NotSigned; signing paths documented in docs/PACKAGING.md (release gate)
# CI matrix                                 # rust fmt/clippy/test/fuzz on three OSes, web on Linux, e2e on three, bundle on demand
# cross-target type check                   # fluxy-proto and fluxy-core pass for x86_64-unknown-linux-gnu from this Windows host
# documentation links                       # pass, 16 files / 55 relative links / 0 broken (bun run check:docs)
# soak (short runs)                         # pass, 3 min/90 rounds and 20 min/598 rounds, 0 failures, 4.7 GiB moved, 0.3-0.6 s per round, RSS 12-26 MB
# version consistency                       # pass, 0.2.0 across workspace, 4 internal deps, package.json, tauri.conf.json
# installer checksums                       # pass, SHA256SUMS.txt written for NSIS + two MSIs
# receive directory                         # pass, persists, creates the folder, refuses a file, and the runtime switches for the next transfer
# Android debug APK                         # pass, 272 MB, arm64-v8a, 16 KB LOAD alignment, debug certificate
# Android target check                      # pass, cargo check -p fluxy --target aarch64-linux-android
# mobile plugin                             # pass, builds for Android, classes in classes6.dex
# identity across restart                   # pass, Android 37 emulator: device_id stable over two cold starts, plain file removed
# Android descriptor source                 # pass, 4 tests incl. reopen-and-resume; manager installed on the emulator (10 GiB run pending)
# Android receive directory                 # pass, emulator sets the external Download/Fluxy path from the plugin marker
# MediaStore export                         # compiles and is registered; not yet reachable from the WebView
# mobile document picker                    # pass, emulator: DocumentsUI opens, cancel resolves as cancelled
# Android listener                          # pass, /proc/net/udp shows the app on 0.0.0.0:44109
# mobile send page                          # passes: renders as Android home, picker opens, port and insets correct
# status bar colour                         # pass, light and dark both match the page background, icons follow
# mobile network facts                      # pass, gateway=10.0.2.2 reaches the fallback probe (traced)
# NsdManager discovery                      # pass on a Titan_2 (Android 16): finds the desktop and offers 配对
# manual interface validation              # pass, a missing interface or address reports 2003, loopback needs the development switch, and a link-local pick keeps its scope
# socket interface pin                     # pass, the platform option is accepted for a real adapter on this host (network byte order on Windows)
# network change rebind                    # pass, automatic mode follows a changed default route, manual mode reports the loss and rebinds when the address returns
# rebind pauses transfers                  # pass, only moving transfers pause, and a second rebind finds nothing to pause
bun run test:e2e                        # pass, 5 local steps: pair, restart/resume chat ACK, clipboard consent, untrusted reject, loopback guard
scripts/acceptance/run-acceptance.ps1 matrix   # pass, T-01/T-02/T-03/T-04/T-06 automated, T-05 manual
scripts/acceptance/run-acceptance.ps1 bench --size 10GiB --release  # pass, 10.00 GiB in 72.5s, receiver peak RSS 51 MiB
cargo test -p udp-chaos --test transfer_faults  # pass, 4 fault tests (T-02/T-03/T-04/T-06), 1 ignored 10 GiB bench
FLUXY_BENCH_FILE=<10GiB> cargo test -p udp-chaos --test transfer_faults a_ten_gib -- --ignored  # pass, root hashes match
# e2e cross-host rehearsal (serve+join)   # pass, SAS, chat ACK, and clipboard offer through the machine-readable serve/join modes
# live desktop diagnostics               # pass, mDNS + fallback active; gateway, directed/limited targets, probe counter visible
# two-process loopback pairing          # pass, matching SAS, exit 0
# CLI pair, disconnect, then --resume    # pass, no second SAS, ACK on both chat messages
# mDNS loopback discovery                # pass, first peer within 3s
# CLI pair then chat, different ports     # pass, ACK delivered, no second SAS
# CLI discover local advertised server    # pass, peer found within 3s
# CLI discover (no peer)                  # pass, exits cleanly on timeout
# udp-chaos outage + recovery             # pass, QUIC retransmits after 250ms outage
# udp-chaos delay/duplicate/reorder       # pass, ACK, fault counters asserted
# CLI pair/chat through udp-chaos         # pass, ACK through delayed proxy
# security review                         # pass, 6 decisions recorded; R-04 mitigated in M1-02
# specta bindings                         # pass, src/bindings.ts generated
# desktop adapter tests                    # pass, 7 seq/snapshot/lifecycle/address tests
# desktop event channel                    # pass, live UI heartbeat seq 5 -> 6, snapshot v1
# desktop UI shell under CSP               # pass, dev + production hash routes render, no CSP console errors
# theme and locale switch                  # pass, data-theme/class and document.lang update
# rotation dialog                          # pass, focus trap, Escape close, focus restore
# responsive layout (760px)                # pass, rail hidden, mobile nav visible, no horizontal overflow
# mDNS fingerprint hint                    # pass, app advertises fp; CLI JSON prints it; no field creates trust
# two-direction app/CLI pairing            # pass, SAS matched on both screens; app initiator and responder paths both saved trust
# trust removal                            # pass, settings dialog removes pin and refreshes snapshot
# identity rotation + network restart      # pass, new device id/namespace; QUIC listener and mDNS advertisement restarted
# in-memory chat + delivery state           # pass, app -> CLI delivered, CLI -> app received
# failed send + retry                      # pass, disconnect marked Sending messages failed; retry reused message id and delivered once
# chat layout                               # pass, 1080px two-pane full width; 390px stacked with no horizontal overflow
# chat transcript bound                     # pass, 1000-message cap and duplicate receive suppression
# clipboard offer/accept/write              # pass, CLI -> app offer shown, explicit accept, digest verified, text written to OS clipboard
# clipboard decline                         # pass by state test: no text sent, pending removed, no clipboard write; 120s expiry is refreshed by the UI
# clipboard payload bounds                  # pass, 1 MiB text cap enforced by adapter/actor; preview truncated to 160 chars
# clipboard layout                          # pass, 1080px full width; 390px stacked with no horizontal overflow
# tray creation + restore                   # pass, app startup logs tray icon; minimizing then clicking the tray icon restores the window
# tray labels                              # pass, shell_set_labels accepts zh-CN and en; frontend locale changes log a label update
# background notification                  # pass, minimized window logs chat/clipboard_offer/clipboard_applied dispatch; a truly foreground window suppresses it
# foreground edge case                     # pass, Win32 foreground fallback handles a stale Tauri focus flag; minimized state always notifies
# notification privacy                     # pass by source/unit test, title/body contain only peer name + event type, no message text
# Windows toast delivery                   # installed build required by the plugin; dev builds may show PowerShell branding
~~~

Desktop bindings:

~~~text
$env:FLUXY_EXPORT_BINDINGS='1'; cargo run -p fluxy   # writes src/bindings.ts and exits
~~~

CLI smoke test:

~~~text
cargo run -p fluxy-cli -- status --data-dir ./.fluxy-dev
cargo run -p fluxy-cli -- id --data-dir ./.fluxy-dev
cargo run -p fluxy-cli -- config --data-dir ./.fluxy-dev
cargo run -p fluxy-cli -- discover --timeout 3
cargo run -p fluxy-cli -- serve --listen 127.0.0.1:0 --dev-loopback --data-dir ./.fluxy-a
cargo run -p fluxy-cli -- pair 127.0.0.1:PORT --fingerprint HEX --dev-loopback --data-dir ./.fluxy-b
cargo run -p fluxy-cli -- chat 127.0.0.1:PORT --device SERVER_UUID --message hello --dev-loopback --data-dir ./.fluxy-b
cargo run -p fluxy-cli -- connect 127.0.0.1:PORT --resume SERVER_UUID --message again --dev-loopback --data-dir ./.fluxy-b
~~~

`serve` and the first `pair` require the same six-digit SAS to be confirmed on
both sides by typing `yes`; a rejection leaves both trust stores empty. `chat`
and legacy `connect --resume` reuse the stored public key and certificate pin
and do not prompt for SAS. Chat text is sent over the control stream and
acknowledged before the client exits.

### Waiting on hardware

Three items cannot be closed in this environment and are listed here so they are not
mistaken for done:

| Item | What it needs |
| --- | --- |
| Tablet touch scrolling | A physical tablet. The structural evidence is positive at 1024x768, 800x1280 and 574x576 (see the verification rows above), and a real drag worked on the 1:1 phone, but the device that reported the problem has not been available since the fix. |
| T-M03 (SAF revocation) | Revoking the URI grant *before* the file is opened. Revoking mid-transfer cannot reach the bar: the sender already holds a descriptor and the process keeps reading through it. |
| T-M15 (VPN/cellular) | A SIM with data, or a configured VPN. This phone reports 
`mDataRegState=OUT_OF_SERVICE`
 and has no 
`tun`
 device. |

## Not implemented yet

- Persisted pairing lockout state across process restarts
- Automatic retry of unacknowledged chat across a new session (manual retry is implemented; automatic retry is not)
- Manual desktop checks that need a real GUI session: the tray restore and close-to-tray behaviour, a login start, Windows toast delivery in an installed build, and the two-machine half of T-11 (moving a cable or joining another network while a transfer runs)
- Sending a folder that contains no files at all (a root of empty directories); an empty directory inside a batch is created, but a purely empty folder has no sub-transfer to announce it
- A fixed transfer port; the default `listen_port: 0` still lets the system assign one, so the firewall guidance allows the program instead of a single port
- Installer-bundled firewall rules; `docs/FIREWALL.md` documents the rules and the help page copies them, but the bundle does not add them yet
- Download-directory selection in settings; the receiver root is still `<data_dir>/received`
- OS keystore-backed identity/TLS keys and explicit Windows key ACLs
- Global connection admission and per-session message rate limits
- Android SAF/NSD/FGS/Keystore work. The M4 track (docs/MOBILE_PLAN.md) cannot start in this environment: the Rust Android targets are installed, but there is no JDK, Android SDK/NDK, adb, or Gradle, so A-01 (build, sign, 16 KB page check) has nothing to build with

M0-06 mDNS/DNS-SD discovery and the peer table are complete. M0-07 adds the
`discover`, `pair`, and `chat` CLI commands, non-loopback mDNS advertising, and
loopback command isolation. M0-08 adds the `udp-chaos` proxy, deterministic
fault counters, and two-node loss/delay/duplicate/reorder recovery tests. M0-09
adds the threat model and security review record, including explicit gates for
keystore-backed keys, connection/message limits, dependency audit, and file path
defenses. M1-01 adds the tauri-specta command contract, a single Channel event
stream with seq/snapshot version, Engine lifecycle/heartbeat events, and a live Solid
adapter panel. M1-02 replaces that adapter-only view with the routed desktop
shell, design tokens, shared UI wrappers, theme/locale providers, and the
WebView hardening gate. M1-03 adds fingerprint-hinted discovery, the nearby
device list, the explicit two-screen SAS dialog, trust management, discovery
diagnostics, and a restartable LAN runtime. M1-04 adds the memory-only chat
runtime, conversation list, message stream, delivery/failure state, and retry.
M1-05 adds the desktop clipboard bridge, offer/consent/text/ACK exchange,
1 MiB/digest checks, the clipboard route, and the CLI clipboard command. M1-06
adds persisted per-device ask/auto/reject policy, explicit auto-receive
approval, origin/hop markers, digest deduplication, and echo suppression. M1-07
adds native notifications for unfocused windows and a localized tray menu that
restores the window. M1-08 adds Vitest/jsdom component tests with a typed mock
engine and fixtures. M1-10 adds UDP gateway probes, directed/limited broadcast,
nonce-checked replies, mDNS channel precedence, and fallback diagnostics. M1-09
adds the cross-platform end-to-end acceptance driver, the Linux/macOS and
Windows wrappers, the `serve`/`join` LAN modes, and the CI matrix that runs the
five-step local scenario on all three desktop platforms. M1 is complete. M2-01
adds the cross-platform file access abstraction and the desktop `std::fs`
implementation, with read/write boundary tests. M2-02 adds the manifest model,
parallel BLAKE3 pre-hash, progress reporting, and the identity-bound manifest
cache, with a real 10 GiB benchmark under the 15 s budget. M2-03 adds the
receiver-owned pull scheduler, the `FCH1` chunk codec, the manifest/chunk
request/ack frames, and the four-stream loopback sender and receiver, verified
by repeated transfers on one connection. M2-04 adds the atomic checkpoint
store, the resumable scheduler and sink reopen path, and loopback tests where
either side crashes and only missing chunks are retransferred. M2-05 adds the
enforced lifecycle state machine, the pause/cancel control handle, the 3006 and
3008 error codes, and the cancel cleanup path that discards the partial file.
M2-06 adds the streaming BLAKE3 root-hash gate before `completed`, per-block
rejection with immediate re-request, the sender-side live stat check (3002),
and the 3005 code. M2-07 adds the transfer negotiation frames, the application
transfer runtime with the accept/reject handshake, a 100 ms progress throttle
and a speed meter, the Tauri transfer commands and the `transfer` event
category, and the `/transfers` transfer center with the incoming-offer prompt
and the file-picker send flow. M2-08 adds window drag and drop, multi-file
batches, the 2002 `peer_busy` admission gate, and the persisted global
active-transfer cap with a FIFO queue. M2-09 adds the UI-free single-file
transfer driver in `fluxy-app` that the CLI and the fault tests share, the
`transfer-send`/`transfer-recv` CLI modes, the T-02/T-03/T-04/T-06 failure
injection tests in `tools/udp-chaos`, the ignored 10 GiB T-01 benchmark, the
`scripts/acceptance/` matrix driver with receiver RSS sampling, and stream
priorities that keep chat responsive under a saturated transfer, and a pull
window that refills on chunk completion instead of on the 100 ms tick. A 10 GiB
release transfer completes with matching root hashes and a 51 MiB receiver peak
RSS, but the measured ~150 MiB/s loopback throughput is still below the 1 GiB/s
target in section 11.3; the disk (3.4 GiB/s copy) and the flow-control windows
(8 MiB stream, 32 MiB connection) are ruled out, and the UDP send path on
Windows is the remaining suspect. M2 is complete. M3-01 completes the settings surface and the accessibility baseline. The
settings page now edits the device name and the desktop-notification preference:
a rename is sanitized and persisted by the engine, refused while a pairing is
pending, and restarts the LAN runtime so the mDNS record carries the new name;
the notification preference is applied to the notifier at startup and before
every dispatch. Notifications remain opt-out and still contain only the peer
name and the event type. Interface copy stays in `src/i18n`, and a new catalog
test fails the build when the two locales drift, when a placeholder disappears
from a translation, when a key written in the source is missing from either
locale, when a state key built from a bindings union is absent, when a component
hardcodes CJK copy, or when a key is no longer referenced; 31 keys that no
component used were removed. Keyboard access was repaired where the flow was
broken: a dialog now restores focus when a parent unmounts it instead of closing
it, which is how the pairing dialog disappears when a challenge is withdrawn,
and the nearby-device pair button keeps focus through `aria-disabled` instead of
`disabled` so the dialog can hand focus back to it. Tests drive the main flow
with keyboard events only, and the dialog focus trap, Escape handling, and focus
restore have their own suite. M3-01 is complete. M3-02 finishes the transfer surface. Every task now has a detail dialog
carrying the whole record - state, direction, peer, progress with speed and ETA,
the creation and update clock - and a failed task adds its reason in the
selected language, the protocol code and number, whether the core considers the
failure repeatable, and the raw message. The reasons come from the stable error
codes rather than from prose stored in Rust, and a code this build does not know
degrades to a neutral message; the catalog guard reads `ErrorCode::as_str` from
`crates/fluxy-core/src/error.rs` and fails the build when a code has no copy, so
the two lists cannot drift. Retry is offered only where the core says the
failure can be repeated - a changed source or a hash mismatch asks for a new
send instead - and it runs straight from the dialog, which satisfies the
one-click retry criterion. Desktop notifications now cover transfer outcomes as
well: a completed or failed transfer raises one naming the file and the peer
while the window is unfocused, a user cancel stays silent, and a per-device
two-second window keeps a bulk send that finishes at once to a single
notification. The M3-01 notification preference mutes all of them. M3-02 is
complete. M3-03 makes the automatic clipboard real. The switch that used to be
display-only now persists, and while it is on a 300 ms poll reads the system
clipboard on the same dedicated thread the manual path uses. A value is only
offered once it has looked the same for a full debounce window, so a
half-written clipboard or a burst of copies produces one offer, and the watcher
keeps a "already sent" digest so the same content never goes out twice. The
loop rule is the interesting part: a digest that the local clipboard *applied*
from a peer is never offered back, so B cannot echo A's text to A even though B
now has it on its clipboard. Together with the `hop` cap (a receiver never
forwards) and the `origin_id` echo check on the receive side, an A -> B -> A
loop is impossible while both devices watch, which is the M3-03 acceptance
criterion. Offers go to every online trusted device - resolved through a live
session first and discovery second, so offline devices are skipped instead of
queued - and each receiver still applies its own per-device receive policy. The
global send switch and the per-device receive policy stay separate switches, as
the plan requires. The clipboard page now shows a real switch, the number of
online targets, and the loop guarantee, and the whole feature is off by default. Turning the switch
on records the current clipboard value rather than sending it, so enabling never
broadcasts something the user copied before making that decision; only what is
copied afterwards goes out. M3-03 is complete. M3-04 makes the shell survive its
own window. Closing the window no longer ends the process: the close request is
prevented and the window is hidden, so chat, clipboard offers, and transfers keep
arriving in the tray, and only Quit sets the deliberate-quit flag that lets the
close through. Because a login start should not throw a window at the user, the
window is created hidden and shown from setup unless the process was started with
`--hidden` - the same flag the login item passes. The tray menu became the
always-available surface for the app: besides open and quit it now carries the
automatic-clipboard switch and the login item as check items whose state is
seeded from the persisted preferences and reverted if the platform refuses the
write, plus a Help entry. The login item is dependency-free - one
`StartupBackend` trait with a Windows backend that writes the per-user Run value
through `reg.exe` and file-based macOS and Linux backends - so the switch is
unit-tested through an injected runner instead of touching the machine's real
login items, and a preference that survived a cleaner's removal is repaired on
the next start. The help entry needed a new event category, because native chrome
cannot drive the router: the tray raises `AppEvent::Shell`, the shell consumes it
and navigates to a new `/help` page showing the live ports, copyable
platform-specific firewall commands built from those ports, a troubleshooting
list, and the diagnostics payload, with `docs/FIREWALL.md` as the standalone
version of the same guidance. M3-04 is complete.

M2-11 makes "which interface" a real setting. A laptop usually has Ethernet,
Wi-Fi, a VPN, and a hypervisor adapter up at the same time, and advertising the
wrong one produces the classic "the device is visible but the connection never
works" report. The picker is pure and unit-tested: real adapters before virtual
ones, LAN-capable types before tunnels, the default-route interface before a
private address, and the platform index for a stable order. The platform type
comes from the OS rather than from a name guess, and the virtual-adapter list is
only a deprioritisation, so a host whose only path is a VPN still gets a listener
and the UI says why. Binding the socket to the chosen interface needed three
platform options; Windows turned out to want the interface index in network byte
order, and a host-order value is refused with WSAEADDRNOTAVAIL - which is why the
runtime treats a rejected pin as a warning and reports it instead of failing the
listener. The two-second poll closes the loop: automatic mode follows a cable
move, manual mode reports 2003 and waits for the address to come back rather than
downgrading silently, and a rebind pauses active transfers first so each one
resumes from its checkpoint. One documented behaviour change: `dev_allow_loopback`
now only permits loopback. Automatic selection never picks it (the plan's 3.5
rule), so the development loopback path is `--listen 127.0.0.1:0 --dev-loopback`
on the CLI or a manual selection of 127.0.0.1. M2-11 is complete.

M3-06 puts that selection in the settings page, which is where the plan's
"the device is visible but unreachable" risk is actually resolved: the user can
see which adapter Fluxy chose, why, and what the alternative would be. The panel
is deliberately explanatory rather than a bare dropdown - the recommended mark,
the default-route mark, and the virtual-adapter mark are the difference between
"pick the right one" and "guess". It also refuses to hide the two costs of a
switch: the confirmation says that sessions drop and that transfers pause and
resume from their checkpoint, and a failure keeps the proposed selection on screen
with the recovery steps instead of clearing the form. The unpinned case is stated
plainly, because a bound source address is not the same as a pinned route. M3-06 is complete.

M3-07 makes a folder a batch rather than a new protocol. Every file keeps its
own manifest, checkpoint, and transfer state, so the resume, pause, cancel, and
retry paths that already existed apply unchanged - which is exactly why ADR-16
chose a batch id that aggregates many single-file transfers over a multi-file
manifest. What is genuinely new is the path boundary, and it is strict on
purpose: the sender describes relative paths, the receiver validates each one,
and only then creates directories. Absolute paths, traversal, NUL, control and
bidirectional-override characters, alternate data streams, reserved device names,
trailing dots, empty segments, duplicates that differ only in case, a path that is
both a file and a directory, and anything that escapes the receive root are
refused with a 3003 rather than sanitised into something else. The batch is
described once: the first sub-transfer carries the entry list, later ones carry
only their relative path, so a 10k-file folder does not repeat itself 10k times,
and an offer whose size disagrees with the entry list is refused. The UI answers
the plan's "one batch row" requirement: collapsible, with batch-wide progress and
batch-wide pause, resume, and cancel. M3-07's automated half is complete; the two-machine T-13 matrix (a nested
10 GiB folder, 10k files, and a symbolically linked tree) is documented as the
manual row.

M3-05 was the regression pass, and it found two defects worth recording because
both were the kind that a passing suite hides. Dropping a folder on the window was
refused: the drop path only understood files, so the plan's "drag a file or folder"
stopped at files even though folder sending itself worked. That is now a scanned,
validated batch that opens its own confirmation. The second was arithmetic: the
per-drop file budget counted accepted files rather than considered paths, so a drop
made of unreadable paths could keep walking past the limit instead of stopping at
it. Neither was a crash, and neither was caught by a test - which is exactly what a
regression milestone is for. M3-05 is complete and the blocking-defect list is empty. M3 exit review and M5
are what remain on the desktop track.

M5-03 is the audit milestone, and its most useful output was a failure. The
first `cargo deny` run rejected the workspace itself: every internal dependency
was path-only, which the wildcard ban reads as an unversioned dependency that
could never be published. That is a real packaging defect, not a policy
formality, and it is now fixed in all five manifests. The second finding was the
honest shape of an offline audit: the RustSec database is a git clone, github.com
is unreachable from this environment, and the advisories check therefore cannot
run here. Rather than quiet the check, it is written down as a release gate with
the exact command, and the rest of the audit - licenses, sources, bans, the
unsafe inventory, and a deterministic fuzz campaign - does run and is
reproducible from one script. The fuzz harness is deliberately not libFuzzer: stable-only toolchains cannot
run it, so the harness trades coverage for reproducibility, and the mutation
generator is what makes that trade acceptable - mutating valid frames reaches
the arithmetic that pure garbage never does.

M5-04 turned the bundle from a placeholder into a release contract. The
interesting part is not that an installer exists; it is that the installer
metadata is now checked in next to the code (publisher, copyright, license file,
per-user install, both locales), so an installer string cannot drift away from
the project. The verification is a script rather than a screenshot: a silent
install into a temp directory must exit 0 and leave the executable and an
uninstaller, a silent uninstall must leave no directory, and an administrative
install of the MSI must extract the same binary the release build produced. Both
formats do. The gap is signature: the packages are unsigned, which the release
notes have to say out loud, and docs/PACKAGING.md carries the two Windows signing
paths and the macOS notarization variables so clearing it is a configuration
step rather than a research project.

M5-02 made the matrix honest. Running the Rust gates on Linux alone was enough
while everything was library code, but the release ships two platform-specific
installers and three platform-specific tray, login-item, and notification
implementations, so "green" has to mean green per platform. The workflow now
runs the same commands on all three, and the one non-obvious dependency is
ordering: tauri-build embeds the frontend, so a clean checkout cannot compile
the shell until the frontend is built. docs/RELEASE_MATRIX.md states each row's
status plainly, including the rows no single host can prove - two-machine
transfers, a clean-machine install, tray behaviour - instead of implying that a
green CI badge covers them.

M5-05 wrote the documentation the release actually needs, and the useful
constraint was audience: one document that explains pairing, clipboard consent,
and folder batches to a new user reads nothing like the checklist a release
manager ticks off. Keeping them separate also keeps them honest - the known-issues
list inherits the gap list rather than a polished summary, and the release notes
repeat the unsigned-installer warning instead of burying it. The one piece of
automation worth adding was the cheapest: a link checker, because a fifteen-document
index rots the moment a file is renamed.

M5-06 turned the release into a scripted rehearsal, and the rehearsal found
things. The first soak run failed at round six with a transport timeout, which
became two CLI defects: the long-running server never accepted an offer (it only
printed it), and a session could outlive its peer because the accept loop waits
for the current session to end while a scripted client exits before its QUIC close
frame leaves. Both are now fixed and the soak went from occasional 21-second
rounds to a steady 0.3-0.6 seconds with zero failures and a flat 12-13 MB resident
set. That is the value of a soak that a unit test cannot provide: every part was
tested in isolation and the composition was not. Version agreement and installer
checksums are scripted too, so the last manual acts are the 24-hour run, the
clean-machine install, and the tag.

## Resolved: the x86_64 emulator build

The x86_64 APK once died in `WryActivity.onCreate` with

~~~text
UnsatisfiedLinkError: cannot locate symbol
"ring_core_0_17_14__aes_gcm_dec_update_vaes_avx2"
~~~

Root cause: a hand-run `cargo check --target x86_64-linux-android` (with
`CC`/`AR`/`LINKER` set by hand) had cached ring's build script output in the
same target directory. The cached output was 20 lines with **no** `rustc-link` directive,
so ring's C and assembly objects were never linked; the linker allowed the
undefined symbol because `-z defs` is not set.

Fix: delete `target/x86_64-linux-android` entirely and build only through
`bun x tauri android build`. The rebuilt `ring` build script writes 168 lines with
three link directives (the same shape the aarch64 target always had), and the APK
starts, listens on `0.0.0.0:40972`, and opens the document picker.

Rule for next time: never point a hand-set `cargo check --target` at an Android target
directory that a Tauri build also uses; the two produce incompatible cached
build-script state.
