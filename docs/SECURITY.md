# Security model

Status: M0 review complete; M1-02 WebView hardening applied (2026-10-22)  
Scope: v0.2.0 M0 CLI transport and M1 desktop shell  
Review record: [docs/security/M0_REVIEW.md](security/M0_REVIEW.md)  
Normative product design: [docs/DESIGN.md](DESIGN.md)  
Wire format: [docs/PROTOCOL.md](PROTOCOL.md)

This is a living threat model. It describes the current M0 transport and M1
shell, not a claim that the future file-transfer, chat, clipboard, or mobile
surfaces are already secure.
The review is a maintainer self-review; an independent external audit has not
been performed.

## 1. Security goals

- A peer must prove possession of a trusted device identity and the private key
  for its pinned certificate before it can send chat or clipboard data.
- The first pairing must require a human comparison of a short authentication
  string (SAS) on both devices. Discovery metadata must never create trust.
- All traffic must use authenticated QUIC/TLS 1.3. There is no
  unauthenticated plaintext mode and no "skip certificate verification"
  option.
- Chat content must not be persisted by the base product. Logs must not contain
  message bodies or file contents.
- The receiver must authorize clipboard and file operations. Automatic
  clipboard is off by default and is scoped per trusted device.
- A forged certificate, a changed client certificate, an unpaired peer, and a
  bad SAS must be rejected without writing trust state.
- The implementation must fail closed on malformed frames, unknown protocol
  versions, oversized input, and path traversal attempts.
- Loopback is a development-only transport. It must be explicitly enabled, must
  not be advertised over mDNS, and must not weaken TLS or SAS checks.

## 2. Non-goals for M0

- Protecting against a compromised operating system, kernel, or admin account.
- Hiding traffic metadata from a local network observer. An observer can see
  that two devices exchange QUIC datagrams, their addresses, and approximate
  timing. TLS 1.3 protects content, not all metadata.
- Anonymous or account-based communication. Fluxy has no account, cloud relay,
  or central identity service.
- File transfer, clipboard, and the Tauri chat UI. Their threats are listed here
  so they are not lost, but implementation and acceptance belong to M1-M3.
- Independent cryptographic review of QUIC, rustls, Ed25519, BLAKE3, or the
  operating system random source.

## 3. Assets

| Asset | Where it lives | Why it matters |
| --- | --- | --- |
| Device identity seed | `identity.json` in the app data directory | Signs Hello/HelloAck transcripts and proves the stable device ID |
| TLS private key | `tls-key.der` in the app data directory | Proves possession of the certificate pinned by peers |
| Trust pins | `trust.json` | Decide whether a device ID, public key, and certificate fingerprint may resume |
| Pairing SAS | Memory and the two user interfaces | The only human trust anchor during first pairing |
| Pairing transcript | Memory | Binds both identities, nonces, certificates, and the SAS decision |
| Chat plaintext | Memory and the current CLI stdout | Must be delivered only inside an authenticated session |
| Discovery records | mDNS cache and UI | Hints for finding candidates; never authority for identity |
| Future clipboard text | Memory and OS clipboard | May contain passwords, tokens, or personal data |
| Future files and paths | Sender file source, receiver destination | Path traversal, overwrite, and disclosure risk |
| Config and logs | App data directory | Can leak names, paths, and operational metadata if mishandled |

The base product intentionally does not persist chat history. M0 stores only the
identity, TLS key material, trust pins, config, and support logs.

## 4. Trust boundaries

1. **LAN to QUIC endpoint.** Anyone on the network can send mDNS records and
   UDP datagrams. The QUIC/TLS endpoint and the application handshake are the
   first enforceable boundary.
2. **TLS handshake to application trust.** The bootstrap server accepts a
   well-formed client certificate at the TLS layer. The signed Hello/HelloAck
   exchange and SAS confirmation bind that certificate to a device identity.
   No control stream may be opened before that application check succeeds.
3. **Trusted peer to local application state.** A resumed peer must match the
   stored device ID, Ed25519 public key, and certificate fingerprint. A paired
   device is trusted for the capabilities the user granted; it is not trusted
   to write arbitrary local paths.
4. **Rust core to Tauri WebView.** Rust owns identity, trust, network, and file
   state. The WebView should receive only rendering snapshots and explicit
   commands, never raw private keys or unrestricted filesystem access.
5. **Local process to app data.** The OS user account is the current boundary
   for identity seeds and TLS keys. Unix files use mode 0600; Windows relies on
   the user-profile ACL until an OS keystore integration is added.
6. **Future file source/sink boundary.** A remote filename is untrusted input.
   The receiver, not the sender, chooses the destination and authorizes writes.

## 5. Assumptions

- The operating system, its random number generator, and the installed
  `rustls`, `quinn`, `ed25519-dalek`, and `rcgen` code are not compromised.
- The user compares the SAS on both devices and does not confirm a mismatch.
- A local administrator or a process with the user's file access can read the
  app data directory; local compromise is out of scope.
- The LAN is not trusted. A hostile peer can spoof mDNS, replay datagrams, and
  flood UDP sockets.
- Wall-clock time is not trustworthy. Protocol decisions use nonces, IDs, and
  local monotonic timers rather than timestamps.
- The v0.2.0 product is a one-to-one CLI prototype. File, clipboard, and mobile
  assumptions will be reviewed again when those features are implemented.

### 5.1 Actors

