# Testing

Fluxy uses the layers defined in `docs/DEVELOPMENT_PLAN.md` section 11.
Every pull request runs the unit, protocol, integration, and frontend checks;
fault injection runs daily and before a release.

## Standard commands

~~~text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
bun run test:frontend
bun run check
bun run check:security
bun run build
~~~

The transport integration tests use real UDP loopback sockets and the same
QUIC/TLS/SAS path as production. The UDP chaos proxy adds a controllable lossy
link between two real endpoints.

## Desktop shell and adapter

The Tauri adapter and the routed UI shell have separate checks:

~~~text
cargo test -p fluxy
$env:FLUXY_EXPORT_BINDINGS='1'; cargo run -p fluxy
bun run check
bun run check:security
bun run build
~~~

The first command runs the event sequence, snapshot, rotation, lifecycle,
nearby-address, chat transcript/state, and clipboard offer/consent/apply tests.
The Vitest suite is described under **Frontend component tests** below.
On Windows, test binaries embed the Common Controls v6 manifest from
`src-tauri/test-resource.rc`; without it the tray/menu linkage fails at load.
The second command regenerates `src/bindings.ts`; it must be committed
together with any command/type change. `bun run check` is the TypeScript contract test.
`bun run check:security` verifies the production/dev CSP, the
`core:default`-only capability, the absence of `opener`, and the absence of
HTML/eval/URL sinks in `src`.

## Frontend component tests

The component suite uses Vitest, jsdom, and `@solidjs/testing-library`. It does
not start Tauri: `src/test/mock-engine.tsx` supplies a typed `EngineContext`
from `src/state/engine.tsx`, `src/test/fixtures.ts` builds snapshots/messages/
shares, and `src/test/render.tsx` wraps a lazily-created component in the i18n
and mock-engine providers. `src/test/setup.ts` installs the dialog, matchMedia,
ResizeObserver, scroll, and `offsetParent` shims and cleans the DOM after each
test.

~~~text
bun run test:frontend
bun run test:frontend:watch
bun run test                  # Rust workspace tests, then Vitest
~~~

The checked-in tests cover:

- `ChatWorkspace`: first-conversation selection, message rendering, trimmed
  send and composer reset, Enter vs Shift+Enter, send-failure recovery,
  failed-message retry, and the offline composer replacement.
- `PairingDialog`: SAS rendering, the acknowledgement gate, cancel, expiry,
  and disabled controls while a pairing request is in flight.
- `ClipboardPage`: manual share, per-device policy update, the auto-receive
  warning, explicit offer accept/reject, and the no-online-peer state.
- `NetworkDiagnostics`: fallback gateway/targets/counters and the fallback
  error state, including the preserved mDNS status.
- `TransfersPage`: progress/size/speed/ETA rendering, pause/resume/cancel/
  retry actions, the incoming-offer accept/reject dialog, the multi-file picker
  plus trusted-device send flow, the dropped-batch confirmation with rejected
  and skipped files, the drop overlay, the queued row, the concurrency control,
  the offline-device guard, the error-details dialog, and the state filter.
  `src/lib/format.test.ts` covers the byte, rate, percent, and ETA formatters
  used by the transfer rows.
- `LocalDeviceSettings`, `SettingsPage`, `Dialog`, `keyboard.test.tsx`, and
  `src/i18n/catalog.test.ts` cover the M3-01 surface; see
  **Settings, i18n, and keyboard accessibility (M3-01)** below.
- The transfer detail dialog, the failure reasons, and the transfer
  notifications are covered in
  **Transfer details and notifications (M3-02)** below.
- The automatic clipboard watcher, its debounce, and the loop rules are covered
  in **Automatic clipboard watching (M3-03)** below.

## Hotspot and gateway fallback

~~~text
cargo test -p fluxy-net fallback
cargo test -p fluxy network
~~~

The first command covers the bounded UDP packet format and a two-instance
loopback probe/response over real UDP sockets. The second covers channel
precedence: when mDNS and the fallback see the same device, the mDNS record
keeps its address and liveness.

### Phone hotspot as the host (T-M10)

This is the case where multicast typically does **not** survive, so it is the
one that proves the fallback rather than merely exercising it. It needs one
Android phone and one desktop; a second phone is not required.

1. **On the phone**, enable the system hotspot in Settings. Fluxy never toggles
   it. Find the hotspot interface and its address:

   ```powershell
   adb shell ip -4 addr show | Select-String 'inet |: ap'
   ```

   Expect an `ap0` (or `wlan1`/`swlan0`) carrying something like
   `10.15.131.189/24`. A `wlan0` address may still be present if the phone is
   also on another network; the `ap0` one is the hotspot.

2. **On the desktop**, join that hotspot. Confirm the gateway is the phone:

   ```powershell
   Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' }
   Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Select-Object NextHop, InterfaceAlias
   ```

   The desktop address sits in the same /24 and the default gateway equals the
   phone's `ap0` address.

3. **Restart the desktop app.** This matters: an instance started before the
   network change keeps its old bind and discovers nothing at all, and the log
   file for the day stays at 0 bytes. Close it and run
   `bun run dev -- --release` (see AGENTS.md: the dev server must already be
   up).

4. **Watch the fallback carry discovery.** If the hotspot filters multicast
   there will be no mDNS peer; the desktop log shows the probe result instead:

   ```text
   INFO UDP fallback discovered a peer peer=TITAN2 platform=android
   ```

   repeating on the probe interval. Reaching the peer only through this channel
   is the expected outcome, not a degraded one.

5. **Confirm the phone is listening on all interfaces.** It binds
   `0.0.0.0:42420`, so no per-interface binding is needed:

   ```powershell
   adb shell 'cat /proc/net/udp' | Select-String ':A5B4'
   ```

   `00000000:A5B4` is `0.0.0.0:42420`.

6. **Send a file and check the session address.** The chat session must show the
   hotspot address, not a leftover Wi-Fi one:

   ```text
   INFO chat session live ... remote=10.15.131.189:49784
   INFO transfer finished file=midtest.zip bytes=104857600 mib_per_s=43.23
   ```

   Measured on a phone hotspot: 43.23 MiB/s, against 44 MiB/s over Wi-Fi, so the
   hotspot is not a throughput bottleneck.

7. If the peer does not appear: confirm the desktop was restarted after joining,
   confirm the gateway is on the same subnet, and confirm inbound UDP 42420 is
   allowed. Then check for client isolation (**T-M14**), which needs router or
   hotspot admin and cannot be set from `adb`.

### Windows hotspot as the host

If the desktop hosts instead, it is the gateway and the phone is the client.
Allow inbound and outbound UDP 42420 in the host firewall; everything else in
the sequence above applies, and the diagnostics should show a directed
broadcast target such as `192.168.137.255:42420`.

### Invariants

Pairing still completes the normal TLS pin and SAS confirmation - the fallback is
a discovery hint and must never skip either step. When both channels work, mDNS
keeps its record and the fallback must not duplicate or override the peer.

## End-to-end acceptance (M1-09)

The M1 main path is scripted for Windows, Linux, and macOS:

~~~text
bun run test:e2e                       # works on Windows, Linux, and macOS
bash scripts/e2e/run-e2e.sh local      # a thin wrapper around the same driver
pwsh -File scripts/e2e/run-e2e.ps1 local # the Windows wrapper, same driver
~~~

The driver builds `fluxy-cli`, runs two isolated peers over loopback, and
checks first pairing with SAS, chat delivery plus application ACK, the headless
clipboard consent rejection, trust persistence across a server restart with a
resume that skips SAS, rejection of an unknown device id, and the loopback
guard. The CI `e2e` job runs this on Ubuntu, macOS, and Windows.

For a real LAN run, start `serve --yes` on one host, copy the printed
`connect`/`device`/`fingerprint` values, and run `join --addr ... --device ...
--fingerprint ... --yes` on the other. The scripts auto-confirm SAS, so use
them only on an isolated test network; see `../scripts/e2e/README.md` for the full
procedure and options. A physical Windows-to-Linux/macOS LAN run and the
phone-hotspot pairing remain manual acceptance gates.

Manual shell smoke test:

1. Run `bun run tauri:dev` and wait for the window.
2. The home view and the event-channel panel should show a heartbeat
   immediately and roughly every 10 seconds; `seq` increases by one. The
   status bar snapshot version changes only when state changes, not on every
   heartbeat.
3. Visit `#/transfers`, `#/clipboard`, and `#/settings`; each route must render
   its own title, empty state, and metadata without a blank frame.
4. Switch theme through the top bar and the Settings choices; the root must set
   `data-theme` and the `.dark` class, and the choice must survive a reload.
5. Switch to English; `document.documentElement.lang` must become `en`, the
   title must change, and no literal `home.*`/`settings.*` key should appear.
6. Open the identity-rotation dialog. Focus must enter the dialog, `Tab` must
   wrap inside it, `Escape` must close it, and focus must return to the trigger.
   Do not confirm rotation in a data directory you care about.
7. Resize the window to 760px wide: the rail hides, the bottom navigation
   appears, and the page must not scroll horizontally.
8. Leave the window open across a sleep/resume cycle. If an event gap or a
   snapshot-version change is observed, the UI calls `state_snapshot` and
   rebuilds the status/peer data.

Manual pairing smoke test (two desktop devices or one desktop plus a CLI peer
that advertises on the same LAN):

1. Open `#/settings` and confirm **Discovery diagnostics** reports the active
   private-LAN interface, a listener address, `advertised=true`, and
   `browsing=true`. A loopback address must not appear unless the explicit dev
   flag is set.
2. On the second device, open Fluxy. The first device must show it under
   **Nearby devices** within a few seconds. There must be no QR, manual-address,
   or one-tap trust entry anywhere in the UI.
3. Click **Pair**. On both devices, confirm the dialog shows the same six-digit
   code, both device names, a live countdown, and the explicit warning to
   compare both screens.
4. Before ticking the acknowledgement checkbox, the confirm button must stay
   disabled. Tick it, then confirm. A success toast and a trusted-device entry
   must appear only after both sides confirm. Reject or let the dialog expire
   once and confirm neither trust store gained an entry.
