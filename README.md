# Fluxy

English | [简体中文](README.zh-CN.md)

Fluxy is a local-first, LAN-only desktop and mobile transfer tool. It provides
device discovery, text chat, clipboard sharing, and resumable file/folder
transfer without an account, cloud relay, or persisted chat history.

This repository contains the v0.2.0 desktop implementation and the Android
v0.3.0 track.

## Fluxy 0.2 vs 0.1.x

0.2 is a rewrite of the transfer path, not a restyle of it. 0.1.x was a Tauri 1
app with a single Rust crate that served the far device a web page over plain
HTTP; 0.2 splits the engine into crates and moves the wire to QUIC. The two do
not interoperate, because 0.1.x's HTTP upload and 0.2's QUIC session share
neither transport nor framing, so both ends must be on the same line.

### Where 0.2 is better

- **The engine is a library, not a window.** 0.1.x's transfer code sat behind
  Tauri commands in `src-tauri/src/server/`, reachable only by opening the app.
  0.2 puts frames in `fluxy-proto`, transport and discovery in `fluxy-net`,
  offers, checkpoints and path rules in `fluxy-core`, and app state in
  `fluxy-app`. `fluxy-cli` drives that engine headlessly, and `tools/udp-chaos`
  injects loss, reordering and stalls against it in tests.

- **Encrypted and authenticated by construction.** A 0.2 session is QUIC over
  TLS 1.3 with a self-signed certificate per device, pinned at pairing.
  Discovery records are hints only: `pair` derives a six-digit code from both
  device IDs, nonces and certificate fingerprints, writes a trust pin only once
  both sides confirm, and refuses a device whose certificate, key or ID has
  changed since it was paired. 0.1.x served unencrypted HTTP, authenticated by
  a single-use token in the QR URL, so whoever held that link had the same
  access as the phone it was shown to, and the bytes on the wire were readable.

- **A transfer survives the network, not just the connection.** Files arrive in
  4 MiB chunks, each verified with BLAKE3 and the whole file checked against
  its root hash, and the receiver writes a checkpoint per chunk, so a pause, a
  crash or a restart resumes from the missing blocks. 0.1.x streamed one HTTP
  POST per file: a dropped connection meant sending that file again from zero.

- **Devices find each other instead of being handed an address.** mDNS
  (`_fluxy._udp.local.`) with a UDP broadcast and gateway-probe fallback, and
  an automatic adapter choice that avoids VPN and hypervisor interfaces and
  re-checks every two seconds. 0.1.x printed a URL for the phone to scan, and
  an address that had moved meant nothing connected.

- **Folders arrive as folders, and paths are checked first.** A batch carries
  the tree, empty directories included, and every relative path is validated
  before a byte is written: no absolute paths, traversal, reserved device
  names, alternate data streams, control or bidirectional characters, or
  case-only duplicates, and symbolic links are refused rather than followed.
  0.1.x moved one file at a time through the browser.

- **Chat and clipboard share the session.** Text messages and clipboard values
  travel over the same authenticated connection, with a per-device receive
  policy and a guarantee that a value which arrived from a peer is never sent
  back. 0.1.x had neither.

### Where 0.1.x is still better

- **Nothing to install on the far end.** 0.1.x's phone story was: scan the code
  in any browser, pick files, upload. 0.2 needs Fluxy on both devices, and the
  Android client is still on the 0.3.0 track, so a guest's phone cannot send
  you a file without installing an app first.

- **It updates itself.** 0.1.x shipped Tauri's updater, with a release
  endpoint and a minisign key, so a new version arrived by itself. 0.2 has no
  updater plugin: upgrading means downloading and installing again, over
  unsigned installers.

- **It could be poked with `curl`.** The 0.1.x surface was `GET /connect`,
  `GET /files`, `GET /download/<path>` and `POST /upload`, with JSON around it,
  so a shell could exercise it and a third-party client was an afternoon's
  work. 0.2's wire is encrypted QUIC carrying protobuf: you use the CLI, a
  packet capture or the chaos harness, and another client means reading
  [docs/PROTOCOL.md](docs/PROTOCOL.md) and linking a TLS stack.