| Actor | Capability | Relevant boundary |
| --- | --- | --- |
| Unpaired LAN peer | Can spoof mDNS, open QUIC connections, send malformed frames, and attempt pairing. | Must never reach a control session without SAS-backed trust. |
| Malicious paired peer | Has a valid trusted identity and can send chat or request user-authorized operations. | Trust is per device and per capability; it is not permission to write arbitrary paths or run code. |
| On-path network attacker | Can observe, drop, delay, duplicate, and reorder UDP datagrams; may relay a first pairing. | TLS 1.3 protects content; the SAS comparison is the first-pair authority. |
| Local unprivileged process | Can read files available to the same OS user and bind loopback ports. | App data permissions and loopback's explicit flag are the M0 controls. |
| Local administrator / physical attacker | Can read process memory, replace binaries, or modify app data. | Explicitly out of scope; an OS keystore would not stop this actor. |
| Supply-chain attacker | Can compromise a dependency or build artifact. | Cargo.lock, review, and the M5 audit gate are the current response. |

## 6. Threat model

Severity reflects the current v0.2.0 CLI scope and the planned product surface.
"A" means accepted for M0, "M" mitigated in current code, and "G" has a release
gate before a later surface ships.

| ID | STRIDE | Threat and impact | Current controls | Residual / decision |
| --- | --- | --- | --- | --- |
| S-01 | Spoofing | A hostile mDNS record points at an attacker endpoint. | Discovery is a hint only. The client pins the server certificate, and first pairing still requires SAS. | M: wrong endpoint fails the pin, unless the user pairs the attacker. |
| S-02 | Spoofing / Tampering | A LAN attacker relays the first pairing and shows different SAS values. | Both sides derive the SAS from both device IDs, nonces, and certificate fingerprints. Trust is written only after both confirm. | A/G: six-digit SAS and user discipline are the v1 trust anchor; see R-02. |
| S-03 | Elevation | An unpaired device opens a control stream or sends chat. | The server runs the signed Hello/SAS handshake before `ControlSession::accept`. Resume requires a matching trust entry. `connect_trusted` refuses a missing pin. | M: tested by untrusted, wrong-SAS, and changed-certificate tests. |
| S-04 | Spoofing | An attacker steals `identity.json` or `tls-key.der`. | Unix key/identity files are mode 0600; keys stay in Rust memory; the frontend never receives them. | A/G: Windows relies on profile ACL and the seed is not encrypted at rest. See R-01. |
| S-05 | Spoofing | A paired device presents a new certificate or public key. | Trust pins include device ID, Ed25519 public key, and certificate fingerprint. Resume rejects any mismatch. Identity rotation changes the ID, invalidates old trust entries, and restarts the QUIC listener plus mDNS advertisement so the new namespace is visible. | M: tested by `resume_rejects_changed_client_certificate` and identity-rotation tests; live rotation restarted discovery at a new listen port. |
| S-06 | Replay | An attacker replays an old Hello, HelloAck, or chat frame. | TLS 1.3 protects the channel; handshake nonces bind each transcript; envelope session ID and source device ID must match. The control layer keeps a bounded seen-ID cache. | M: replay outside the cache can duplicate an old chat if the same trusted session key is compromised; no persistent history exists in M0. |
| S-07 | DoS | Malformed or oversized frames cause allocation, panic, or disconnect. | Protobuf kind checks, 8 MiB control-frame cap, 32 KiB text cap, bounded QUIC stream counts and windows, and decode errors that reject rather than panic. | M: proto and transport malformed-frame tests pass. |
| S-08 | DoS | A peer opens many connections or sends messages faster than the UI can process them. | QUIC stream limits per connection; pairing limiter per source IP; text/frame caps. | A/G: no global connection cap or per-session message rate yet. See R-03. |
| S-09 | Spoofing / DoS | An attacker guesses the six-digit SAS or repeatedly restarts pairing. | Both users must confirm; the desktop prompt has a 120 s window and never auto-confirms; five failed attempts per IP trigger escalating 2-60 s cooldowns. | A: limiter is in-memory and per-IP; add persistence and a global policy before release. The 120 s window is intentional for a deliberate two-screen comparison. |
| S-10 | Tampering | A local attacker rolls back or edits `trust.json`, restoring an old trusted device. | Trust store is loaded from a versioned JSON file with atomic replace; resume still requires matching certificate and public key. | A: local file write is within the user's OS authority; no signature over the trust store in M0. Revisit for shared/managed deployments. |
| S-11 | Information disclosure | mDNS advertises a hostname, platform, version, features, and a TLS certificate fingerprint hint. | TXT fields are bounded and sanitized; the `fp` value is a public BLAKE3 certificate hash and is never treated as authentication evidence; no secret or chat content is advertised; mDNS is LAN-scoped. | A: device names can be personal data. Provide a neutral device name in the UI before release. A fingerprint hint lets a LAN observer confirm a Fluxy device more easily but does not reveal traffic or keys. |
| S-12 | Information disclosure | Chat text or file paths leak into logs or crash output. | M0 logs include version, data directory, and lifecycle events, not message bodies. CLI output intentionally prints chat to the terminal. | M for logs; A for CLI stdout. Future UI must not log message/path content and must redact crash reports. |
| S-13 | Tampering / Elevation | XSS or a compromised WebView calls Tauri commands or opens arbitrary paths. | Rust owns state; the handler exposes status, snapshot, event subscription, identity rotation, pairing start/confirm/cancel, trust removal, and discovery diagnostics, and never hands private keys to the WebView. Pairing commands accept only a device ID or challenge ID; no WebView-supplied address, key, or SAS is accepted. M1-02 adds a restrictive production CSP, a Vite-only `devCsp`, `freezePrototype`, removes the unused `opener` plugin/permission, and limits the main-window capability to `core:default`. `bun run check:security` rejects `innerHTML`/`eval`/`window.open`/direct URL sinks and CSP regressions. | M: live WebView loads the routed shell under CSP with no policy violations; dialog focus is trapped; no peer-controlled HTML is rendered. Residual: `style-src` still permits inline styles for the UI runtime; peer text remains escaped and must stay that way. See R-04. |
| S-14 | Tampering | A compromised crate or build dependency injects code or exfiltrates keys. | Cargo.lock pins resolved versions; no remote code is downloaded at runtime; release builds use LTO and strip. | A/G: no cargo-deny, audit, or reproducible-build gate yet. See R-05. |
| S-15 | Tampering | A sender supplies `../`, an absolute path, ADS, overlong name, or a malicious manifest. | Partial in M2-01: the desktop manager rejects `.`/`..`, path separators, NUL/control characters, overlong and Windows-reserved names, and writes only under the receiver-chosen root. M2-10 adds the full overwrite policy, ADS, symlink/no-follow, and quota rules. | G: must pass T-09 before any file write ships. See R-06. |
| S-16 | DoS / Tampering | Symlink following, sparse-file abuse, or disk exhaustion during receive. | Partial in M2-01: sinks write a receiver-root `.part` temp file, only rename after full byte coverage, and remove the temp on abort or incomplete finalize. Space checks, sparse-file limits, and symlink policy remain M2-10/M3. | G: add `no-follow` opens, link checks, per-task quotas, and free-space checks in M2/M3. |
| S-17 | Spoofing | A hostile hotspot gateway answers a future discovery probe. | Probe results are hints only; the eventual QUIC/TLS/SAS path still applies. M1-10 adds a bounded UDP probe (512-byte cap, 64-byte name, 32-byte platform), validates magic/version/length/port/fingerprint, ignores self and loopback unless explicitly enabled, requires a matching random nonce for unicast responses, rate-limits replies per source, and prefers mDNS when both channels see the same device. | M for spoofed discovery: a forged response can at most make an untrusted peer visible, and TLS pin plus SAS still decide trust. Physical phone-hotspot validation remains a release gate. |
| S-18 | Elevation | Loopback mode is left enabled in production. | Loopback requires `--dev-loopback` or `dev.allowLoopback=true`; it is never advertised; tests use separate identities. | M: loopback still performs the same TLS/SAS checks. A local process can speak loopback only if it can already run on the host. |
| S-19 | Information disclosure | Another local user reads the app data directory or process memory. | OS account isolation; Unix 0600 on key files. | A: local admin/user compromise is out of scope; OS keystore is the next improvement. |
| S-20 | Tampering | A sideloaded Android APK is replaced by a malicious update, or a compromised store build is installed. | Planned v0.3.0 distribution is a signed GitHub APK only; no Play Store/AAB. Android Keystore is planned for identity seeds. | G: M4/M5 release signing, update verification, and Keystore tests. |
| S-21 | DoS | A slow or half-open QUIC peer holds server resources. | QUIC idle/keepalive settings; 120 s pairing-window timeout; one pairing at a time in the desktop runtime and one accepted application session at a time in the CLI loop. | A/G: add connection admission limits and per-IP connection accounting before GUI. |
| S-22 | Spoofing | A user rotates identity but an old peer still treats the old device as trusted, or mDNS keeps advertising the old namespace. | Rotation changes the device ID and writes a new identity; the old ID no longer matches. The desktop runtime restarts the QUIC listener and mDNS advertisement after rotation, and refuses rotation while pairing is active. | M: `rotation_invalidates_trust_and_changes_id` passes; live rotation produced a new device ID and discovery namespace at a new listen port. |
| S-23 | Tampering / DoS | A crafted or incomplete receive leaves a partial file visible, or a sink writes outside the intended file. | Sinks write to a receiver-root temp file, track covered byte ranges, refuse out-of-range or uncovered finalize, remove the temp on abort or incomplete finalize, and refuse to overwrite an existing destination; source reads are bounded by the opened handle size and report a changed source on a short read. | M for the M2-01 boundaries; overwrite, space, and link policy remain M2-10. |
| S-24 | Tampering | A sender or local cache supplies a manifest with wrong block hashes, an oversized block count, or stale cache metadata. | The manifest digest binds the ordered block hashes; block and root verification helpers reject wrong data; the block count is capped at 32768 (4 MiB control frame); cache entries are bound to source identity, block size, and block count and are rejected when the digest, identity, or shape does not match; a cache hit is never a substitute for the final root-hash check in M2-06. | M for M2-02; final root verification and source-change rejection land in M2-06. |
| S-25 | Tampering / DoS | A sender pushes unrequested, duplicated, oversized, or hash-mismatched chunks, or a peer stalls the transfer forever. | The receiver owns the request window (4 streams × 2 = 8 outstanding) and only accepts chunks it requested; every `FCH1` frame is capped at 4 MiB, checked for magic, declared length, transfer id, manifest index, offset, and block hash before the sink write; duplicates are written at most once; a request has a 15 s deadline and a 3-attempt budget, after which the chunk is reported missing and the transfer fails instead of hanging. | M for M2-03; resume checkpoints land in M2-04 and the final root-hash re-check in M2-06. |
| S-26 | Tampering / DoS | A torn, stale, or foreign checkpoint resumes a transfer into the wrong file, or a changed source is silently resumed. | `progress.bin` is framed with a BLAKE3 checksum and both checkpoint files are written through an fsynced temporary and an atomic rename; loading validates magic, version, framing, length, and checksum; resume requires the stored manifest digest, block size, and file size to match the offered manifest and the bitmap length to equal the block count; the sender records its source identity (size and modification time) so a restart can compare it with the live file; any mismatch or corrupt file discards the checkpoint and restarts instead of stitching two content versions. | M for M2-04; the full stat check and 3002 mapping land in M2-06. |
| S-27 | Cleanup / integrity | A cancelled transfer leaves a partial file or checkpoint behind, or a caller forces an illegal state such as completed without verification. | Only the core state machine may move a transfer, and the DESIGN §8.1 transition table rejects illegal moves with 3001; cancel is idempotent from every active state, discards the partial file through `FileSink::discard`, and removes the checkpoint unless the caller opts in; pause requeues the outstanding window without consuming the retry budget and keeps the checkpoint for resume; a completed transfer is terminal; `TransferError` maps cancel to 3008 and missing chunks to 3006. | M for M2-05; the root-hash gate before `completed` lands in M2-06. |
| S-28 | Tampering | A sender serves a block whose bytes do not match the manifest, or the whole file is assembled from mismatched content, and the receiver still finalizes it. | Every chunk frame is checked against both its own payload hash and the manifest block hash; a mismatch is rejected by name and retried without writing, and the transfer fails with 3006 once the budget is spent. After the last block, the receiver hashes the finished partial file with BLAKE3 and compares it with `manifest.root_hash` before `verifying -> completed`; a mismatch fails with 3005, drops the checkpoint, and discards the partial file, so a corrupt transfer is never renamed into place. | M for M2-06; M2-09 adds the physical LAN acceptance run. |
| S-29 | Spoofing / privacy | A peer or a compromised sender learns a local filesystem path, or a transfer center silently starts a receive that overwrites local data. | Control frames carry only `transfer_id`, file metadata, and hashes; a local path or file handle never appears in a frame. On the desktop the native picker runs in Rust and returns an opaque `{file_id, name, size}` handle, so the WebView never sees a path. A receiver records an offer as `offered` and writes nothing until the user accepts in the transfer center, the destination stays inside the receiver-owned root with a validated portable name, and an existing destination is still refused. Progress frames are advisory, so a dropped or forged value cannot change which blocks are written, and progress is throttled to 100 ms per transfer. | M for M2-07; the T-09 path matrix, overwrite/space policy, and the physical LAN run remain M2-09/M2-10. |