5. In Settings, remove the trusted device. The list count must drop and a new
   connection must require pairing again.
6. Rotate identity while idle. The device ID and mDNS namespace must change and
   discovery must return to `discovering` at a new listener port. Rotation while
   a pairing is active must fail with `pairing_busy`.
7. Repeat with the desktop as responder (peer initiates). The dialog text must
   describe the incoming request, and the same checkbox gate must apply.

Manual chat smoke test (desktop plus a trusted CLI peer or second desktop):

1. After pairing, open the home view. The conversation list must show the
   trusted peer with an online/connecting/offline state; the message pane must
   show the memory-only notice. There must be no message persistence or history
   restore after restarting the app.
2. Select the conversation and send a message. Its initial state is **Sending**;
   it must become **Delivered** only after the peer application ACK arrives.
3. Send from the peer to the desktop. The desktop must append the message once
   and show **Received**; a duplicate retry with the same message id must not
   append a second copy.
4. Stop the peer process while a session is open. Messages that were still
   **Sending** must become **Failed** and expose a **Retry** action; the
   composer must disable once the peer is offline.
5. Start the peer again, wait for the connection, and use **Retry**. The retried
   message must reuse its original id and become **Delivered** without a second
   transcript row.
6. Resize to 1080px and 390px. The two-pane layout must become stacked on the
   narrow view and the page must not scroll horizontally.
7. Rotate identity after a chat. The transcript must clear, and the old peer
   must require pairing again.

Manual clipboard smoke test (desktop plus a trusted CLI peer or second desktop):

1. Put a short text on the sender's system clipboard. Open `#/clipboard`;
   the send card must list only online trusted peers and must not read the
   clipboard until **Send current clipboard** is clicked.
2. Send. The sender history must show an offer with preview, size, and a
   countdown. The receiver must show an incoming offer with the same preview
   and size; its system clipboard must stay unchanged until **Accept**.
3. Accept. The receiver writes the text, verifies the digest, and shows
   **Applied**; the sender history must show **Applied**. Paste on the receiver
   to confirm the exact text and line breaks.
4. Reject a second offer. The sender must show **Rejected**, no text may be
   sent, and the receiver clipboard must remain unchanged.
5. Let an offer expire without action. It must disappear from the incoming
   queue and no clipboard write may occur.
6. Send 1 MiB of UTF-8 text and then more than 1 MiB. The first must transfer;
   the second must fail before sending with a visible error.
7. Run `fluxy-cli clipboard <address> --device <trusted-id> --text ...`
   against the desktop. The desktop must require acceptance before the text is
   sent, and the CLI must print `clipboard-applied` only after the final ACK.
   Headless `fluxy-cli serve` must reject offers with `cli_no_consent`.
8. On the receiver's policy panel, the trusted device must default to **Ask**.
   Send another offer: it must still appear in the incoming queue. Change the
   policy to **Auto**, restart the desktop, and confirm the selection persisted.
   Send again: the receiver must write without an incoming prompt, show
   **Applied**, and the sender must receive the applied ACK.
9. Change the same device to **Reject** and send once more. No prompt may
   appear, the receiver clipboard must stay unchanged, and the sender must show
   **Rejected** with `policy_reject` (or a visible declined state). Change the
   policy back to **Ask**.
10. With Auto/Ask in place, send the same text twice from the CLI/desktop.
    The second offer must be declined with `duplicate_content` and the receiver
    system clipboard must not be written again. Re-sharing text that was just
    received must fail locally with `clipboard_duplicate`.
11. Resize to 1080px and 390px. The peer choice, policy buttons, and send row
    must stack on the narrow view and the page must not scroll horizontally.

Manual notification and tray smoke test (desktop plus a trusted CLI peer):

1. Start the desktop and confirm the Fluxy tray icon is present. Minimize the
   window, then left-click the tray icon: the window must restore and focus.
   Right-click must show the localized **Open Fluxy** and **Quit** items.
2. Switch the UI to English and reopen the tray menu. The labels must update to
   **Open Fluxy** / **Quit** without restarting; switch back to Chinese and
   confirm they update again.
3. With the Fluxy window focused, send a chat from the CLI. No OS notification
   may appear, but the message must arrive in the transcript.
4. Minimize or focus another application and send another chat. A single OS
   notification must appear with only the peer name and **New message**; it
   must not contain the message text. Click it: the window must restore and
   focus. A notification shown while the window is focused is a failure.
5. With the device policy set to **Ask**, send a clipboard offer while the
   window is unfocused. A **Clipboard offer** notification must appear with
   only the peer name. With **Auto**, the receiver must notify only after the
   content is applied, and the notification must not contain clipboard text.
6. Check the application log at debug level for `desktop notification
   dispatched` and `tray icon created`; neither log line may contain a message
   body or clipboard text.

On Windows, the notification plugin only delivers native toasts for an installed
build; a `tauri:dev` binary may show PowerShell branding. Run the installed
build for step 4 if the development toast does not appear.

For a CSP console check, run the shell with WebView2 remote debugging:

~~~powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
bun run tauri:dev
~~~

Connect to `http://127.0.0.1:9222/json`, enable `Runtime` and `Log`, and
reload. There must be no CSP refusal or uncaught exception. The Vite HMR
websocket is allowed only by `devCsp`; the production bundle uses the stricter
`csp`.

The browser preview cannot reach Tauri IPC; it should show the explicit
desktop-runtime error state with retry and copy-diagnostics actions.

To verify the production policy rather than `devCsp`, build without an
installer and launch the binary:

~~~powershell
bun run tauri:build --debug --no-bundle
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
.\target\debug\fluxy.exe
~~~

The production window loads from `http://tauri.localhost/` (not the Vite
origin); the same route, theme, and dialog smoke checks must pass and the
`Log` domain must report no CSP refusal.

## File access boundaries (M2-01)

`crates/fluxy-core/src/files` is the only layer that touches platform paths.
The desktop manager registers a `FileReference::Path`, opens a `FileSource`
with verified random-read capability, and creates `FileSink` writers under the
receiver root. Run the focused tests with:

~~~text
cargo test -p fluxy-core files
~~~

Covered boundaries:

- `read_at`: zero length, exact range, tail clamp at EOF, empty read at
  `offset == size`, out-of-range rejection, and short-read source-change
  detection;
- `write_at`: zero-length writes, overflow and past-size rejection, out-of-order
  and overlapping ranges, and coverage merging;
- `finalize`/`abort`: incomplete coverage removes the temp and returns
  `Incomplete`, an existing destination is refused, abort removes the temp, and
  zero-size files finalize;
- manager: missing file, directory, URI/bookmark unsupported, unknown handle,
  invalid names, and export checks.

Path, space, and overwrite hardening beyond this slice is tracked as M2-10.

## Manifest and pre-hash (M2-02)

`crates/fluxy-core/src/transfer/manifest.rs` computes the per-file manifest with
a parallel BLAKE3 window pass. The focused tests are:

~~~text
cargo test -p fluxy-core manifest
~~~

Covered behavior:

- reference block hashes, root hash, and manifest digest for a known file;
- multi-window parallel hashing matches a single pass and reports monotonic
  block progress;
- empty files, invalid block sizes, and the 32768-block manifest limit;
- block and root verification reject wrong lengths and bytes;
- cache hit, source-change invalidation, identity mismatch, digest tamper, and
  atomic replacement of an existing entry.

The 10 GiB performance gate is ignored by default because it needs a large
local file. Run it in release mode with:

~~~text
FLUXY_BENCH_FILE=/path/to/10gib.bin cargo test -p fluxy-core --release --lib \
  benchmark_10gib_prehash_under_15s -- --ignored --nocapture
~~~

On this machine the gate measured 10 GiB in 7.23 s (1416 MiB/s). The same
command is the manual gate for the plan’s 15 s budget on other hardware.

## Chunk protocol and scheduler (M2-03)

The chunk codec and scheduler live in `crates/fluxy-core/src/transfer`
(`bitmap.rs`, `chunk.rs`, `scheduler.rs`); the QUIC sender and receiver live in
`crates/fluxy-net/src/transfer.rs`. Focused commands:

~~~text
cargo test -p fluxy-core transfer::bitmap
cargo test -p fluxy-core transfer::chunk
cargo test -p fluxy-core transfer::scheduler
cargo test -p fluxy-net transfer
~~~

Covered behavior:

- bitmap set/clear/count/missing, byte round-trip, and truncated or mismatched
  serialized input;
- `FCH1` frame round-trip, bad magic, truncation, length mismatch, payload-hash
  tamper, manifest-hash mismatch, out-of-range index, and wrong offset;
- window fill without duplicates, next batch after receipts, timeout retry
  accounting, retry-budget exhaustion into the failed set, duplicate/unknown
  receipts, empty-file progress, and stable transfer error codes;
- manifest protobuf round-trip plus rejection of a tampered block hash, a bad
  transfer id, and a short block hash, and the in-band manifest exchange that
  must match the receiver’s manifest before any chunk is written;
- a 5 MiB file over four default streams with 1 MiB blocks, verified against the
  manifest root hash and the written file;
- a 3 MiB out-of-order/duplicate run with a 2-stream window, and three repeated
  8 MiB transfers over one connection to show the four-stream path is stable;
- a receiver that gets no chunks and fails with `MissingChunks` after its retry
  budget instead of hanging.

The loopback tests use pinned TLS endpoints and a raw QUIC connection, so they
do not need pairing or SAS. They complete in well under a second locally.

## Checkpoints and resume (M2-04)

Checkpoint state lives in `crates/fluxy-core/src/transfer/checkpoint.rs`, the
resumable sink in `crates/fluxy-core/src/files/desktop.rs`, and the resumable
pull in `crates/fluxy-net/src/transfer.rs`. Focused commands:

~~~text
cargo test -p fluxy-core checkpoint
cargo test -p fluxy-core manifest_bytes
cargo test -p fluxy-core resume_sink
cargo test -p fluxy-net transfer::tests::sender_crash
cargo test -p fluxy-net transfer::tests::receiver_crash
cargo test -p fluxy-net transfer::tests::receiver_task_abort
~~~