- **It is a smaller thing to build and audit.** One crate on Tauri 1 and Salvo,
  against a workspace that links QUIC, rustls, prost and BLAKE3 and asks for
  Rust 1.85 with edition 2024. Both the clean build and the dependency surface
  `cargo deny` reviews are larger now.

- **It has more miles on it, and less ceremony.** 0.1.x had seventeen
  releases behind it, while 0.2.0 is an alpha with documented limits in
  [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md). Pairing is also stricter:
  reinstalling a device changes its certificate, so it has to be paired again
  instead of quietly reconnecting.

The 0.1.x line is not deleted. It is kept on the `0.1.X` branch of
[fluxy-rs/fluxy-0.2](https://github.com/fluxy-rs/fluxy-0.2), with its releases
still online, for anyone who needs the browser workflow. This repository's
`main` is 0.2, and development continues here.

## Documentation

| Document | What it covers |
| --- | --- |
| [User guide](docs/USER_GUIDE.md) | Installing, pairing, chat, clipboard, transfers, settings, troubleshooting |
| [Known issues](docs/KNOWN_ISSUES.md) | What this build does not do yet, with workarounds |
| [Packaging](docs/PACKAGING.md) | Installers, signing, clean-machine checklist |
| [Release matrix](docs/RELEASE_MATRIX.md) | Which check is verified on which platform, and how to reproduce it |
| [Release checklist](docs/RELEASE_CHECKLIST.md) | The gate list for tagging a release |
| [Release notes 0.2.0](docs/RELEASE_NOTES_0.2.0.md) | What shipped and what to expect |
| [Firewall guide](docs/FIREWALL.md) | Per-platform rules and the usual connectivity failures |
| [Security model](docs/SECURITY.md) | Threat model, controls, and the verification evidence |
| [Security audit](docs/SECURITY_AUDIT.md) | Dependency policy, advisory status, unsafe inventory, fuzzing |
| [Testing](docs/TESTING.md) | Every suite, what it proves, and how to run it |
| [Implementation status](docs/IMPLEMENTATION_STATUS.md) | Milestone-by-milestone record with test counts |
| [Protocol](docs/PROTOCOL.md) | Frames, handshake, transfer negotiation, chunk format |
| [Design](docs/DESIGN.md) | Product and architecture decisions |
| [Development plan](docs/DEVELOPMENT_PLAN.md) | Milestones, estimates, and acceptance criteria |
| [Mobile plan](docs/MOBILE_PLAN.md) | The Android track |
| [OpenHarmony plan](docs/OPENHARMONY_PLAN.md) | The HarmonyOS NEXT track (experimental, branch-gated) |

A release candidate is soaked with `bun run soak --minutes 60`, which pairs the CLI
once and then loops chat plus a hashed file transfer, recording every round in
`.soak/log.jsonl`.

## Requirements

- Rust stable (edition 2024, MSRV 1.85)
- Bun 1.4+
- Tauri 2 system prerequisites for your OS

## Commands

```bash
bun install
bun run build          # frontend production build
bun run tauri:dev      # desktop dev shell
bun run tauri:build    # desktop bundle
bun run check          # TypeScript contract check
bun run check:security # CSP/capability/sink regression check
bun run test:e2e       # cross-platform CLI acceptance (pair, chat, clipboard, restart)

cargo check --workspace
cargo test --workspace
cargo run -p fluxy-cli -- status --data-dir ./.fluxy-dev
```

The CLI is the reference consumer of the engine during the early milestones.
It supports `id`, `status`, `config`, `discover`, `interfaces`, `serve`, `pair`,
`chat`, `clipboard`, and the legacy combined `connect` command.

## Discovery

A non-loopback listener advertises `_fluxy._udp.local.` over mDNS and prints
`discovery=mdns`. A second machine on the same LAN can browse for it:

```bash
cargo run -p fluxy-cli -- serve --listen 0.0.0.0:0 --data-dir ./.fluxy-a
# listen=0.0.0.0:xxxxx
# device=<server device UUID>
# fingerprint=<64 hex characters>
# discovery=mdns service=_fluxy._udp.local.

cargo run -p fluxy-cli -- discover --timeout 3
# peer device=<server device UUID> name=... platform=... addr=... port=xxxxx version=1
```

Use `--interface <name>` on `serve` or `discover` to restrict multicast to one
interface, and `--json` on `discover` for scripted output; JSON includes the
certificate fingerprint as a pin hint. Discovery records are hints only; `pair`
is still required and the QUIC/TLS handshake remains authoritative.

Omitting `--listen` on `serve` hands the choice to the interface selection:
`--interface` (with an optional `--address`) pins one adapter and address, and
otherwise the automatic pick takes the interface that carries the default route,
prefers a real adapter over a VPN or hypervisor one, and re-checks every two
seconds so a moved cable or a newly joined Wi-Fi is followed. The listener socket
is pinned to the chosen interface where the platform supports it (Windows
`IP_UNICAST_IF`, macOS `IP_BOUND_IF`, Linux `SO_BINDTODEVICE`), and the serve
line prints `selection=`, `listen=`, and `interface_pinned=` so a script can see
what happened:

```bash
cargo run -p fluxy-cli -- interfaces          # every adapter, with the automatic pick marked
cargo run -p fluxy-cli -- serve --interface "Wi-Fi" --data-dir ./.fluxy-a
cargo run -p fluxy-cli -- serve --address 192.168.1.20 --data-dir ./.fluxy-a
```

In the desktop app the same choice is a persisted preference (`network` in
`config.json`) and **Settings → Network interface** drives it through
`set_network_selection`. The panel shows the active interface, why it was chosen,
whether the socket is pinned, and how often a network change has rebound the
listener, then lists every adapter with its kind, index, addresses, and the
recommended/default-route/virtual marks. Switching mode or adapter asks for
confirmation first, because it drops current sessions and pauses transfers in
flight (they resume from their checkpoint). A manual pick that disappears is
reported as error 2003 with the recovery steps - choose another interface, or
plug the network back in - and the app never downgrades to a different address on
its own. A copy button writes the interface list and addresses as text for
support.

Loopback is never advertised. A listener on a loopback address prints
`discovery=disabled-loopback`; direct loopback pairing remains available only
through the explicit `--dev-loopback` flag.

## File access

The transfer layer never sees platform paths. `fluxy-core::files` defines
`FileHandleId`, `FileSource`, `FileSink`, and `FileAccessManager`; the desktop
implementation registers `FileReference::Path` sources and writes received
files to receiver-root temp files that only become visible after every byte is
covered. Boundary tests:

~~~text
cargo test -p fluxy-core files
~~~

`fluxy-core::transfer` adds the manifest model used before a transfer: a
parallel BLAKE3 window pass produces per-block hashes, the file root hash, and a
manifest digest, and an identity-bound on-disk cache skips re-hashing when the
source is unchanged. Run the focused tests and the ignored 10 GiB gate with:

~~~text
cargo test -p fluxy-core manifest
FLUXY_BENCH_FILE=/path/to/10gib.bin cargo test -p fluxy-core --release --lib benchmark_10gib_prehash_under_15s -- --ignored --nocapture
~~~

The receiver then pulls the file with a bounded window: `fluxy-core::transfer`
owns the chunk bitmap and the pull scheduler (16 outstanding requests, 15 s
retry deadline, 3 attempts), and `fluxy-net::transfer` serves each request on
the transfer's own stream as an `FCH1` frame and hash-verifies every chunk
against the manifest before writing it. Each transfer names its stream with its
transfer id before anything else, so several files can move over one connection
without swapping streams. Loopback tests exercise concurrent transfers,
out-of-order duplicates, repeated transfers on one connection, and the
missing-chunk failure path:

~~~text
cargo test -p fluxy-core transfer
cargo test -p fluxy-net transfer
~~~

Resume state is stored per transfer under `checkpoints/<transfer-id>/`: a
binary `manifest.bin` and a checksummed `progress.bin` holding the role, state,
source identity, and received-block bitmap. Both are written through an fsynced
temporary and an atomic rename, and the receiver reopens the deterministic
`.fluxy-<transfer-id>.part` file through `resume_sink` so only missing chunks are
pulled. A torn or mismatched checkpoint restarts the transfer instead of
stitching two content versions:

~~~text
cargo test -p fluxy-core checkpoint
cargo test -p fluxy-net transfer::tests::sender_crash
cargo test -p fluxy-net transfer::tests::receiver_crash
cargo test -p fluxy-net transfer::tests::receiver_task_abort
~~~

A transfer can also be paused, cancelled, or retried. `fluxy-core::transfer`
owns the enforced lifecycle (`StateMachine`) and a cloneable `TransferControl`
handle; the receive loop polls it every 100 ms, so a pause freezes the request
window without consuming retries, and a cancel returns 3008 `Canceled`, moves
the state to `cancelled`, discards the `.part` file, and removes the checkpoint:

~~~text
cargo test -p fluxy-core state
cargo test -p fluxy-core control
cargo test -p fluxy-net transfer::tests::pause_then_resume
cargo test -p fluxy-net transfer::tests::cancel_from_paused
~~~

A completed pull is verified before it becomes visible. Every chunk is checked
against its payload hash and the manifest block hash, and a mismatch is rejected
by block index and retried instead of written. Once the last block lands, the
receiver streams a BLAKE3 hash over the partial file and compares it with
`manifest.root_hash`; only a match moves the state to `completed` and renames
`.fluxy-<transfer-id>.part` into place. A mismatch fails with 3005
`HashMismatch`, drops the checkpoint, and discards the partial file, and the
sender re-stats the source before each request so a file changed mid-transfer
stops the transfer with 3002:

~~~text
cargo test -p fluxy-core files::desktop::tests::root_hash
cargo test -p fluxy-core scheduler::tests::a_rejected_chunk
cargo test -p fluxy-net transfer::tests::a_tampered_chunk
cargo test -p fluxy-net transfer::tests::a_source_changed
cargo test -p fluxy-net transfer::tests::a_root_hash_mismatch
~~~

The transfer center (`/transfers`) is the user-facing side of all of this. It
sends a file through the native picker and a trusted-device chooser, prompts for
an incoming offer before anything is written, and shows per-task progress, size,
speed, and remaining time, with pause, resume, cancel, and retry. Progress is
throttled in the backend to at most one update per 100 ms per transfer, so a
fast transfer cannot flood the UI, and terminal events are never dropped.

Every task has a **Details** dialog: state, direction, peer, progress, speed and
remaining time, creation and update clock, and — for a failure — a localized
reason, the protocol code and its number, whether the core considers the failure
retryable, and the raw message. Reasons are resolved from the stable error codes
(`src/lib/transfer-errors.ts`); a code from a newer peer degrades to a neutral
message instead of a raw key. Retry is offered where the core says the failure
can be repeated (a changed source or a hash mismatch asks for a new send
instead) and works straight from the dialog. Copying the details yields the same
fields as text, and the filesystem path never leaves Rust:

~~~text
cargo test -p fluxy transfer::tests
bun run test:frontend
~~~

Files can also be dropped straight onto the window or picked in one batch. Every
file becomes its own transfer, so one failure never blocks the rest. At most 100
files are queued per send, and the number of transfers running at the same time
is capped (default 2, adjustable from the transfer center) — everything else
waits in the queue, and a paused transfer keeps its slot. A device that is
already at its cap answers a new offer with 2002 `PeerBusy` instead of starting
a transfer it cannot serve. The drop itself is handled in Rust, so the WebView
receives opaque handles and base names rather than local paths:

~~~text
cargo test -p fluxy transfer::tests
cargo test -p fluxy-app
bun run test:frontend
~~~

Path, overwrite, and space policy hardening lands in M2-10.

## Local development

Two CLI instances can pair over loopback. Start the listener, copy the printed
device id, address, and fingerprint, then pair from the second terminal:

```bash
cargo run -p fluxy-cli -- serve --listen 127.0.0.1:0 --dev-loopback --data-dir ./.fluxy-a
# listen=127.0.0.1:xxxxx
# device=<server device UUID>
# fingerprint=<64 hex characters>

cargo run -p fluxy-cli -- pair 127.0.0.1:xxxxx \
  --fingerprint <64 hex characters> --dev-loopback --data-dir ./.fluxy-b
```

Both sides print the same six-digit SAS and wait for `yes` on stdin. The
certificate is pinned by fingerprint, and both trust stores are written only
after the SAS is accepted. Once paired, send chat in a later process without
another SAS prompt:

```bash
cargo run -p fluxy-cli -- chat 127.0.0.1:xxxxx \
  --device <server device UUID> --message "hello" --dev-loopback --data-dir ./.fluxy-b
```

`chat` sends UTF-8 text (at most 32 KiB) over the control stream and waits for
the application-level `MESSAGE_ACK`. The legacy `connect` command combines
pairing or resume with an optional `--message` for compatibility.

Clipboard sharing is a two-step exchange. The sender offers a preview, size, and
BLAKE3 digest; the receiver must accept before the text is sent:

```bash
cargo run -p fluxy-cli -- clipboard 127.0.0.1:xxxxx \
  --device <server device UUID> --text "hello" --dev-loopback --data-dir ./.fluxy-b
```

`clipboard` waits up to 90 seconds for the receiver's decision, then prints
`clipboard-applied` only after the final ACK. The text is capped at 1 MiB and is
never written to disk or logs. A headless `serve` has no consent UI and rejects
offers with `cli_no_consent`.

File transfer is a receiver-driven pull over the same session. `transfer-send`
offers one file, `transfer-recv` accepts it and reports the final root hash:

```bash
cargo run -p fluxy-cli -- transfer-recv --listen 127.0.0.1:0 --dev-loopback --data-dir ./.fluxy-a
cargo run -p fluxy-cli -- transfer-send 127.0.0.1:xxxxx \
  --device <server device UUID> --file ./payload.bin --dev-loopback --data-dir ./.fluxy-b
```

Both print `FLUXY_XFER key=value` lines so a script can follow progress and read
`root_hash`, `bytes_written`, and `resumed`. A retry after an interrupted run
reuses the checkpoint when the same `--transfer-id` is passed; `transfer-recv`
takes the id from the offer.

Pass `--once` to `serve` when a single connection is enough for a scripted
test. TLS certificate/key material is stored under the data directory, so the
server fingerprint and the client certificate pin stay stable across restarts.
Chat is in-memory only; no conversation is written to disk.

Loopback binding and targets require an explicit `--dev-loopback` flag; loopback
is never advertised over mDNS and is not a production network mode.

## Desktop shell

`bun run tauri:dev` starts the Tauri shell and the Solid frontend. The shell uses
a hash router so a packaged window can reload any route without a server
fallback: `/` (devices and session), `/transfers`, `/clipboard`, and
`/settings`. The layout is a dark instrument rail, a ruled workspace, and a
status bar on desktop; at 768px and below the rail collapses to a bottom
navigation.

The theme follows the system by default and can be pinned to light or dark from
the top bar or Settings. Locale defaults to `zh-CN` and can switch to English;
the document `lang` and window title follow the choice. Interface copy lives
only in `src/i18n`, and a catalog test fails the build when the two locales
drift, when a placeholder disappears from a translation, or when a component
hardcodes copy. Settings also edits the device name (1-64 characters; saving
restarts the LAN advertisement so other devices see the new name right away)
and turns desktop notifications on or off. The main flow is reachable with the
keyboard alone: a skip link jumps to the content, the rail, pairing dialog,
offer dialog, and composer are native controls, dialogs trap focus and close on
Escape, and closing a dialog returns focus to the control that opened it. All
route states share the same empty, loading, and error primitives, and
`prefers-reduced-motion` turns transitions off.

Nearby devices appear automatically on the rail and home page. Pairing opens a
dialog with both device names, the same six-digit SAS on each screen, a live
120-second countdown, and an explicit warning to compare both screens. The
confirm button stays disabled until the acknowledgement checkbox is ticked;
Fluxy never auto-confirms. There is no QR or manual-address entry. Successful
pairing writes trust, and Settings can remove a trusted device or show discovery
diagnostics (interface, listener, mDNS state, and nearby count).

Discovery prefers mDNS. When a phone or Windows hotspot filters multicast, the
desktop also sends a small UDP announcement to the limited broadcast address and
each directed subnet broadcast, and probes the default gateway by unicast. A
peer found this way is still only a hint: the normal TLS certificate pin and
six-digit SAS confirmation decide trust. Settings shows whether the fallback is
active, the gateway and targets, probe/reply counters, and fallback-only peers.

The home view also contains the memory-only chat workspace: a conversation list
of trusted devices, a message stream, a composer, and per-message delivery
state. Sending opens a trusted resume with the selected peer; an inbound trusted
resume becomes a chat session. A message stays **Sending** until the peer's
application ACK arrives, becomes **Failed** with a retry action if the session
ends first, and a retry reuses the original message id so the receiver does not
deliver it twice. The transcript is capped at 1000 messages, is never persisted,
and is cleared on exit or identity rotation.

The clipboard route lists online trusted peers and sends the current system
clipboard only after **Send current clipboard** is clicked. The sender reads the
clipboard at that moment, sends a preview/size/digest offer, and sends the text
only after the receiver accepts. Incoming offers show the peer, preview, size,
and a countdown; nothing is written to the system clipboard until **Accept**.
Accepted text is verified against the offer digest, and the history keeps only
in-memory previews and status. Each trusted device has a persisted receive
policy: **Ask** (default) shows the incoming queue, **Auto** requires an
explicit per-device selection and writes without a prompt, and **Reject**
declines silently.

**Auto send** is the other, separate switch and is off by default. When it is
on, the backend reads the system clipboard every 300 ms and offers a value only
after it has looked the same for a full debounce window, so a half-written
clipboard or a burst of copies produces one offer instead of many. Offers go to
every online trusted device and the receiver still applies its own receive
policy. The loop rules are what make it safe to leave on: offers carry an origin
id and a hop count, a receiver never forwards, and a digest that the local
clipboard applied from a peer is never offered back - so a device that just
received text does not echo it to the sender even though it is now on its
clipboard. The same content is never auto-sent twice, only text is synced, and
the 1 MiB cap still applies. Turning the switch off stops all clipboard reads,
and turning it on records whatever is already on the clipboard instead of
sending it, so enabling never broadcasts a password copied a moment earlier.

## Install

Release builds ship an installer per platform; the Windows build produces
NSIS and MSI packages, macOS a `.dmg`, Linux an AppImage and a deb. The
Windows installer is per-user (no elevation) and installs in English or
Simplified Chinese.

~~~text
bun x tauri build --bundles nsis     # target/release/bundle/nsis/*.exe
pwsh -NoProfile -File scripts/package/windows.ps1
~~~

Packages are unsigned by default; signing (Windows certificate, macOS
notarization) and the clean-machine install checklist are in
[docs/PACKAGING.md](docs/PACKAGING.md). Uninstalling removes the program and the
tray entry but keeps `%APPDATA%/com.thep0y.fluxy`, so reinstalling does not
orphan the device identity or its pairings.

## Platforms

Fluxy targets Windows, macOS, and Linux. The CI matrix runs format, lint, the
full test suite, the decoder fuzz harness, and the CLI acceptance scenario on all
three, and `tauri build --no-bundle` on demand;
[docs/RELEASE_MATRIX.md](docs/RELEASE_MATRIX.md) records which row is verified
where and which ones still need a host of each platform.

## Security checks

`cargo deny` enforces the license allow list, a crates.io-only source rule, and a
ban on wildcard dependencies (`deny.toml`); the decoder fuzz harness runs on
every `cargo test` and can be pushed to millions of mutations; and
`bun run audit` runs both plus an inventory of the five `unsafe` blocks
in the workspace. The results, the two license decisions, and the known items -
including the advisories check that needs network access - are in
[docs/SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md).

## Performance

Transfers are measured, not estimated. The repeatable benchmark moves 256 MiB
through two endpoints over loopback on one transfer stream and writes the
received file to disk:

```bash
cargo test -p fluxy-net --release --lib benchmark_loopback_throughput -- --ignored --nocapture
```

It reports roughly 240-265 MiB/s on a desktop CPU, which comfortably covers a
1 GbE link (105 MB/s) but is below the plan's 1.0 GiB/s loopback goal: the limit
is one UDP socket's packet rate and the per-chunk control round trip, not
bandwidth or copying, so closing that gap needs multiple sockets or GSO/GRO
rather than tuning. Stream count, window size, MTU, socket buffers, and
per-chunk copies were all swept and none of them moved the number by more than
noise; docs/TESTING.md records the measurements.

A folder can be sent as a batch: drag it onto the window, or use **Send folder**
on the transfer page, and the confirmation names the root, the file count, the empty-directory
count, and the total size before anything moves. The receiver rebuilds the tree
under its own receive directory - empty folders included - and refuses any
relative path that would escape it, is absolute, contains a traversal segment, a
control or bidirectional character, an alternate data stream, a reserved device
name, a trailing dot, an empty segment, or a case-only duplicate. Every file in
the batch stays an ordinary transfer, so it has its own progress, checkpoint, and
retry, while the transfer page shows one collapsible row per batch with
batch-wide pause, resume, and cancel. Symbolic links are refused rather than
followed, and a batch is capped at 10,000 entries.

A native tray icon restores a hidden or minimized window; its menu - **Open
Fluxy**, the **Auto send clipboard** and **Start at login** switches, **Help and
firewall**, and **Quit** - follows the selected UI locale, and the two check items
mirror the persisted preferences. Closing the window no longer ends the process:
the close request is prevented and the window is hidden, so chat, clipboard
offers, and transfers keep arriving in the tray, and only **Quit** stops the
listener. **Start at login** registers a per-user login item (Windows Run value,
macOS launchd plist, Linux XDG autostart entry) that launches the app with
`--hidden`, so a login start lands in the tray; the app repairs the item on the
next start if something removed it while the preference was on. When the window
is unfocused, incoming
chat messages (and pending clipboard offers under **Ask**, or applied clipboard
under **Auto**) raise an OS notification containing only the peer name and event
type, never the message or clipboard text. A finished or failed transfer raises
one too, naming the file and the peer; a user cancel stays silent, and a bulk
send that finishes at once coalesces to a single notification per device instead
of one per file. Settings can mute all of them. Clicking the notification or the tray
icon restores and focuses the window. On Windows, native toast delivery requires
an installed build; development builds may show PowerShell branding.

Because the app is LAN-only, the operating system firewall has to allow its
inbound UDP traffic: discovery uses a fixed UDP port and transfers use the port
the listener binds (system-assigned by default). The in-app **Help and firewall**
page and [docs/FIREWALL.md](docs/FIREWALL.md) list the per-platform rules, show
the ports the running instance actually uses, and cover the usual "devices see
each other but never connect" cases.

The Rust adapter exposes `engine_status`, `state_snapshot`, `events_subscribe`,
`identity_rotate`, `pair_start`, `pair_confirm`, `pair_cancel`, `trust_remove`,
`clipboard_set_policy`, `chat_open`, `chat_send`, `chat_retry`,
`clipboard_share`, `clipboard_accept`, `clipboard_reject`, `set_auto_clipboard`,
`set_autostart`, `set_network_selection`, `pick_folder`, `start_folder`,
`shell_set_labels`,
`discovery_diagnostics`, `pick_file`, `pick_files`, `register_file`,
`start_transfer`, `start_transfers`, `set_transfer_concurrency`,
`set_device_name`, `set_notifications`, `transfer_respond`,
`transfer_control`, and `transfers_list` through tauri-specta. The UI first loads a snapshot
and then subscribes to one event Channel; each event carries a process-wide
`seq` and a `snapshot_version`, and a gap triggers another `state_snapshot`
instead of an incremental guess.

The generated contract lives in `src/bindings.ts` and should not be edited by
hand. Regenerate it after changing a command or IPC type:

```powershell
$env:FLUXY_EXPORT_BINDINGS='1'; cargo run -p fluxy
```

Frontend component tests run under Vitest and jsdom with a typed mock engine
context, so they do not need Tauri IPC: `bun run test:frontend`. `bun run test`
runs the Rust workspace suite and then the component suite. The M1-09
acceptance scripts in `scripts/e2e/` run the pairing, chat, clipboard-consent,
restart/resume, and rejection path on Windows, Linux, and macOS; see
[scripts/e2e/README.md](scripts/e2e/README.md).

## Security

The M0 threat model, high-risk decisions, and release gates are in
[docs/SECURITY.md](docs/SECURITY.md). The review record is
[docs/security/M0_REVIEW.md](docs/security/M0_REVIEW.md). The M1-03 shell runs
under a strict production CSP, uses only `core:default` in its main-window
capability, has no `opener` plugin, and enables `freezePrototype`;
`bun run check:security` guards those settings. mDNS certificate fingerprints
are pin hints only; all first-pair trust still comes from the two-screen SAS
comparison. The CLI and shell are still a
development prototype: OS keystore-backed keys, global connection/message
limits, and file path defenses remain release gates.

## Fault injection

`tools/udp-chaos` forwards UDP through a lossy link so recovery can be tested
without unplugging a network. It supports packet loss, fixed delay, jitter,
duplication, reordering, and a one-shot outage:

```bash
cargo run -p udp-chaos -- \
  --listen 127.0.0.1:42430 \
  --target 127.0.0.1:42420 \
  --loss 0.05 \
  --delay-ms 20 \
  --outage-after-ms 3000 \
  --outage-duration-ms 500
```

Point `pair` and `chat` at the proxy address while the server keeps its real
address and fingerprint. The proxy never creates trust and cannot bypass
TLS/SAS. See [docs/TESTING.md](docs/TESTING.md) for the manual workflow and
`cargo test -p udp-chaos` for the automated two-node recovery suite.

### Acceptance matrix and the 10 GiB benchmark

`scripts/acceptance/` drives the real CLI to cover the plan's T-01..T-06 matrix:
pairing, a local transfer with matching root hashes, a link outage with resume,
a receiver restart with resume, the fault-injection tests, and the benchmark
with a receiver memory ceiling.

```bash
node scripts/acceptance/acceptance.mjs local --size 64MiB
node scripts/acceptance/acceptance.mjs matrix              # T-01..T-06, T-05 manual
node scripts/acceptance/acceptance.mjs bench --size 10GiB --release
```

The two CLI modes behind it are `transfer-send` and `transfer-recv`. T-05
(revoked file access) needs an ACL change and is a documented manual step in
[scripts/acceptance/README.md](scripts/acceptance/README.md).

## License

GNU Affero General Public License v3.0. See [LICENSE](LICENSE).

This is the strictest licence that is still an open-source licence, chosen for
one reason: **a modification has to stay open**. AGPL section 13 extends GPL
section 12 to network use, so running a modified Fluxy as a service and letting
others talk to it counts as distribution - the source has to be offered, under
the same terms.

**Commercial use.** AGPL does not forbid it; forbidding it would make this
project "source-available" rather than open source, because the Open Source
Definition requires that a licence place no restriction on fields of endeavour.
What AGPL does forbid is commercial use that keeps the modifications closed.
For a company that means publishing every change it makes under AGPL on the
same server it runs, which is why in practice nobody takes this code and builds
a closed product on it.

If you need to do that - keep modifications private, or ship Fluxy inside a
closed product - a separate commercial licence is the only route. Open an issue
at <https://github.com/fluxy-rs/fluxy/issues> and describe the use.

**Contributions.** Bug fixes and features belong here rather than in a fork:
a pull request against `main` is easier to review, easier to carry forward,
and keeps everyone on one implementation. See
[AGENTS.md](AGENTS.md#pull-requests-one-purpose-split-into-commits) and
[AGENTS.md](AGENTS.md#commits-conventional-and-saying-why) for the shape a
contribution is expected to take.