| S-30 | Tampering / DoS | A drag-and-drop path leaks a local filesystem path into the WebView, or a peer floods the device with offers until it runs out of tasks, streams, or disk. | A window drop is handled in Rust: the path never enters the WebView, only an opaque `{file_id, name, size}` handle and the base name, and directories are refused. A batch is capped at 100 files per send, and simultaneous active transfers are capped (default 2, configurable 1..8, persisted). A send that cannot start waits as `queued` without holding a stream, and an offer that arrives above the cap is rejected with 2002 `peer_busy` before any file, sink, or checkpoint is created. | M for M2-08; the T-09 path matrix, overwrite/space policy, and a physical drag/LAN run remain M2-09/M2-10. |

## 7. High-risk register and decisions

The M0 review has no unresolved high risk that blocks the CLI prototype. R-04
is closed in M1-02; the remaining high risks have explicit release gates. This
is not a claim that any risk is eliminated.

| ID | Risk | Severity | Decision | Gate / owner |
| --- | --- | --- | --- | --- |
| R-01 | Identity seed and TLS private key are not held in an OS keystore; Windows relies on profile ACL and neither file is encrypted at rest. | High if the host is shared or backups are exposed | Accepted for M0 CLI only. Unix is 0600; keys never go to the WebView. | Before desktop/mobile release: Windows DPAPI/Credential Manager, macOS Keychain, Android Keystore, iOS Keychain. Owner: platform/security; M4/M5. |
| R-02 | Six-digit SAS is the only first-pair MITM defense; a tired or rushed user can confirm an attacker. | High | Accepted as the v1 protocol trust anchor. Both sides show the same SAS, both must confirm, mismatch writes no trust, and the handshake is bounded. M1-03 adds a dedicated dialog with both device names, a 120 s countdown, an explicit "compare both screens" warning, a required acknowledgement checkbox, and a confirm button disabled until acknowledgement. No QR/manual entry and no auto-confirm exist. | Residual risk remains human error; M1-03 satisfies the UI gate. No protocol change without a new SAS design. |
| R-03 | No global connection cap, no per-session message rate limit, and the pairing limiter is in-memory and per-IP. | High availability risk on a hostile LAN | Accepted for the single-user CLI prototype; frame/text/stream caps and per-IP backoff are present. | M1: add admission limits, per-session token bucket, and a global limiter; M5: fuzz and soak. Owner: net. |
| R-04 | Tauri CSP is null and `opener:default` is enabled; a future peer-text XSS could reach commands or local paths. | High once peer content is rendered; Low for the adapter UI | **Mitigated in M1-02; M1-04 renders peer text through Solid interpolation only.** Production CSP has `default-src`/`script-src` self-only, `object-src` none, explicit IPC connect sources, and no `unsafe-eval`; `devCsp` adds only Vite HMR origins. `opener` is removed from the package, Cargo dependency, plugin registration, and capability. The main window gets only `core:default`; the small custom command set remains the effective command surface. `freezePrototype` is on. `bun run check:security` guards the config and scans for HTML/eval/URL sinks. | Done; the inline-style allowance remains, and peer-text escaping is checked in M1-04. |
| R-05 | No dependency vulnerability/license audit or reproducible-build gate. | High supply-chain exposure | Accepted with a locked workspace and pinned toolchain expectations. | M5-03: `cargo-deny`, advisory scan, fuzz budget, and release review. |
| R-06 | File transfer path traversal, symlink, ADS, and disk-exhaustion defenses do not exist yet. | High when file transfer ships | Not exposed in M0. The receiver-owned destination model and validation rules are specified in DESIGN and PROTOCOL. | M2-10/M2/M3: block T-09; no file write until all path cases reject safely. |