Covered behavior:

- the `FMF1` manifest frame round-trips and rejects bad magic, version, digest,
  count, and length;
- the `FCP1` progress frame round-trips and rejects truncated, tampered,
  bad-magic, and bad-version input;
- `CheckpointStore` writes both files atomically, replaces an existing entry,
  lists and removes transfers, and reports the missing blocks and bytes for a
  resume;
- a manifest mismatch or a corrupt progress file degrades `resume_plan` to a
  fresh transfer instead of stitching two content versions, and a leftover
  atomic-write temporary is ignored on load;
- `resume_sink` reopens the deterministic partial file, seeds its coverage,
  extends a short file, and rejects missing partials or out-of-range coverage;
- a sender crash mid-transfer leaves an `interrupted` checkpoint, and the
  resumed round transfers exactly the missing bytes and produces a file whose
  BLAKE3 root hash matches;
- a receiver sink failure after four chunks leaves a four-block checkpoint, and
  the resumed round completes with only the missing bytes;
- aborting the receiver task mid-transfer leaves the last persisted
  `transferring` checkpoint, and a fresh store and file manager resume from disk
  alone.

The loopback crash tests use the pinned-TLS endpoints directly and finish in
about a second. A real `kill -9` acceptance run on two hosts is part of the
M2-09 fault matrix.

## Pause, cancel, and retry (M2-05)

The lifecycle state machine lives in
`crates/fluxy-core/src/transfer/state.rs`, the pause/cancel handle in
`crates/fluxy-core/src/transfer/control.rs`, and the loop integration in
`crates/fluxy-net/src/transfer.rs`. Focused commands:

~~~text
cargo test -p fluxy-core state
cargo test -p fluxy-core control
cargo test -p fluxy-core scheduler::tests::pause
cargo test -p fluxy-net transfer::tests::pause_then_resume
cargo test -p fluxy-net transfer::tests::cancel_from_paused
cargo test -p fluxy-net transfer::tests::sender_cancel
~~~

Covered behavior:

- the full happy path Preparing → Transferring → Paused → Transferring →
  Interrupted → Resuming → Transferring → Verifying → Completed;
- illegal transitions are rejected (for example Transferring → Completed or
  Paused → Verifying) and a completed transfer is terminal;
- every active state can be cancelled, cancel is idempotent, and retry works
  from failed, interrupted, paused, and cancelled;
- the control handle is reversible for pause/resume, cancel wins over pause,
  repeated clicks are no-ops, and clones share one state;
- `MissingChunks` maps to 3006 and `Canceled` to 3008, and the error payload
  serializes with those numbers;
- `discard` removes the partial file in place, and `PullScheduler::pause`
  requeues the window without burning the retry budget;
- a paused loopback transfer stops pulling (progress freezes), keeps its
  `.part` file and `paused` checkpoint, and completes after resume;
- cancelling from the paused state returns `Canceled` (3008), discards the
  partial file, and removes the checkpoint;
- cancelling the sender makes `send_file` return `Canceled` and stops the
  receiver.

The pause/cancel latency bound is one `TRANSFER_TICK` (100 ms), which the
loopback tests rely on by settling for 120 ms before asserting that progress
froze.

## Integrity, finalize, and source change (M2-06)

Root-hash verification lives in `crates/fluxy-core/src/files/` (streaming
BLAKE3, live re-stat, sink root hash) and `crates/fluxy-net/src/transfer.rs`
(per-block rejection and retry, sender stat check, final gate before
`completed`). Focused commands:

~~~text
cargo test -p fluxy-core transfer::chunk::tests::rejects_hash
cargo test -p fluxy-core files::desktop::tests::root_hash
cargo test -p fluxy-core files::desktop::tests::stat_detects
cargo test -p fluxy-core scheduler::tests::a_rejected_chunk
cargo test -p fluxy-net transfer::tests::a_tampered_chunk
cargo test -p fluxy-net transfer::tests::every_chunk_tampered
cargo test -p fluxy-net transfer::tests::a_source_changed
cargo test -p fluxy-net transfer::tests::a_root_hash_mismatch
~~~

Covered behavior:

- `hash_reader` streams BLAKE3 in 1 MiB blocks and matches `blake3::hash`
  for a normal file and for the empty file;
- `ChunkHeader::decode_header` still returns the block index when the payload
  hash does not match, so a tampered block can be named and retried;
- `FileSource::stat` detects a size or modification-time change and
  `verify_unchanged` reports 3002;
- a rejected chunk expires immediately, is re-requested on the next tick, and
  still spends its retry budget (failing with 3006 when exhausted);
- a dishonest sender that corrupts the first delivery of every block still
  yields a byte-identical file because the receiver rejects and retries;
- a sender that corrupts every delivery fails the receiver with
  `MissingChunks` (3006) and leaves the `.part` file for a retry;
- changing the source file mid-transfer makes `send_file` return
  `SourceChanged` (3002) and the receiver observes the stop;
- a sink that reports the wrong root hash is rejected with `HashMismatch`
  (3005) before finalize, and the checkpoint and partial file are removed.

The final hash pass reads the whole partial file once, so the extra cost is one
sequential read on top of the receive; the 10 GiB pre-hash benchmark covers the
sender-side pre-hash, not this pass.

## Transfer center and negotiation (M2-07)

The application transfer runtime lives in `src-tauri/src/transfer.rs`, the
transfer negotiation frames in `crates/fluxy-net/src/session.rs`, and the
transfer center in `src/pages/TransfersPage.tsx`. Focused commands:

~~~text
cargo test -p fluxy transfer::tests
cargo test -p fluxy-net transfer::tests
bun run test:frontend
~~~

Covered behavior:

- the progress throttle emits at most once per 100 ms per transfer with injected
  timestamps, and hub-level progress events are coalesced the same way;
- the speed meter converges on the observed rate instead of the latest burst;
- `register_file` returns an opaque `{file_id, name, size}` handle, rejects a
  directory or a missing path, and keeps the filesystem path out of WebView
  state;
- a summary carries the failure code, numeric code, message, and retryable flag;
- control rejects an unknown transfer and a retry of a completed transfer, and
  pause/resume flip the local `TransferControl` state;
- progress updates the record summary and the protocol code-name table matches
  the wire table;
- the transfer center renders per-task progress, size, speed, and ETA, applies
  pause/resume/cancel/retry through the engine store, prompts for an incoming
  offer before anything is written, sends a picked file to a chosen trusted
  device, blocks an offline device, shows the error code and message, and
  filters the list by state.

The throttle is enforced in the backend, so the component tests assert rendered
text rather than event timing; the throttle itself is covered by the Rust tests
with synthetic instants.

## Drag and drop, batches, and the transfer cap (M2-08)

The window drop handler lives in `src-tauri/src/lib.rs`, the queue and batch
runtime in `src-tauri/src/transfer.rs`, and the transfer center in
`src/pages/TransfersPage.tsx`. Focused commands:

~~~text
cargo test -p fluxy transfer::tests
cargo test -p fluxy-app
bun run test:frontend
~~~

Covered behavior:

- the active cap comes from the persisted config and is clamped to 1..8, and a
  config file without the field still loads with the default;
- `active_count`/`offered_count`/`occupied` count only records that hold a
  slot, so a queued or terminal transfer never blocks admission;
- `claim_next_queued` returns the oldest queued send, ignores receiving
  records, and flips the claimed record to `preparing` so two pumps cannot
  race; a ten-deep queue with a cap of two starts two and holds eight;
- an empty batch and a batch above the 100-file limit are rejected before any
  transfer exists;
- `register_paths` keeps regular files, reports a directory by base name, and
  counts the paths beyond the batch limit as skipped;
- a ten-file drop registers ten handles and publishes one `files_dropped`
  event; `drag_hover` is published once per transition, never per mouse move;
- an unanswered offer expires as a non-retryable 2001 failure and hands its slot
  to the next queued send, and expiring it twice is a no-op;
- the transfer center shows the picked and dropped batches with totals, renders
  the drop overlay while a drag is active, renders a queued row with cancel but
  without pause, filters queued rows into the active tab, and persists a new
  concurrency choice.

The drop path is covered through `register_paths` and `accept_drop`, not
through a real OS drag: a physical drag, the ten-file queue on a live peer, and
the peer-busy rejection are manual desktop checks, and a real folder batch stays
with M3-07.

## Failure injection, the acceptance matrix, and the 10 GiB benchmark (M2-09)

M2-09 turns the acceptance matrix from
[docs/DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) section 11.2 into runnable
checks. Two layers do the work:

- `crates/fluxy-app/src/headless.rs` is a UI-free single-file transfer driver
  that the CLI and the fault tests share, so a test exercises the same
  offer/accept/serve/pull path the desktop app uses;
- `scripts/acceptance/` drives the real CLI as processes, which is what lets it
  cut a link or kill a receiver mid-transfer.

Focused commands:

~~~text
cargo test -p fluxy-app
cargo test -p udp-chaos --test transfer_faults
FLUXY_BENCH_FILE=<10GiB file> cargo test -p udp-chaos --test transfer_faults a_ten_gib -- --ignored --nocapture
pwsh -File scripts/acceptance/run-acceptance.ps1 matrix  # Windows-only: the acceptance matrix drives Windows paths
~~~

Covered behavior:

- T-01: the ignored `a_ten_gib_transfer_matches_its_root_hash` test moves a real
  10 GiB file over loopback, asserts both root hashes agree, and prints the
  pre-hash time, the transfer time and the throughput; the acceptance `bench`
  scenario adds the receiver memory ceiling (peak RSS at or below 200 MB);
- T-02: `a_short_outage_recovers_without_losing_blocks` cuts the chaos proxy
  mid-transfer and requires the run to finish with the same root hash, and the
  acceptance `outage` scenario cuts a real relay and retries with the same
  transfer id, requiring `resumed=true` and `missing_bytes_before` below the file
  size;