### Decision notes

- **R-01 is not downgraded by TLS.** A stolen key lets an attacker impersonate
  the device to peers that still trust it. File permissions and OS account
  isolation are the only M0 controls; an OS keystore is required before the
  product handles real user files or clipboard data.
- **R-02 cannot be fixed by a longer PIN alone.** A six-digit SAS is a usability
  choice backed by mutual confirmation. Raising entropy or adding a QR code is
  a protocol/product change, not a silent M0 patch.
- **R-03 is an availability risk, not a confidentiality break.** The LAN-only
  single-user CLI can tolerate it; the GUI must not accept an unbounded stream
  of connections or messages.
- **R-04 is now mitigated, not just deferred.** M1-02 tested the CSP in both
  the normal Tauri IPC path and Vite HMR. The production policy is strict; the
  dev policy exists only for the local Vite server. The remaining inline-style
  allowance is required by the UI runtime and is not a script-execution path.
- **R-06 has no current attack surface.** The repository contains no file write
  command yet; the review records the gate so it cannot be forgotten.

## 8. Controls present in M0 and the M1-02 shell

- QUIC with ALPN `fluxy/1`, TLS 1.3, mandatory client certificate, and a
  BLAKE3 certificate-fingerprint pin on the client.
- Self-signed certificate and key generated once, stored under the app data
  directory; Unix key/identity files are written with mode 0600.
- Signed Hello/HelloAck application transcripts, fresh 32-byte nonces, stable
  Ed25519 device IDs, and a six-digit SAS derived from both identities, both
  nonces, and both certificate fingerprints.
- Mutual SAS confirmation before either trust store is written. A rejected or
  mismatched SAS leaves both stores empty.
- Trusted resume requires a matching device ID, Ed25519 public key, and
  certificate fingerprint. Certificate changes and untrusted devices are
  rejected.
- mDNS records are hints only; the certificate fingerprint `fp` is a pin hint and no mDNS field creates trust. The desktop runtime binds a private LAN address, never loopback unless explicitly enabled.
- The UDP fallback uses the same hint-only rule. It announces a bounded device ID/name/platform/QUIC port/certificate fingerprint to the limited broadcast address and each directed subnet broadcast, and probes the default gateway by unicast; a response must echo a pending nonce. No key, token, chat, clipboard, or file content is sent, and mDNS records take precedence when both channels find the same device.
- The pairing dialog never auto-confirms; a successful trust write requires the user to acknowledge the warning and press the confirm button. The runtime permits one pairing at a time and enforces a 120 s challenge TTL.
- 8 MiB control-frame cap, 32 KiB chat text cap, 1 MiB clipboard text cap,
  bounded QUIC stream/window values, and a bounded duplicate-message cache.
- In-memory per-IP pairing limiter with escalating cooldowns after five failed
  attempts.
- Loopback is explicit, never advertised, and still uses the production
  TLS/SAS path.
- Chat and clipboard text are memory-only; logs do not record message or
  clipboard bodies. Clipboard offers carry only a bounded preview. The receiver
  writes the OS clipboard only after an explicit accept, or after the user has
  explicitly set that trusted device to Auto; Ask is the default.
- Desktop notifications are emitted only while the main window is unfocused.
  Their title/body contain the event type and a control-stripped, 64-character
  peer display name, never message or clipboard text. Notification clicks and
  tray actions restore the main window. Tray labels are supplied by the trusted
  frontend i18n catalog and are bounded server-side; the notification plugin has
  no frontend capability and is driven only from Rust.
- Tauri exposes only status/snapshot/event subscription, identity rotation,
  pairing start/confirm/cancel, trust removal, discovery diagnostics, and
  clipboard share/accept/reject/read, per-device clipboard policy update, and
  bounded native shell labels;
  private keys and clipboard bodies stay
  in Rust, and pairing never accepts a WebView-supplied address, key, or SAS
  value.
- Production WebView CSP is self-only for scripts and objects, with explicit
  Tauri IPC connect sources; `devCsp` scopes Vite HMR to localhost. The unused
  `opener` plugin and permission are removed, the main window gets only
  `core:default`, and `freezePrototype` is enabled.
- `bun run check:security` fails if the CSP is weakened, `opener` returns, the
  capability grows beyond `core:default`, or the source gains `innerHTML`,
  `eval`, `new Function`, `document.write`, `window.open`, or direct location
  assignment.

## 9. Verification evidence