- T-03: `a_receiver_restart_resumes_from_its_checkpoint` fails the first run
  after the link is cut, restarts the driver on the same data directory, and
  asserts the resumed pull writes exactly the missing bytes; the acceptance
  `restart` scenario kills the receiver process outright and does the same;
- T-04: `a_changed_source_is_refused_with_3002` changes the source after the
  offer, so the sender fails with `source_changed`/3002 and no complete file
  appears on the receiver;
- T-04 also stands in for the plan's "corruption" script at the source level; a
  corrupted *block* is rejected with 3005 and re-requested, which the M2-06
  integrity tests in `cargo test -p fluxy-core transfer` cover directly;
- T-06: `chat_stays_responsive_while_the_link_is_saturated` sends 100 stamped
  messages during a transfer and requires every one to arrive with a p95 at or
  below 250 ms;
- T-05 (revoked access, 3003, and a kept partial) is a documented manual step in
  [scripts/acceptance/README.md](../scripts/acceptance/README.md), because it
  needs an ACL change and the desktop `WaitingForFileAccess` state is M4 work.

The benchmark found two performance issues. The receiver refilled its request
window only on the 100 ms tick, which capped throughput at one window per tick;
the window now refills as soon as a chunk lands (`refill_unless_paused` in
`fluxy-net::transfer`). Measured on the development machine (Windows 11,
Ryzen 7 8845H, NVMe, release build, loopback), a 10 GiB transfer completes with
matching root hashes at about 150 MiB/s and a 51 MiB receiver peak RSS. That is
below the 1 GiB/s loopback target in section 11.3, and the usual suspects are
ruled out: a plain 2 GiB disk copy runs at 3.4 GiB/s, the QUIC stream and
connection windows are 8 MiB and 32 MiB, and 1 MiB and 4 MiB blocks give the same
rate, so the limit is not disk, flow control, or per-chunk overhead. The next
step is to profile the UDP send path; Windows has neither GSO nor `sendmmsg` in
`quinn-udp`, so one datagram per syscall is the prime suspect. The debug build
reaches about 70 MiB/s, so always benchmark with `--release`.

The chat test found a real scheduling problem: the session control stream shared
the connection with bulk chunk data at the same priority, so chat frames queued
behind it and p95 reached 276 ms. `fluxy-net` now sets
`CONTROL_STREAM_PRIORITY` (100) on the session control stream, and chunk data
rides the transfer's own stream at the default priority, which brought p95 down
to about 140 ms. Setting `FLUXY_FAULT_TRACE=1` makes the test append stage
markers to `%TEMP%/fluxy-fault-trace.log`, which is how a hung run can be
inspected.

## Settings, i18n, and keyboard accessibility (M3-01)

M3-01 finishes the settings page and turns the keyboard path into a tested
contract:

~~~text
cargo test -p fluxy-app
cargo test -p fluxy
bun run test:frontend
~~~

`set_device_name` sanitizes the name (control characters removed, trimmed, at
most 64 characters, never empty), writes `config.json` atomically, bumps the
snapshot, and restarts the LAN runtime when the name really changed so the mDNS
record and the next handshake carry the new value; it and `set_notifications`
both return 4105 while a pairing challenge is pending. `set_notifications` also
flips the `Notifier` mute flag, which is applied from the persisted config
during startup. The read side stays on `state_snapshot`: `SettingsSnapshot`
gained `notifications` next to `device_name`, so the plan's
`settings_get`/`settings_set` pair is implemented as one snapshot plus typed
setters that validate at the boundary instead of a generic JSON blob.

The frontend suite covers the new surface:

- `LocalDeviceSettings`: the persisted name and toggle state, a trimmed save
  through the form submit, the empty-name refusal with `aria-invalid` and a
  `role="alert"` message, Escape reverting the field, a failed rename keeping
  the typed value, and the notifications toggle including its in-flight state.
- `SettingsPage`: every section the plan lists for `/settings`, the theme
  choice (storage key, `dark` class, `aria-pressed`), and the locale switch
  (storage key, translated heading, `document.lang`).
- `Dialog`: focus moves to the first control on open, Tab loops in both
  directions, Escape closes and reports the change, focus returns to the
  captured trigger, and focus also returns when a parent unmounts the dialog
  instead of closing it.
- `keyboard.test.tsx`: the main flow with keyboard events only - the skip link
  moving focus into `#main`, a nearby device paired from the rail (SAS
  acknowledged, confirmation enabled, confirmed), Tab trapped inside the pairing
  dialog with Escape cancelling it and focus back on the pair button, an
  incoming transfer offer accepted from its dialog, and the chat entry and
  composer on native controls. jsdom does not synthesise the click a browser
  fires for Enter or Space, so every step asserts both that the control is a
  real focusable element and that activating it completes the step.
- `src/i18n/catalog.test.ts`: the catalog guard. It reads every source file
  through `import.meta.glob(..., { query: "?raw" })` and fails when the two
  locales drift, when a placeholder disappears from a translation, when a key
  written in the source is missing from either locale, when a state key built
  from a bindings union has no copy, when a component outside `src/i18n`
  contains CJK copy, or when a catalog key is no longer referenced. That last
  check removed 31 unused keys; `transfers.state.*`, `clipboard.state.*`, and
  `clipboard.direction.*` are expanded from `src/bindings.ts` because the UI
  builds them with template strings.

`src/test/setup.ts` now reports `offsetParent` from `parentElement`: jsdom has
no layout engine, so every element would otherwise look hidden and the dialog
focus trap would find no tabbable children.

The fault-injection tests in the previous section share one machine with the
build, and a saturated link plus a busy CPU can push the sender past its join
timeout. Run `cargo test -p udp-chaos --test transfer_faults` on its own, or
re-run it before treating a single timeout as a regression. Both
`a_receiver_restart_resumes_from_its_checkpoint` and
`a_short_outage_recovers_without_losing_blocks` have been observed to fail only
inside a full `cargo test --workspace` run and pass alone in under two seconds,
so re-run the file before investigating.

`fluxy-net`'s `a_source_changed_mid_transfer_returns_3002` used to be
load-sensitive and is now deterministic. It waited a fixed 60 ms and then
rewrote the source file, which could be read mid-write: a chunk then failed its
manifest hash and the receiver closed the connection, so the sender reported a
transport error instead of 3002. The test now grows the file with an append
(after the source has actually served two chunks, counted through an
`AtomicUsize`) so the bytes the manifest describes stay intact and only the
source identity changes - which is the case the sender's own check is supposed to
catch. `cargo test -p fluxy-net --lib a_source_changed_mid_transfer` is stable
under load now.

## Transfer details and notifications (M3-02)

M3-02 turns a transfer row into something a user can diagnose and act on:

~~~text
cargo test -p fluxy transfer::tests
cargo test -p fluxy notify::tests
bun run test:frontend
~~~

- The **Details** dialog is available for every task, not only a failed one. It
  lists the state, direction, peer, progress with speed and remaining time, and
  the creation and update clock; a failed task adds the reason, the protocol
  code with its number, the retryability the core recorded, and the raw message.
  "Copy details" writes the same fields as text.
- **Failure reasons** live in `src/lib/transfer-errors.ts` and resolve the
  stable snake_case protocol codes to `transfers.reason.<code>`. The catalog
  guard reads `ErrorCode::as_str` out of `crates/fluxy-core/src/error.rs` and
  fails when a code has no copy in either locale, so the enum stays the single
  source of truth; a code from a newer peer falls back to a neutral message
  instead of showing a raw key.
- **Retry** is offered only where the core marks the failure retryable, or for a
  user cancel, in the row and in the dialog; a terminal failure such as a hash
  mismatch or a changed source asks for a new send instead of repeating a futile
  attempt.
- **Notifications** now cover transfer outcomes. `ShellLabels` gained transfer
  completion and failure labels, and `Notifier` gained
  `notify_transfer_completed`/`notify_transfer_failed`, which carry the file
  name and the peer name and never file content. `TransferRuntime` calls them
  from `complete` and `fail_with` and never from `cancelled`, so a user cancel
  does not interrupt. A per-device two-second window keeps a bulk send that
  finishes at once to one notification instead of one per file, and the M3-01
  notification preference mutes the whole path.

`notify.rs` records every requested dispatch behind `#[cfg(test)]`, which is
what lets `a_batch_burst_raises_one_transfer_notification_per_device` and
`completion_and_failure_notify_but_a_cancel_does_not` assert the routing without
a running Tauri app. The Windows toast caveat from M1-07 still applies to a
manual check: an installed build is required for native toasts, and a
development build may show PowerShell branding.

## Automatic clipboard watching (M3-03)

The automatic clipboard is off by default and only runs in the desktop shell:

~~~text
cargo test -p fluxy clipboard::tests
cargo test -p fluxy chat::tests
cargo test -p fluxy-app
bun run test:frontend
~~~

- `ClipboardWatcher` (in `src-tauri/src/clipboard.rs`) is a pure state machine
  so every rule can be tested without a real clipboard server or a live peer:
  it is fed the current text and a clock, and it answers "send this now?".
  `the_watcher_sends_only_after_the_value_has_been_stable` covers the debounce
  (a changed value restarts the window), `the_watcher_never_sends_the_same_content_twice`
  covers the duplicate rule, `received_content_is_never_sent_back` covers the
  loop rule, and `the_watcher_ignores_empty_and_oversized_content` covers the
  1 MiB cap plus `reset()` after the switch is turned off.
- `the_watcher_never_sends_received_content_back` (in `chat.rs`) drives the
  real path over a fake clipboard: it applies an incoming offer, asserts the
  text really is on the local clipboard, and then asserts that repeated watch
  ticks stay quiet - the T-10 "two devices, no loop" case - before checking that
  locally copied text is still sent after the debounce.
- `automatic_clipboard_sending_is_off_by_default_and_persists` covers the
  preference: off at startup, persisted on change, and visible in the snapshot.
- Enabling the switch records the value that is already on the clipboard instead
  of sending it, so turning the feature on never broadcasts a password copied a
  moment earlier. `enabling_the_watcher_does_not_send_what_is_already_on_the_clipboard`
  covers the watcher rule and
  `enabling_the_watcher_records_but_does_not_send_the_current_clipboard` covers
  the shell path that primes it; what the user copies next is still sent.