| Check | Evidence |
| --- | --- |
| Wrong server fingerprint rejected | `crates/fluxy-net/src/tls.rs` test `wrong_fingerprint_is_rejected` |
| Wrong SAS writes no trust | `crates/fluxy-net/src/handshake.rs` tests `initiator_rejection_writes_no_trust`, `user_rejection_writes_no_trust`, and `async_pairing_allows_delayed_human_confirmation` |
| Certificate change on resume rejected | `resume_rejects_changed_client_certificate` |
| Missing trust refused before connecting | `untrusted_server_is_rejected_before_connecting` |
| Identity rotation invalidates trust | `crates/fluxy-app/src/engine.rs` test `rotation_invalidates_trust_and_changes_id` |
| Pairing backoff after repeated failures | `pairing_limiter_blocks_after_repeated_failures` |
| Malformed/oversized frames rejected | `crates/fluxy-proto` frame tests, including 8 MiB/truncation cases |
| Duplicate chat suppression | Bounded seen-ID cache in `ControlSession` and `PersistentSession` |
| Loopback isolation | CLI smoke and `crates/fluxy-net` discovery/loopback tests |
| UDP fallback bounds and spoof resistance | `crates/fluxy-net/src/fallback.rs` tests cover packet round-trip, control stripping, malformed/trailing/zero-port packets, incompatible protocol, netmask broadcast, and a two-instance loopback probe/response with nonce validation; `src-tauri/src/network.rs` test `mdns_is_preferred_over_the_gateway_fallback` covers channel precedence |
| Fault recovery | `tools/udp-chaos/tests/session_recovery.rs` |
| WebView CSP and capability baseline | `bun run check:security`; live `bun run tauri:dev` and a `tauri:build --debug --no-bundle` binary under WebView2, both with no CSP console violations |
| Dialog focus boundary | Live rotation dialog: initial focus, Tab wrap, Escape close, and focus restore to trigger |
| Pairing UI gate | Live app/CLI pairing in both directions: matching SAS, confirmation disabled until acknowledgement, trust only after confirm; settings removal and discovery diagnostics exercised |
| Clipboard consent and integrity | `crates/fluxy-net/src/session.rs` test `clipboard_offer_text_and_ack_round_trip`; fourteen `src-tauri/src/clipboard.rs` tests cover the accept-before-text gate, digest mismatch, rejection, oversized offer, revoke, apply-once, digest deduplication, and the watcher debounce/echo/duplicate rules; `contract` test `clipboard_policy_is_per_device_and_persisted` verifies persistence and the trust event; `chat` test `the_watcher_never_sends_received_content_back` drives the T-10 loop case over a fake clipboard; live CLI-to-app tests exercised auto-apply, manual acceptance, reject, and duplicate rejection, with the system clipboard checked before and after |
| Notification privacy and focus | `src-tauri/src/notify.rs` tests bound and control-strip labels, peer names, and file names, keep the placeholders, mute the whole path, and coalesce a same-device transfer burst to one dispatch; `src-tauri/tests/native_shell.rs` verifies the camel-case label contract; a transfer outcome notification carries the file name and the peer name and never file content, and a user cancel raises none; live minimized-window notification contained no message body and focused-window delivery was suppressed |
| Cross-platform main path | `scripts/e2e/e2e.mjs` local scenario pairs, chats, exercises the clipboard consent rejection, restarts the server to prove trust persistence without a second SAS, rejects an unknown device, and refuses a loopback listener without the dev flag; the CI `e2e` matrix runs it on Ubuntu, macOS, and Windows |
| File access boundaries | `crates/fluxy-core/src/files` tests cover random read offsets and EOF, out-of-range writes, overlapping range coverage, incomplete-finalize cleanup, abort cleanup, existing-destination rejection, missing/directory/unsupported registration, unknown handles, portable name validation, stable error-code mapping, and source-shrink detection |
| Manifest integrity and pre-hash | `crates/fluxy-core/src/transfer/manifest.rs` tests cover reference block/root/digest hashes, multi-window parallel hashing, empty files, invalid block sizes, the 32768-block limit, block and root verification, cache hit/miss, identity and digest tamper rejection, atomic replace, and error mapping; the ignored 10 GiB benchmark measured 7.23 s (1416 MiB/s) on this machine |
| Chunk protocol and scheduler | `crates/fluxy-core/src/transfer/{bitmap,chunk,scheduler}.rs` tests cover bitmap bounds and serialization, frame magic/truncation/length/hash/offset/index rejection, window fill, timeout retries, retry exhaustion, and failure reporting; `crates/fluxy-net/src/transfer.rs` loopback tests transfer 5 MiB over four streams, handle out-of-order duplicates, run three repeated 8 MiB transfers on one connection, reject a tampered manifest, and fail with `MissingChunks` when no chunk arrives |
| Integrity and source change | `crates/fluxy-core/src/transfer/chunk.rs` tests cover the payload/manifest hash checks and the header parse that names a rejected block; `crates/fluxy-core/src/files/desktop.rs` tests the streaming root hash, the empty-file hash, and live stat detection; `crates/fluxy-net/src/transfer.rs` loopback tests reject and retry a tampered-once delivery, fail with 3006 when every delivery is corrupt, return 3002 when the source changes mid-transfer, and reject a wrong root hash with 3005 before finalize while removing the checkpoint and partial file |
| Pause, cancel, and retry | `crates/fluxy-core/src/transfer/{state,control}.rs` tests cover the full lifecycle, rejected transitions, cancel from every active state, idempotent cancel, retry rules, control reversibility, and shared clone state; `crates/fluxy-core/src/files/desktop.rs` tests `discard`; `crates/fluxy-net/src/transfer.rs` loopback tests freeze progress while paused, complete after resume, discard the partial file and checkpoint on cancel from paused, and stop the sender on cancel |
| Resume checkpoints | `crates/fluxy-core/src/transfer/checkpoint.rs` tests cover manifest and progress round-trips, torn/tampered/bad-version rejection, atomic save and replace, list/remove, missing-block plans, manifest-mismatch and corrupt-file degradation, source-identity matching, and covered-range merging; `crates/fluxy-core/src/files/desktop.rs` tests reopen and extend a partial sink; `crates/fluxy-net/src/transfer.rs` loopback tests resume after a sender crash, after a receiver sink failure, and after aborting the receiver task so that only on-disk state carries over, transferring exactly the missing bytes |
| Transfer negotiation and center | `src-tauri/src/transfer.rs` tests cover the 100 ms progress throttle and hub coalescing with injected instants, the EWMA speed meter, the opaque `register_file` handle plus directory/missing-path rejection, failure details and retryability in a summary, unknown-transfer and completed-retry control rejection, pause/resume control state, and the protocol code-name table; `src/pages/TransfersPage.test.tsx` covers progress/speed/ETA rendering, pause/resume/cancel/retry, offer accept and reject, the file-picker send flow, the offline-device guard, the error-details dialog, and the state filter; `src/lib/format.test.ts` covers the byte/rate/percent/ETA formatters |
| Drag, batch, and the transfer cap | `src-tauri/src/transfer.rs` tests cover the cap source and clamping, slot accounting, FIFO queue claiming, the 100-file batch limit, directory rejection in `register_paths`, a ten-file drop, a ten-deep queue that starts two and holds eight, one drag-hover event per transition, and the expiry of an unanswered offer; `crates/fluxy-app` tests cover the persisted cap, the clamp, and a config file without the field; `src/pages/TransfersPage.test.tsx` covers the picked batch, the dropped batch with rejected and skipped files, the drop overlay, the queued row, the active filter, and the concurrency control |
| Settings and desktop notifications | `crates/fluxy-app/src/config.rs` tests bound a device name (control characters stripped, trimmed, 64-character cap, never empty) and default `notifications` for a config written by an earlier version; `crates/fluxy-app/src/engine.rs` and `src-tauri/src/contract.rs` tests cover persistence, the status/snapshot propagation, the notifier mute, and the 4105 refusal while a pairing is pending; `src-tauri/src/notify.rs` tests the mute flag, and the notification bodies still carry only the peer name and the event type |
| Tray menu and login item | `src-tauri/src/startup.rs` tests drive the login item through an injected command runner and a temp directory: the per-user Run value is written and read with `reg.exe`, the plist and desktop entry escape the executable path, and a refused write leaves the preference untouched; `src-tauri/src/contract.rs` tests cover the switch, the self-heal path, the close-hides-until-quit flag, and the shell help request |
| Interface selection and binding | `crates/fluxy-net/src/interface.rs` tests cover the automatic ranking (real adapter before virtual, LAN type before tunnel, default route before private address), the virtual-adapter heuristic on name and platform description, the loopback and tunnel filters, manual validation (missing interface and missing address are separate 2003 failures), link-local scope formatting and parsing, and a real UDP socket pinned to a real adapter. `crates/fluxy-app` tests cover the persisted preference, its serde default, and that an unparsable manual pick is refused without changing the stored value; `src-tauri/src/network.rs` tests cover keep/rebind/lost reconciliation for both modes and the interface list the settings page renders; `src-tauri/src/transfer.rs` covers the pre-rebind pause. |
| Network interface settings | `src/components/NetworkSettings.test.tsx` covers the panel (selection explained, recommended/current/virtual/down marks, two-step switch confirmation, retained proposal plus recovery guidance on failure, lost-address guidance, unpinned note, inert rows in automatic mode, copyable interface list); `src-tauri/src/contract.rs` covers the command's rejection paths. |
| Folder batches | `crates/fluxy-core/src/transfer/batch.rs` tests cover the hostile-path table, duplicates, kind conflicts, the entry cap, and metadata round-trips; `files/folder.rs` covers the real walk (empty directories, Unicode, a link); `src-tauri/src/transfer.rs` covers placement under a fresh numbered root and every rejection; `TransfersPage.test.tsx` covers the grouped row and the batch-wide controls |
| Full suite | `cargo test --workspace` (304 tests in this slice, 2 ignored benchmarks) plus 91 Vitest component tests (including the i18n catalog guard, the dialog keyboard contract, the keyboard-only main flow, the transfer detail/reason tests, and the clipboard auto-send switch) and the M1-09 end-to-end scenario |