- A manual check needs two desktop instances: enable **Auto send** on both, copy
  text on A, and watch B's clipboard update once; then copy text on B and watch
  it come back to A. Nothing should bounce a second time, and turning the switch
  off must stop all clipboard reads.

## Tray, login item, and the firewall guide (M3-04)

The shell now outlives its window, and the login item is real on every platform:

~~~text
cargo test -p fluxy startup::tests
cargo test -p fluxy contract::tests
cargo test -p fluxy --test native_shell
bun run test:frontend
~~~

- `the_login_command_starts_the_shell_hidden` pins the command line a login item
  runs: the executable plus `--hidden`, which is what makes a login start land in
  the tray instead of on screen.
- The Windows backend never runs `reg.exe` in tests. `WindowsStartup` takes an
  injected `CommandRunner`, so `the_run_key_value_hides_the_window_and_is_per_user`
  asserts the pure argument builders (per-user `HKCU` key, `REG_SZ` value ending
  in `--hidden`, `/f` to overwrite) and
  `the_windows_backend_reports_and_writes_the_login_item` asserts the call order
  (query, add, delete) and the exit-code mapping, including
  `a_failed_registration_is_reported_but_a_missing_removal_is_not` - removing a
  value that is not there exits non-zero and is still the state the caller wanted.
- `the_file_formats_escape_the_executable_path` covers the macOS plist and the
  XDG desktop entry with a path containing `&`, `<`, quotes, and spaces, and
  `the_file_backend_writes_and_removes_the_login_item` exercises the file backend
  in a temp directory, including the second removal being a no-op.
- `the_login_item_follows_the_switch_and_is_persisted` drives the shell: off by
  default, written on enable, persisted to `config.json`, removed on disable, and
  the location always reported. `a_rejected_login_item_leaves_the_preference_alone`
  asserts that a refused write returns `autostart_failed` (4601) and does *not*
  flip the preference, and `startup_repairs_a_login_item_that_went_missing`
  covers the self-heal path (repair only while the preference is on).
- `closing_the_window_hides_until_the_tray_quits` covers the M3-04 criterion at
  the state level: a close hides, and only a deliberate quit lets it through.
- `the_tray_help_entry_asks_the_frontend_for_the_help_page` asserts that the tray
  raises `AppEvent::Shell`; `AppShell.test.tsx` asserts that the shell consumes it
  and routes to `/help`; `HelpPage.test.tsx` asserts the live ports appear in the
  platform commands, that a copy reports its result, that the FAQ answers carry the
  discovery port, and that the login-item state is shown rather than assumed.
- `LocalDeviceSettings.test.tsx` covers the settings switch, including a rejected
  write that leaves the switch showing the persisted value.

Manual check: run the desktop shell, close the window, and confirm the process is
still listed and a second instance (or the CLI) can still reach it - the tray icon
must restore the window on left click, and **Quit** must be the only action that
ends the process. Then enable **Start at login** from the tray or settings, sign
out, sign in, and confirm the app appears in the tray with no window. The login
item location is shown in the settings panel and on the help page, so it can be
inspected directly (Windows: `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).

## Multi-interface selection and binding (M2-11)

The listener is no longer "the first private IPv4 address". The selection is a
pure function over the interface table, so the interesting cases are unit tests,
and only the platform pin needs a real socket:

~~~text
cargo test -p fluxy-net --lib interface
cargo test -p fluxy-app --lib config
cargo test -p fluxy-app --lib engine
cargo test -p fluxy --lib network
cargo test -p fluxy --lib transfer::tests::pause_active
cargo run -p fluxy-cli -- interfaces
~~~

- `the_default_route_interface_wins`, `ethernet_and_wifi_rank_equally_without_a_default_route`,
  and `the_display_order_puts_the_best_candidate_first` pin the automatic order.
- `a_virtual_adapter_does_not_win_over_a_real_one` covers the case that motivated
  the heuristic: a Hyper-V `vEthernet` adapter is reported as an Ethernet-type
  interface and can even own the default route, so the name and the platform
  description are what demote it. The test also asserts the fallback - a host whose
  only path is virtual still gets a listener.
- `a_tunnel_is_only_used_when_nothing_else_is_up` and
  `loopback_needs_the_development_switch_and_never_wins_on_its_own` cover the two
  filters the plan asks for: a VPN is never preferred, and loopback is never part
  of the automatic pick.
- `manual_selection_is_validated_against_the_live_table` asserts that a missing
  interface and a missing address are distinct errors and that both convert to the
  plan's 2003. `a_manual_loopback_pick_still_needs_the_development_switch` keeps
  the loopback rule in manual mode too.
- `a_link_local_address_carries_its_scope` checks the `address%interface` display,
  the scope id on the socket address, and that a global address needs no scope.
- `a_socket_can_be_pinned_to_an_interface` is the only test that touches a real
  socket: it creates a UDP socket and applies the platform option to a real
  adapter. It skips when the host has no usable interface. This test is what found
  the Windows detail worth remembering: `IP_UNICAST_IF` wants the interface index
  in **network byte order**, and a host-order value fails with WSAEADDRNOTAVAIL.
  The runtime treats any rejected pin as a warning (`interface_pinned: false`)
  rather than failing the listener, because routing can still deliver the packets.
- `the_interface_selection_persists_and_rejects_a_bad_manual_pick` drives the
  engine: a manual pick round-trips through `config.json`, an unparsable one is
  refused with 2003 without changing the stored preference, and going back to
  automatic clears both fields.
- `automatic_mode_follows_a_better_interface`, `automatic_mode_reports_a_missing_network`,
  `manual_mode_never_downgrades_silently`, and `manual_mode_rebinds_when_the_index_changes`
  cover the two-second poll as a pure decision function: keep, rebind, or report
  the loss. The manual cases assert that a better interface appearing does *not*
  move a manual choice, and that a disabled adapter (name kept, address gone)
  counts as lost.
- `pause_active_stops_every_moving_transfer` asserts the rebind hook: only moving
  transfers pause, and a second rebind finds nothing to do.

One naming trap cost an afternoon and is now pinned by a test: the interface
selection is built on `default-net`, whose Windows `name` is an adapter GUID, while
mDNS (`mdns-sd`) and the gateway fallback go through `if-addrs`, whose Windows
`name` is the adapter's friendly name. Passing the selection's `name` straight into
`DiscoveryConfig.interface` compiles, binds, and then silently reports
`discovery=unavailable` because no interface matches. `InterfaceInfo::discovery_name`
is the translation (friendly name on Windows, platform name elsewhere), and both
`local_addresses` and the fallback target builder now also accept the GUID, so a
caller can pass either spelling. `the_discovery_name_follows_the_platform_convention`
locks the rule in.

Manual check (this is the T-11 row): on a machine with more than one adapter, run
`cargo run -p fluxy-cli -- interfaces` and read the list - the `<- automatic` mark
must be on the interface that actually carries the LAN, not on a VPN or hypervisor
adapter. Then bind explicitly and pair from another host:

~~~text
cargo run -p fluxy-cli -- serve --interface "Wi-Fi" --data-dir ./.fluxy-a
cargo run -p fluxy-cli -- serve --listen 127.0.0.1:0 --dev-loopback --data-dir ./.fluxy-b
~~~

The single-host half of this row was run on a Windows laptop with Ethernet
(down), Wi-Fi, a Hyper-V adapter, and Bluetooth: `interfaces` marked the Wi-Fi
adapter (`WLAN`, 192.168.1.8, default route) as the automatic pick and left the
virtual adapters unmarked; `serve --interface WLAN` bound 192.168.1.8 with
`interface_pinned=true` and `discovery=mdns`; `discover --interface WLAN` found
that service through mDNS on the same adapter; `pair` completed the SAS handshake
against the LAN address; and `chat` resumed the trust without a second SAS prompt
and reported `delivered=true`. What is still a two-machine check is moving a cable
or joining a different network while a transfer is running.

The serve line prints `selection=`, `listen=`, and `interface_pinned=`. Pull the
cable (or disable the adapter) while a serve is running: an automatic selection
prints a new `listen=` line after the two-second poll, while a manual selection
keeps the address and the desktop shell reports 2003 until it comes back.

## Network interface settings and diagnostics (M3-06)

M3-06 is a settings-page milestone, so its tests are component tests plus the
contract they rely on:

~~~text
bun run test:frontend
cargo test -p fluxy --lib contract::tests::a_bad_network_selection
cargo test -p fluxy --lib network::tests
~~~

- `NetworkSettings.test.tsx` renders the panel against a fixture with an
  Ethernet adapter (recommended, default route, active) and a Wi-Fi adapter, and
  asserts that the active selection is explained rather than just displayed: the
  pinned-socket note, the recommended and in-use marks, both addresses, the
  automatic-mode hint, and the promise that the interface list stays local.
- The switch is a two-step interaction on purpose, and the test asserts both
  steps: picking an adapter enables **Apply**, **Apply** opens the confirmation
  that names the two consequences (sessions drop, transfers pause and resume from
  their checkpoint), and only confirming sends the command -
  `("manual", "Wi-Fi", "10.0.0.5")` in the fixture.
- A rejected switch has its own test: the command throws, the panel keeps the
  proposed selection, **Apply** stays enabled for a retry, and the recovery block
  shows the reason next to "choose an adapter, then apply". A runtime state of
  `unavailable` with a manual selection shows the "the chosen address is
  gone" wording instead, so a lost address is not reported as blocked discovery.
- Two smaller cases cover the honest edges: `interface_pinned: false` produces
  the "traffic may still leave through a VPN" note, and a virtual adapter plus a
  disconnected adapter produce their own chips. Automatic mode leaves the rows
  inert so the list cannot be mistaken for a choice.
- The copy action writes the selection header (mode, source, interface, address,
  listen address, pinned, rebind count) followed by one line per adapter with its
  kind, index, flags, and addresses - the plan's "diagnostics can copy interfaces
  and addresses".
- `a_bad_network_selection_is_rejected_with_2003` pins the command's own contract,
  which the panel's guidance depends on: an unknown mode, an unparsable address, a
  manual pick without an interface or without an address, and blank strings (what
  an untouched form control sends) all return 2003 with a message that names what
  is missing.
- `SettingsPage.test.tsx` now lists the network section among the sections the
  plan requires, so removing it fails a test.

Manual check: open **Settings** on a machine with more than one adapter. The
panel must name the adapter Fluxy is using, mark the recommended one, and mark any
VPN or hypervisor adapter. Switch to **Manual**, pick the other adapter, and
confirm: the tray keeps working, the peers list repopulates through discovery
within a few seconds, and an in-flight transfer shows **paused** and then resumes.
Then pick an adapter whose address does not exist (a manual config edit is the
cheapest way) and confirm the panel shows the recovery block instead of a generic
error.

## Folder batches (M3-07)

A folder is sent as a batch of single-file transfers, so this milestone is mostly
about the path boundary and the batch row:

~~~text
cargo test -p fluxy-core --lib transfer::batch
cargo test -p fluxy-core --lib files::folder
cargo test -p fluxy --lib transfer::tests
bun run test:frontend
~~~

- `transfer::batch` is a table of hostile paths. `absolute_and_traversal_paths_are_refused`
  covers `/etc/passwd`, a UNC path, `C:/...`, `..`, `a/../b`, and `a/./b`;
  `platform_hostile_paths_are_refused` covers empty segments, backslashes, NUL,
  control characters, the bidirectional overrides that make a name render as a
  different name, ADS colons, `CON`/`con.txt`/`a/NUL/b.txt`, trailing dots and
  spaces, and an over-long path. `unicode_paths_survive_byte_and_segment_limits`
  pins the 255-byte segment budget against three-byte characters.
- The plan-level rules have their own tests: duplicates that differ only in case,
  a path that is both a file and a directory (in both directions), the 10,000
  entry cap, sorting and totals, and a JSON round-trip that rejects a foreign
  schema. `an_empty_plan_is_valid_but_an_oversized_one_is_not` records a decision:
  an empty folder is a legitimate batch because the receiver still creates the
  root directory.
- `files::folder` walks a real tree: nested files, empty directories (only the
  outermost of an empty subtree is listed), Unicode names, a file passed instead
  of a folder, an empty folder, and a symbolic link - the link test creates a real
  link and skips when the account lacks the privilege, which is the honest
  platform behaviour rather than a false pass.
- `a_batch_offer_is_placed_under_a_fresh_root_with_its_empty_directories` drives the
  runtime placement: the sink directory is under `received/holiday/photos`, the
  declared empty directory exists after the first offer, and a second batch with
  the same root gets `holiday (1)`. `a_batch_offer_with_an_unsafe_path_is_refused`
  and its neighbours cover an unsafe relative path, a path outside the entry list,
  and an entry-list size that disagrees with the offer.
- `registering_a_folder_describes_its_files_and_empty_directories` covers the IPC
  summary, including the 3003 for a file passed as a folder and for an
  unregistered folder id.
- `TransfersPage.test.tsx` covers the "one batch row" requirement: two files of
  one batch collapse into a single row with the root name and the file count, the
  files appear only after expanding, batch-wide pause touches only the file that is
  moving, and "Send folder" opens the confirmation with the root name, file count,
  empty-directory count, and total size before `start_folder` is called.

Manual check (the T-13 row): send a nested folder to a second machine and compare
the tree and every file's BLAKE3 against the source, including an empty directory,
a Unicode file name, and a name that would be hostile if it were accepted. Then
send a folder containing a symbolic link and confirm the sender refuses it before
offering anything. The 10k-file and 10 GiB cases belong to the same matrix and are
run per release rather than per test suite.

## Regression pass (M3-05)

M3-05 is a checkpoint rather than a feature, so it has two halves: the gates, and
the defects the gates do not catch.

~~~text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
bun run check
bun run test:frontend
bun run check:security
bun run build
bun run test:e2e
~~~

The suite was run three times in a row under load. Two defects were found by
reading rather than by a red test:

- **A dropped folder was refused.** `register_paths` only understood files, so
  dropping a folder produced a "rejected" entry even though "Send folder" worked.
  The drop path now scans and validates the folder before anything is offered, the
  event carries `folders`, and the confirmation opens from the drop. Folders have
  their own budget (`MAX_DROPPED_FOLDERS`) because one folder can hold 10,000
  files, and a second dropped folder is counted rather than silently dropped.
  `a_dropped_folder_becomes_a_batch_and_a_file_stays_a_transfer` and
  `opens the folder confirmation when a folder is dropped` cover it.
- **The drop budget counted the wrong thing.** The file limit advanced on *accepted*
  files, so a drop of unreadable paths never reached it and walked the whole list.
  It now advances on every file path considered, matching what the limit meant, and
  the existing "many missing files" assertion pins it again.

The regression checklist also re-verified the behaviours that earlier milestones
call "must not regress": the automatic clipboard watcher stays off by default with
its 300 ms poll, a close request still hides to the tray unless a quit is
deliberate, the window is still created hidden and shown unless `--hidden`, the
login item still launches with `--hidden`, tray check items still revert when
the platform refuses a write, the help entry still raises its shell event, and a
manual network selection still reports 2003 instead of downgrading.

## Throughput benchmark (M5-01)

The loopback number in plan 11.3 (≥ 1.0 GiB/s) is measured by an ignored test, so
it is reproducible rather than remembered:

~~~text
cargo test -p fluxy-net --release --lib benchmark_loopback_throughput -- --ignored --nocapture
FLUXY_BENCH_STREAMS=8 FLUXY_BENCH_WINDOW=4 cargo test -p fluxy-net --release \
    --lib benchmark_loopback_throughput -- --ignored --nocapture
~~~

It moves 256 MiB through two real endpoints (TLS, QUIC, four chunk streams) and
writes the received file to disk, then prints MiB/s and GiB/s.

What the sweep showed, in one table:

| Knob | Range tried | Effect |
| --- | --- | --- |
| Chunk streams | 4 → 16 | none beyond noise |
| Window per stream | 2 → 4 | none beyond noise |
| Block size | 1 → 4 MiB (protocol cap) | none; 8 MiB is rejected by the chunk limit |
| Initial MTU | 1452 → 9000 | +5%; 16000 fails the handshake |
| UDP socket buffer | 64 KiB → 8 MiB | none on loopback |
| QUIC receive window | 8/32 MiB → 16/128 MiB | none |
| Per-chunk copies | 3 → 1 | none |

Baseline 242 MiB/s, best 266 MiB/s. The conclusion is that a single UDP socket's
packet rate and the per-chunk control round trip dominate, not bandwidth or
copying, so meeting 1.0 GiB/s needs a structural change (multiple sockets or
GSO/GRO, neither available through quinn on Windows) rather than parameter
tuning. 1 GbE's 105 MB/s is met with a wide margin, so the practical target for
this release is unaffected. The kept changes are documented in
docs/IMPLEMENTATION_STATUS.md.

## Security audit and fuzzing (M5-03)

~~~text
bun run audit
node scripts/security/audit.mjs --fuzz-iterations 35000000
cargo deny check licenses bans sources
cargo deny check advisories        # needs network: clones the RustSec database
FLUXY_FUZZ_ITERATIONS=1000000 cargo test -p fluxy-core --release --test decoder_fuzz
~~~

- `deny.toml` is the policy, not a report: the license allow list, the
  crates.io-only source rule, and the wildcard ban are enforced by
  `cargo deny`. The first run failed on wildcards because every workspace
  dependency was path-only, which is now an explicit version on the path
  dependency. Duplicate versions stay warnings (about 45 crates, all from the
  Tauri toolchain) and are listed as a known item instead of being forced into
  one version.
- `crates/fluxy-core/tests/decoder_fuzz.rs` is the fuzz harness. It is
  deterministic on purpose: a fixed seed and a reproducible generator mean any
  finding can be replayed, and it runs on stable where `cargo-fuzz` cannot.
  The interesting part is the generator: pure random bytes stop at the magic
  check, so the harness mutates a valid sample - bit flips plus truncation - for
  about three quarters of the iterations, which is what reaches the offset, length,
  and hash arithmetic.
- `the_fuzz_samples_are_themselves_valid` is the harness's own guard: if a
  sample ever stops being valid, the mutation loop would silently degrade into
  testing the magic check over and over.
- Coverage: `decode_frame`, `decode_payload::<Hello>`, `ChunkHeader::decode`,
  `Manifest::from_bytes`, `TransferCheckpoint::from_bytes`, `BatchPlan::from_json`,
  `validate_relative_path`, and `sanitize_root_name`.
- The campaign numbers on this machine: 20,000 iterations in about 1 s (the
  default, run by every `cargo test`), 10,000,000 in 175 s, and
  35,000,000 for the plan's ten-minute run.

Manual gate before tagging a release: run `cargo deny check advisories` on a
networked machine. Its `ignore` list is empty on purpose, so any advisory fails.

## Packaging and installers (M5-04)

~~~text
bun x tauri build --no-bundle          # release binary only
bun x tauri build --bundles nsis       # installer
pwsh -NoProfile -File scripts/package/windows.ps1
~~~

- The bundle section of `src-tauri/tauri.conf.json` is the release contract:
  publisher, copyright, category, descriptions, `licenseFile`, per-user NSIS
  installs, and the two UI locales for NSIS and WiX. Changing an installer
  string means changing that file, not a script.
- `scripts/package/windows.ps1` builds and then reports the binary's version
  resource and every installer it produced, so a release log has sizes and
  versions without anyone remembering them.
- Install verification is scripted rather than visual: a silent install into a
  temp directory must exit 0 and produce `fluxy.exe` and `uninstall.exe`,
  and a silent uninstall must exit 0 and leave no directory behind. That is the
  check this repository can run on any Windows host:

~~~powershell
$dir = Join-Path $env:TEMP 'fluxy-install-test'
Start-Process target/release/bundle/nsis/Fluxy_0.2.0_x64-setup.exe -ArgumentList '/S', "/D=$dir" -Wait
Get-ChildItem $dir -Filter *.exe
(Get-ChildItem $dir -Filter 'uninstall*.exe').FullName | ForEach-Object { Start-Process $_ -ArgumentList '/S' -Wait }
~~~

- Signature status is part of the record: an unsigned installer reports
  `NotSigned` from `Get-AuthenticodeSignature`, and that is a release
  blocker to clear with the signing paths in docs/PACKAGING.md, not something to
  leave undocumented.

The clean-machine checklist in docs/PACKAGING.md is the manual half of the plan's
acceptance and must be run on a host that never had Fluxy before tagging.

## Cross-platform matrix (M5-02)

~~~text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p fluxy-core --release --test decoder_fuzz
bun run check && bun run test:frontend && bun run check:security && bun run build
bun run test:e2e
~~~

The commands are the same everywhere; what differs is where they run. `.github/workflows/ci.yml` runs the matrix on pull requests: the Rust set on Ubuntu,
macOS, and Windows, the web set on Linux, and the CLI scenario on all three. A merge
to main does not repeat it - the tree is the one the pull request just tested - so
`.github/workflows/warm-cache.yml` runs after a dependency change instead and
refills the caches the next pull request restores.

- `docs/RELEASE_MATRIX.md` is the authoritative matrix: one row per check,
  one column per platform, each marked local, CI, or manual, with the reproduce
  command and the known platform-specific areas (listener pinning, tray, login
  item, clipboard, notifications).
- The frontend build has to run before cargo: `tauri-build` embeds the
  frontend, so a clean checkout cannot compile the shell crate until `dist/` exists.
  The matrix legs install Bun and build first for that reason.
- One-host cross checks are recorded where they are meaningful:
  `cargo check -p fluxy-proto -p fluxy-core --target x86_64-unknown-linux-gnu` passes on
  the Windows host and covers the protocol, file, transfer, batch, and path
  safety layers. `fluxy-net` is excluded because `ring` needs a Linux C
  cross-compiler; that is a toolchain gap and the CI Linux leg closes it.

Rows that cannot be automated on one host stay manual and are listed in the
matrix as such: two-machine transfers (T-11, T-13), a clean-machine install, tray
and login-item behaviour per platform, and toast delivery on Windows.

## Documentation checks (M5-05)

Documentation is code that nobody compiles, so at minimum its links have to
resolve:

~~~text
bun run check:docs
~~~

`scripts/check-docs-links.mjs` walks the README and every file in `docs/`,
extracts relative Markdown links (skipping external URLs and anchors), and fails
if a target does not exist. It runs in the CI `web` job next to the UI
security scan, so renaming a document without updating the index is a red build
rather than a dead link found by a reader.

The documents this milestone added, and what each is for:

| Document | Audience | Kept honest by |
| --- | --- | --- |
| `docs/USER_GUIDE.md` | Someone installing Fluxy for the first time | It mirrors the shipped UI: switch names, the three clipboard policies, the 300 ms watcher, the six-digit code, error codes |
| `docs/KNOWN_ISSUES.md` | Anyone deciding whether the build is usable | Each entry is a limitation that still exists; the gap list in IMPLEMENTATION_STATUS.md is its source |
| `docs/RELEASE_CHECKLIST.md` | The release manager | Every row has a command and an expected artifact, so a tick is evidence |
| `docs/RELEASE_NOTES_0.2.0.md` | Users and reviewers | Written from the implementation, with the known issues repeated rather than softened |

The checklist deliberately repeats test counts (302 Rust, 90 frontend) that also
live in IMPLEMENTATION_STATUS.md. That is duplication by design: a release
checklist that points at another document for its numbers is the document people
stop reading. Update both when the counts change, and `check-docs` makes sure
the links between them still work.

## Soak runs (M5-06)

~~~text
node scripts/soak/soak.mjs --minutes 8 --size-mib 8            # a short run
node scripts/soak/soak.mjs --minutes 1440 --size-mib 8 --interval-seconds 30   # the 24 h gate
~~~

The harness pairs once with the real CLI, then loops chat and an 8 MiB transfer
over loopback, verifying every received file by SHA-256 and sampling the server's
resident memory. Each round is one JSON object in `.soak/log.jsonl`; the run
ends with `.soak/summary.json`. Three consecutive failed rounds stop it, because
that is a defect rather than a flake.

It found two defects on its first real run, both in the CLI server that the soak
uses as the receiving side:

1. **`serve` could not receive files.** The long-running server only printed
   `transfer-offer` and never accepted, so every send timed out after the offer
   window. `serve --accept-files` now receives and prints the outcome; the desktop
   app still asks the user, which is why this is a flag.
2. **Sessions outlived their peer.** The loop could only accept the next
   connection after the current session ended, and a scripted client that exits
   right after `close` never sends the QUIC close frame, so the server waited for
   the transport timeout and later peers timed out. The server now closes an idle
   session after 10 s, and the client waits 150 ms after `close` so the frame
   leaves. Rounds went from an occasional 9-21 s to a steady 0.3-0.6 s.

What the runs measured on this machine:

| Run | Rounds | Failures | Data | Per-round | Server RSS |
| --- | --- | --- | --- | --- | --- |
| 3 min, 8 MiB | 90 | 0 | 720 MiB | 0.31-0.64 s | 12-13 MB |
| 20 min, 8 MiB | 598 | 0 | 4,784 MiB | 0.31-0.64 s | 12-26 MB |

The 20-minute run is the longest recorded here. The resident set grew from 12 MB to
26 MB over 598 sessions, which is small but not flat: the 24-hour gate should
confirm whether it converges, and a monotonic climb would be the one thing that
would move this row from "green" to "investigate".

Evidence to record for a release candidate: the summary JSON from a run of at
least an hour, and for the release gate a 24-hour run with `failures: 0` and a
flat `memory_last_mb` (the desktop binary is what ships; the soak exercises the
protocol and the CLI receiver, not the WebView).

## Receive directory (settings)

~~~text
cargo test -p fluxy-app --lib config
cargo test -p fluxy --lib transfer::tests::the_receive_root_switches_and_is_created
bun run test:frontend -- src/components/LocalDeviceSettings.test.tsx
~~~

- `the_receive_directory_persists_and_rejects_a_file` covers the stored
  preference: the directory is created on demand, the value round-trips through
  `config.json`, a file is refused, and the previous value survives
  the refusal.
- `the_receive_root_switches_and_is_created` covers the runtime: the root moves,
  the new directory is created, and a refused path leaves the old root in place.
- The component test drives the panel: the current directory is shown, the folder
  picker result is saved, and the reset action sends `null` for the default.

Manual check: change the directory, receive a file, confirm it lands there, then
reset and confirm the next file lands under the data directory again. A transfer
already running when the setting changes finishes in the old directory, which is
intended.

## Android build and 16 KB pages (A-01)

~~~powershell
bun run android:build                     # debug, arm64-v8a
bun run android:build -- --release       # release (needs: bun run android:keystore)
~~~

The script sets `JAVA_HOME`, `ANDROID_HOME` and `NDK_HOME`, runs
`tauri android build`, then extracts the APK and checks that every `.so` has
16 KB-aligned LOAD segments. That check is not cosmetic: Android 15+ refuses a
library that is only 4 KB aligned, and NDK r27 does not default to 16 KB, so
`.cargo/config.toml` passes the linker flag explicitly.

Current evidence: `app-universal-debug.apk` is 272,068,744 bytes, ships one
ABI (`arm64-v8a`) with `libfluxy_lib.so`, every LOAD segment reports
`0x4000`, and `apksigner` reports the Android debug certificate.

Porting notes from the first build, because they will matter for A-02 through
A-07: `arboard` has no Android backend, the tray and folder-picker APIs are
desktop-only, and `build.rs` must key its Windows resource on the target OS
rather than the host.

## Mobile plugin (A-02)

~~~text
cargo check -p tauri-plugin-fluxy-mobile
cargo check -p tauri-plugin-fluxy-mobile --target aarch64-linux-android
bun run android:build
~~~

The plugin crate is part of the workspace, so the desktop gates cover its Rust
side. The Android side is proven by the APK: after a build, `classes6.dex` in
the universal APK contains `FluxyMobilePlugin`, `KeystoreSecrets` and
`secretGet`, which means Gradle compiled the module and the app links it.

One Tauri detail worth recording, because it cost an iteration: the plugin handle
for the Kotlin class comes from `api.register_android_plugin(...)` inside
`setup`, not from a public accessor on `PluginApi`, and `run_mobile_plugin`
returns `PluginInvokeError` rather than `tauri::Error`.

### Identity across a restart (A-02)

~~~text
adb -s emulator-5554 install -r -t src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb -s emulator-5554 logcat -c
adb -s emulator-5554 shell am force-stop com.thep0y.fluxy
adb -s emulator-5554 shell am start -n com.thep0y.fluxy/.MainActivity
adb -s emulator-5554 logcat -d | Select-String 'sealed in the platform keystore'
adb -s emulator-5554 shell run-as com.thep0y.fluxy ls -1 fluxy-identity.bin identity.json
~~~

The sealing task logs the device id it sealed, which is the evidence for the plan's
acceptance ("a restart keeps the identity"). Two cold starts on an Android 37
x86_64 emulator both reported `device_id=01a17795-cf51-75f5-b309-f95b79494632`; after each
launch `fluxy-identity.bin` exists and `identity.json` does not, so the plain copy
only lives between the Kotlin decrypt and the post-start seal.

Why it is shaped that way: `Engine::load` is synchronous and runs in the setup hook,
while `run_mobile_plugin` is async and would deadlock if the main thread blocked
waiting for it. The Kotlin constructor therefore materialises the plain file
first (it runs inside `register_android_plugin`, before the hook), and the seal
happens after the engine is up. A crash in between leaves a working plain
identity rather than none.
The Keystore path is not verified on a device yet: `Engine::load` still reads the
identity from a JSON file, so the "restart keeps the device id" acceptance for
A-02 is unproven until that wiring lands and an emulator or phone confirms it.

### Android descriptor sources (A-03)

~~~text
cargo test -p fluxy-core --lib files::android
~~~

Four tests cover the send-side file layer: a descriptor registered from a URI
reads at an offset and refuses a read past the end, a blank display name falls
back to `file-<size>`, appending to the file after registration
makes `verify_unchanged` return 3002, and a dropped then reopened descriptor
reports the same identity and still reads a later range - the shape a resume
takes after a restart. The plugin half is proven by the APK build:
`SourceBridge.kt` compiles against `contentResolver` and the descriptor
handover, and `fs_open_source` is registered in the command list. The
emulator log shows `android file manager installed` next to a sealed
identity, so the manager swap leaves identity handling intact.

The registration chain is covered by the existing transfer suites, which now run
through a handle taken at registration instead of a path resolved per send: 308
Rust tests pass unchanged, so the refactor is a no-op on desktop. Not covered yet:
a real pick-and-send from the mobile UI (there is no mobile entry point yet) and a
10 GiB resume.

### Android receive location and export (A-04)

~~~text
adb -s emulator-5554 shell run-as com.thep0y.fluxy cat receive-dir.txt
adb -s emulator-5554 shell ls -d /storage/emulated/0/Android/data/com.thep0y.fluxy/files/Download/Fluxy
adb -s emulator-5554 logcat -d | Select-String 'receive directory set'
~~~

The plugin writes the preferred directory before the setup hook, so the shell
reads it synchronously: the log shows `android receive directory set` with the
external Download path, and both the marker and the directory exist on the
emulator.

Export is implemented (`MediaStore.Downloads` with `IS_PENDING`, pending cleared only
after the copy) but not yet wired to a command the WebView can call, and no
10 GiB receive-and-export run has been recorded. The pre-accept `StatFs` check
is available as `storage_available` and not yet enforced.

### Mobile document picker

~~~powershell
adb -s emulator-5554 shell run-as com.thep0y.fluxy touch probe-picker.txt
adb -s emulator-5554 shell am force-stop com.thep0y.fluxy
adb -s emulator-5554 shell am start -n com.thep0y.fluxy/.MainActivity
adb -s emulator-5554 logcat -d | Select-String 'picker probe'
~~~

The debug build opens the picker once when `probe-picker.txt` exists (and deletes it),
so the round trip is testable from a headless emulator with no UI. Evidence:
the system log shows `START u0 {act=android.intent.action.OPEN_DOCUMENT ...}` from the app
and DocumentsUI as the resumed activity; sending a back key ends with
`picker probe: cancelled`.

Why the registry: `Plugin` has no `onActivityResult`, and AndroidX refuses
`registerForActivityResult` after the activity started, but
`activityResultRegistry.register(key, contract, callback)` is allowed at any time.
The plugin module also needs an explicit `androidx.activity` dependency:
the app has it, a library module does not inherit it.

### Mobile shell insets and port

~~~powershell
adb -s emulator-5554 shell am start -n com.thep0y.fluxy/.MainActivity
adb -s emulator-5554 shell cat /proc/net/udp | Select-String ' 10235 '
adb -s emulator-5554 shell screencap -p /sdcard/f.png; adb -s emulator-5554 pull /sdcard/f.png .
~~~

Two checks in one screenshot: the header sits below the status bar (the activity
content view pads itself with the system bar insets) and the subtitle shows the
bound port. Compare it with `/proc/net/udp`: the port in the UI matches the
socket the app owns.

Note the layer: the insets are consumed by `android.R.id.content`, not pushed into
CSS. Padding the WebView alone does not help, because its parent stays edge to
edge; CSS safe-area variables are an iOS concept and Android does not fill them.

### Status bar follows the theme

~~~powershell
adb -s emulator-5554 shell "cmd uimode night yes"   # or no
adb -s emulator-5554 shell screencap -p /sdcard/f.png
adb -s emulator-5554 pull /sdcard/f.png .
~~~

Switch the system theme and look at the strip behind the status bar: it has to be
the same colour as the page, and the clock and icons have to stay readable. The
colour is read from `--fx-bg` in the stylesheet, so this also catches a page that
stopped following the theme.

### Mobile network facts (A-05)

~~~powershell
adb -s emulator-5554 logcat -d | Select-String 'gateway|fallback'
adb -s emulator-5554 shell cat /proc/net/udp | Select-String ' 10235 '
~~~

The plugin answers `local_network_info` from `ConnectivityManager`: default-route
gateway, interface name, Wi-Fi transport, link addresses. Android does not expose
the interface table to apps, which is what the SELinux denials were about.

Status: the bridge and the runtime wiring are in, and the listener still binds.
The gateway value has not been proven to reach `FallbackDiscovery` yet - the old
`gateway=None` line disappeared without a replacement - so treat the discovery half of
A-05 as unverified until a device run shows it.

### Two Android target directories

A hand-set `cargo check --target x86_64-linux-android` and `tauri android build`
must not share a `target` directory. They pass different toolchain variables, so
ring's build script caches one answer and the other links an incomplete library;
the APK then dies at startup with

~~~text
UnsatisfiedLinkError: cannot locate symbol "ring_core_0_17_14__aes_gcm_dec_update_vaes_avx2"
~~~

The way out is to keep them apart:

~~~powershell
cargo check -p fluxy --target x86_64-linux-android --target-dir target/check-android
bun run android:build -- --target x86_64
~~~

If a build already went wrong, delete `target/x86_64-linux-android` and rebuild
through the script alone; the polluted directory is not repairable in place.

### When one direction discovers and the other does not

Discovery is two independent flows: each device advertises and each browses.
Check them separately before touching the code.

~~~powershell
# phone sees desktop?
adb shell logcat -d | Select-String 'FluxyDiscovery'
# desktop sees phone? (start it with stderr captured)
Start-Process target\debug\fluxy.exe -RedirectStandardError desk.log
Get-Content desk.log | Select-String 'discovered'
~~~

A one-way failure is usually the network. Windows Firewall needs an inbound
rule for the executable on the active profile; ICMP being blocked is normal and
proves nothing either way. A phone that never reaches the desktop also cannot
send its pairing request, so fixing the inbound path fixes both.

On the app side, discovery holds a `WifiManager.MulticastLock`: without it the
Wi-Fi chip drops multicast and broadcast frames addressed to the app and the UDP
fallback hears nothing.

A silent direction is often a failed advertisement rather than a blocked packet.
On Android the fallback used to inherit the wildcard interface name "any" and
could not bind to it, so the phone advertised nothing at all; `interface: None`
lets the OS choose and both directions work. `UDP fallback discovered a peer` on
each side is the line to look for.

### Pairing shows the same code on both screens

~~~powershell
# desktop, with stderr captured
Get-Content desk.log | Select-String 'pairing code is ready'
# phone
adb shell logcat -d | Select-String 'RustStdoutStderr'
~~~

The responder must log `pairing code is ready` within milliseconds of
`incoming pairing connection accepted` - before the initiator taps confirm. If it only
appears after the initiator acknowledges, the responder is showing nothing while
the other screen says "confirm on the other device", which reads as a lost request.

## UDP chaos proxy

`tools/udp-chaos` is a small UDP forwarding proxy. It keeps one upstream socket
per downstream client, so QUIC connection IDs and source addresses behave as
they would through a NAT. It supports:

| Fault | CLI flag | Library field |
| --- | --- | --- |
| Packet loss | `--loss 0.05` | `ChaosConfig::loss` |
| Fixed delay | `--delay-ms 20` | `ChaosConfig::delay` |
| Jitter | `--jitter-ms 10` | `ChaosConfig::jitter` |
| Duplicate | `--duplicate 0.1` | `ChaosConfig::duplicate` |
| Reorder | `--reorder 0.1` | `ChaosConfig::reorder` |
| One-shot outage | `--outage-after-ms 1000 --outage-duration-ms 300` | `ChaosHandle::set_outage` |

Probabilities are in `0.0..=1.0`. The outage window drops every datagram in
both directions; the session must recover through QUIC retransmission.

### Manual CLI run

Start a loopback server and note the printed address and fingerprint:

~~~text
cargo run -p fluxy-cli -- serve --listen 127.0.0.1:42420 --dev-loopback --data-dir ./.fluxy-chaos-server
~~~

In a second terminal route the client through the proxy:

~~~text
cargo run -p udp-chaos -- \
  --listen 127.0.0.1:42430 \
  --target 127.0.0.1:42420 \
  --loss 0.05 \
  --delay-ms 20 \
  --jitter-ms 10 \
  --outage-after-ms 3000 \
  --outage-duration-ms 500
~~~

Pair and chat through the proxy address. The server fingerprint is still the
real endpoint fingerprint because QUIC is end-to-end:

~~~text
cargo run -p fluxy-cli -- pair 127.0.0.1:42430 --fingerprint HEX --dev-loopback --data-dir ./.fluxy-chaos-client
cargo run -p fluxy-cli -- chat 127.0.0.1:42430 --device SERVER_UUID --message through-chaos --dev-loopback --data-dir ./.fluxy-chaos-client
~~~

For a real LAN test, use the server's LAN address as `--target` and omit
`--dev-loopback` from the client commands. The proxy itself only forwards
UDP; it never creates trust or bypasses the TLS/SAS checks.

### Automated coverage

~~~text
cargo test -p udp-chaos --test session_recovery
~~~

The suite covers:

- A complete outage during a chat write: the client retransmits after the link
  returns and still receives the application ACK.
- A fixed 20 ms delay on every datagram in both directions.
- Duplicate and reordered datagrams, with deterministic counters proving that
  the proxy actually injected the faults.

`UdpChaosProxy` exposes counters through `ChaosHandle::stats()` and a
`shutdown()` method so tests do not leak sockets or tasks.

## Current M0 limits

- The proxy has no bandwidth limit or application-aware fragmentation.
- The automated outage is one-shot; the library handle can toggle it repeatedly.
- A 30-second outage/reconnect soak and one-hour memory soak are M0 exit checks
  performed separately, not part of the per-commit test budget.