The test suite is evidence of specific behaviors, not a proof of security. A
future T-08 matrix run must still exercise forged certificates, revocation,
rotation, and re-pairing against real processes on all desktop platforms.

## 10. DESIGN 14.3 and plan 11.4 checklist

| Requirement | M0 status |
| --- | --- |
| Authenticated QUIC/TLS; trust cannot come from IP or mDNS fields | Pass, including the M1-10 UDP fallback: discovery fields never create trust |
| Receiving a file requires authorization; never auto-execute | Pass for M2-07: an offer is recorded as `offered` and nothing is written until the user accepts in the transfer center, and received files are never executed |
| Bound control messages, text, streams, and tasks | Partial: frame/text/stream bounds exist, and M2-08 caps simultaneous active transfers (default 2, 1..8) and the per-send file count (100); connection admission and message rate remain open |
| Path normalization, traversal defense, symlink/same-name handling | Not exposed yet; gate M2/M3 |
| Least-privilege UI/WebView; no secrets or file content in frontend | Pass for the M1 shell: pairing/trust/diagnostics commands accept only IDs, strict production CSP, no opener, `core:default`-only capability; no secrets or file content in the WebView |
| Automatic clipboard off by default; per-device authorization | Pass: the background watcher is off until the user enables it and stops reading the clipboard when it is switched off; every offer uses the persisted per-device policy, Ask is the default, Auto only follows an explicit user selection for that trusted device, and Reject declines without prompting; the send switch and the receive policy stay separate |
| Clipboard loop and echo prevention | Pass: the watcher never offers a digest the local clipboard applied from a peer and never offers the same content twice, a receiver never forwards, the hop field is capped at 1 on receive, an offer whose origin is this device is refused, and a manual share of recently received content is rejected with 4305 |
| Enabling the watcher sends nothing retroactively | Pass: switching the watcher on records the value already on the clipboard, so only content copied afterwards is offered |
| Closing the window does not stop the shell | Pass: a close request hides the window to the tray (`api.prevent_close()` plus `window.hide()`) and only a deliberate Quit sets the flag that lets the process end, so receiving continues without the window |
| Autostart is a user-owned, per-user login item | Pass: off by default, written only after an explicit switch, stored per user (`HKCU` Run value, `~/Library/LaunchAgents` plist, XDG autostart entry), removable from the same switch, and never installed by the bundle itself |
| The interface list stays local | Pass: the selection is local state only. Interface names, descriptions, indexes, and addresses are shown in the local UI and written to the local log; nothing about them is added to mDNS TXT records, the handshake, or any control frame, and a peer still sees only the advertised service address the OS published |
| Dependencies are auditable and redistribution-safe | Pass with one gap: licenses, sources, and bans are machine-checked and pass (622 packages, no missing license, MPL-2.0 only through unmodified CSS parser files, LGPL arm unused); the RustSec advisories check could not run in this offline environment and is a documented release gate |
| Folder batches cannot write outside the receive root | Pass: the sender only produces relative paths, the receiver validates every one before creating anything (no absolute path, no `..` or `.` segment, no NUL, control, or bidi character, no backslash, no ADS colon, no reserved device name, no trailing dot or space, no empty segment, no duplicate that differs only by case, no file/directory conflict, and no path that resolves outside the root), symbolic links are refused by the scan instead of followed, the entry list is capped at 10,000, and an offer whose size disagrees with its entry list is rejected |
| A batch cannot smuggle content past the manifest | Pass: every sub-transfer keeps its own manifest, block hashes, and checkpoint, so a batch file is verified exactly like a single file; the batch of a resumed transfer has to match its batch id and relative path |
| The network settings page cannot widen the listener | Pass: the panel only proposes a `mode`/`interface_id`/`address` triple through the same command the runtime already validated, the backend re-validates it (unknown mode, unparsable address, missing interface or address, and loopback without the development switch all return 2003), and the frontend never receives a capability it did not have - it renders text from the local interface table and sends strings back. |
| Interface pinning fails closed to a warning | Pass: when the platform has no pin option, or refuses it, the listener still binds the chosen address and reports `interface_pinned: false` instead of pretending the traffic is pinned; a failed pin is logged with the interface name and index |
| Firewall guidance does not widen the attack surface | Pass: the help page and `docs/FIREWALL.md` describe inbound UDP rules for the discovery port and the transfer port only, and the app still requires pairing and a pinned certificate before any chat, clipboard, or file data is accepted |
| Notifications do not leak message bodies | Pass: unfocused-only delivery, peer name plus event type only, bounded/control-stripped native strings, no frontend notification capability |
| Logs redact message bodies and paths | Pass for M0 logs; future path logging must be redacted |
| Unpaired devices cannot send chat/clipboard/files | Pass for chat, clipboard, and files: inbound resume requires trust, the runtime rechecks trust before accepting a channel, and a transfer runs only over a session that chat established with a trusted device |
| Peer chat/clipboard text never reaches an HTML/eval sink | Pass in M1-04/M1-05: Solid text interpolation escapes bodies and previews, logs do not record bodies, and `bun run check:security` scans `src` for HTML/eval/URL sinks |
| Chat is memory-only and bounded | Pass: 1000-message cap, no disk writes, cleared on exit/identity rotation, retry reuses the original id |
| Clipboard is memory-only and consent-gated | Pass: 64 share/32 pending caps, 120s offer TTL, 1 MiB text cap, BLAKE3/size check before write, no body logs; Auto applies only to explicitly authorized devices and digest/origin guards suppress loops |
| Wrong SAS writes no trust | Pass |
| Changed certificate rejected and re-pair prompted | Backend pass; the M1-03 pairing dialog displays the new SAS and requires explicit confirmation |
| Connection count, frame size, message rate, pairing rate bounded | Partial: frame size and pairing rate; connection/message rate pending |
| Identity seed encrypted by keystore on mobile | Not implemented; gate M4/M5 |
| Android fd only from user-selected URI | Not implemented; gate M4 |
| Mobile clipboard only in foreground | Not implemented; gate M3/M4 |

The dependency policy, the advisory status, the unsafe inventory, and the fuzz
campaign are recorded in [docs/SECURITY_AUDIT.md](SECURITY_AUDIT.md), which is the
document to update when a release changes the dependency set. `deny.toml` is the
machine-checked half of that document: `cargo deny check licenses bans sources` runs
on every release, and `cargo deny check advisories` is a release gate that needs
network access.

## 11. Security release gates

- **Before M1 chat UI:** the restrictive CSP, `opener` removal, command
  allowlist, and peer-text escaping review are done (M1-02/M1-04). Global
  connection admission and per-session message limits remain open under R-03
  and must be added before the desktop release.
- **Before clipboard release:** per-device ask/auto/reject policy, explicit
  auto authorization, digest deduplication, and origin/hop echo suppression are
  implemented in M1-06. The global auto-send listener remains off. Foreground-
  only mobile access remains a gate for M3/M4.
- **Before notification/tray release:** keep the notification plugin backend-only,
  emit only peer name plus event type while unfocused, and retain click-to-focus
  behavior. M1-07 implements these controls; a packaged installer should confirm
  toast delivery under the registered app identity.
- **Before file transfer:** T-09 path matrix, receiver-owned destination,
  no-follow opens, symlink/ADS rejection, space checks, and bounded manifests.
  M2-07 lands the offer/consent gate, the receiver-owned destination root, and
  the opaque picker handle that keeps paths out of the WebView; the T-09 path
  matrix, overwrite, and space policy remain M2-10.
- **Before desktop release:** OS keystore or OS-protected key storage on each
  platform, signed installers, log/crash-report redaction, and a dependency
  audit.
- **Before Android beta:** signed GitHub APK only, no Play Store/AAB, Keystore
  seed storage, SAF-only descriptors, foreground-service lifecycle tests, and
  no QR/manual-address entry path.

## 12. Reporting security issues

Do not open a public issue for a suspected vulnerability. Report it privately
to the maintainers with the affected version, platform, reproduction steps, and
impact. The maintainers will acknowledge the report, assess severity, and
coordinate a fix and disclosure. Never include real private keys, chat content,
or personal files in a report; use synthetic fixtures.
