# M0 Security Review Record

Date: 2026-10-21  
Scope: Fluxy v0.2.0 M0 CLI transport prototype  
Status: Complete; high-risk decisions recorded  
Reviewers: Fluxy maintainers (maintainer self-review)  
Independent audit: Not performed

## Inputs reviewed

- [docs/DESIGN.md](../DESIGN.md), especially sections 6, 7, and 14.3
- [docs/DEVELOPMENT_PLAN.md](../DEVELOPMENT_PLAN.md), especially T-08, T-15,
  and section 11.4
- [docs/PROTOCOL.md](../PROTOCOL.md)
- [docs/SECURITY.md](../SECURITY.md)
- Current Rust implementation in `crates/fluxy-core`, `crates/fluxy-net`,
  `crates/fluxy-app`, `crates/fluxy-cli`, and `src-tauri`
- The M0 transport, discovery, chaos, and CLI tests

## Method

1. Enumerated assets, actors, and trust boundaries.
2. Walked STRIDE categories for discovery, pairing, resume, control messages,
   local key storage, WebView isolation, and planned file/clipboard/mobile
   surfaces.
3. Checked each claimed control against code and a test where one exists.
4. Recorded accepted risks separately from missing controls. A missing control
   with a future release gate is not treated as "fixed".
5. Ran the workspace test, lint, formatting, and frontend checks as a baseline.

The review is deliberately scoped to M0. It does not certify future file,
clipboard, Tauri chat, Android, or iOS code.

## Summary

- No open Critical finding.
- Six High risks have explicit decisions: R-01 through R-06 in
  [docs/SECURITY.md](../SECURITY.md).
- R-04 was mitigated in M1-02. The other five have either a release gate or an
  explicit residual-risk decision; R-02 is the accepted residual risk of the v1
  six-digit SAS UX.
- The CLI prototype is acceptable for local/LAN development with synthetic
  data. It is not approved for real sensitive files or clipboard content until
  the gates below are met.

## High-risk findings

| ID | Finding | Decision | Rationale | Owner / gate |
| --- | --- | --- | --- | --- |
| R-01 | Identity seed and TLS private key are plaintext files; Unix uses 0600, Windows relies on the user-profile ACL, no OS keystore. | Accepted for M0 CLI; release-blocking for GUI/mobile | Local OS access is already in the attacker model, but a shared host or backup can expose long-lived credentials. Keys never reach the frontend. | Platform/security; M4/M5 |
| R-02 | Six-digit SAS plus human comparison is the only first-pair MITM defense. | Accepted residual for v1 | The protocol binds both identities, nonces, and certificate fingerprints; both users must confirm; mismatch writes no trust. A user can still confirm an attacker. | M1-03 UI warning; no silent auto-confirm |
| R-03 | No global connection cap, no per-session message rate, and the pairing limiter is in-memory/per-IP. | Accepted for M0 CLI; add before GUI | Frame/text/stream caps and per-IP backoff exist, but a hostile LAN peer can still consume accept/processing capacity or evade a per-IP limit with multiple addresses. | net; M1/M5 |
| R-04 | Tauri CSP is null and `opener:default` is enabled. | **Mitigated in M1-02** (was accepted for the M1-01 adapter only) | Production CSP is self-only for scripts/objects with explicit IPC sources, `devCsp` is limited to Vite HMR, `opener` is removed end to end, the capability is `core:default` only, and `freezePrototype` is enabled. The four narrow commands are unchanged. Live WebView and static `check:security` checks pass. | Done in M1-02; recheck peer-text escaping before M1-04 |
| R-05 | No cargo-deny/advisory/reproducible-build gate. | Accepted with locked dependencies; release gate | A locked `Cargo.lock` limits accidental version drift but does not detect a known-vulnerable crate. | M5-03 |
| R-06 | File transfer path/symlink/ADS/space defenses are not implemented. | Not exposed in M0; blocking for file transfer | No file-write command exists yet. The receiver-owned destination model and path rules are specified but not executable. | M2-10/M2/M3 |

All High findings now have a conclusion and an owner/gate. None blocks the M0
CLI acceptance criteria.

## Medium findings

| ID | Finding | Decision / action |
| --- | --- | --- |
| M-01 | Trust store rollback is possible for a local attacker who can write the app data directory. | Accepted within the local-user threat model; revisit for managed/shared deployments. No trust-store signature in M0. |
| M-02 | Pairing limiter state is lost on restart; the design's 120 s pairing window is not implemented (the handshake has a 10 s timeout). | Accepted for M0; align design/UI and consider persisted throttling in M1. The shorter timeout is stricter, not a security regression. |
| M-03 | mDNS exposes a device name, platform, version, and feature list. | Accepted; these are hints and contain no secrets. Provide a neutral/private device name option before desktop release. |
| M-04 | Duplicate-chat suppression is a bounded in-memory cache; an old message ID could be replayed after eviction. | Accepted for memory-only chat. No persistent history exists. Revisit when persistent sessions/file metadata are added. |
| M-05 | CLI prints chat text to stdout; logs avoid it, but terminal capture is possible. | Accepted for the CLI; document that stdout may contain message content. Future desktop logs and crash reports must redact text and paths. |
| M-06 | Windows key files inherit the user-profile ACL rather than an explicit restrictive ACL. | Accepted for M0; covered by R-01 and the OS keystore gate. |
| M-07 | The discovery probe/hotspot fallback and file-transfer features are not implemented. | Deferred; every future probe field remains a hint and file paths remain untrusted input. |

## Low findings and accepted limitations

- Loopback is explicit, never advertised, and still performs TLS/SAS. A local
  process that can bind loopback can already run code on the host.
- A local administrator can read process memory and app data; this is outside
  the threat model.
- A network observer can see QUIC metadata and traffic timing. Content remains
  protected by TLS 1.3.
- Device rotation intentionally invalidates every old trust entry; the old peer
  must re-pair. This is a security feature, not a bug.
- The application has no account, cloud relay, or chat history, so account
  takeover and server-side retention are out of scope.

## T-08 / T-15 review

| Case | Result | Evidence |
| --- | --- | --- |
| Wrong SAS | Rejected; no trust written | `initiator_rejection_writes_no_trust`, `user_rejection_writes_no_trust` |
| Unpaired peer sends chat | Refused before control session | `untrusted_server_is_rejected_before_connecting`; server handshake path |
| Forged/changed certificate | Refused | `wrong_fingerprint_is_rejected`, `resume_rejects_changed_client_certificate` |
| Removed trust | Refused as untrusted | Missing-pin check in `connect_trusted`; exercised by the untrusted test |
| Identity rotation | New device ID; old trust invalid | `rotation_invalidates_trust_and_changes_id` |
| Loopback default off | Rejected without explicit flag | CLI guard and loopback smoke tests |
| Loopback two-instance pair/chat | Passes with explicit flag | M0-07 CLI smoke; chaos proxy smoke |

Re-pairing after revocation and a user-facing "certificate changed" prompt are
UI/flow work for M1-03, not an M0 backend gap.

## Action register

| Action | Owner | Gate |
| --- | --- | --- |
| A-01 Set a restrictive Tauri CSP, remove unused `opener` permissions, and add a command allowlist | M1-02 | **Done 2026-10-22**; see M1-02 addendum |
| A-02 Add global connection admission and per-session message-rate limits | net/app | M1 before GUI |
| A-03 Show an explicit "compare both screens" SAS warning and never auto-confirm | M1-03 | **Done 2026-10-22**; pairing dialog requires an acknowledgement checkbox and a separate confirm action |
| A-04 Reconcile the 120 s design window with the 10 s handshake and persist throttling if needed | net/product | **Partially done 2026-10-22**: pairing window is 120 s; throttling persistence remains M1 |
| A-05 Move identity seed and TLS key into platform keystores or OS-protected storage | platform/security | M4/M5 |
| A-06 Add cargo-deny, advisory scanning, fuzz budget, and release dependency review | release | M5-03 |
| A-07 Implement T-09 path matrix before any file write | file/app | M2/M3 |
| A-08 Add device-name privacy and log/crash redaction policy to desktop UX | frontend/release | M1/M4 |
| A-09 Complete Android Keystore/SAF/FGS security tests and signed-APK verification | mobile | M4/M5 |

## Verification performed

~~~text
cargo test --workspace                  # 53 at M0 review; 58 after M1-01/M1-02; 61 after M1-03
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
bun run check
bun run check:security
bun run build
~~~

Specific security tests are listed in
[docs/SECURITY.md](../SECURITY.md#9-verification-evidence).

## Addendum: M1-01 adapter (2026-10-21)

M1-01 adds the tauri-specta command contract, a single event Channel, Engine
start/stop/heartbeat events, and the Solid adapter panel. No wire protocol,
cryptographic primitive, trust decision, or file-write path changed. The new
`identity_rotate` command can clear trust but requires no secret and does not
expose a key to the WebView. R-04 (CSP/opener) remained accepted for this
adapter-only UI and was the blocking gate for M1 chat content.

## Addendum: M1-02 WebView hardening (2026-10-22)

M1-02 replaces the adapter-only view with the routed Solid shell and closes
A-01/R-04:

- Production `csp` is `default-src 'self'`, `script-src 'self'`,
  `object-src 'none'`, with only `ipc:` and `http://ipc.localhost` in
  `connect-src`; no `unsafe-eval`.
- `devCsp` adds only the Vite HMR origins (`http://localhost:1420`,
  `ws://localhost:1420`, `ws://localhost:1421`) for local development.
- `freezePrototype` is enabled and the routed UI loads without CSP violations
  in the live WebView.
- `@tauri-apps/plugin-opener` is removed from `package.json`, the Rust
  dependency and `.plugin()` registration are removed, `opener:default` is
  removed from the capability, and the main window has only `core:default`.
- The four custom commands remain the effective allowlist; private keys still
  never cross into the WebView.
- `bun run check:security` fails on CSP weakening, opener regression,
  capability growth, or HTML/eval/URL sinks in `src`.
- Dialog focus is trapped, Escape closes, and focus returns to the trigger; the
  shell also has a working skip link and escaped peer text.

No Rust test count changed: the workspace remains at 58 tests. The security
check and live WebView smoke are separate gates. Residual risk is the inline
style allowance required by the UI runtime; script execution remains
self-only. Peer-controlled chat/clipboard text is still absent from this
slice, so M1-04 must repeat the escaping review before rendering it.

## Sign-off and review triggers

M0 security review is complete for the CLI transport prototype. The high-risk
decisions are recorded; none is silently accepted for the full product. This
record should be reviewed again when:

- a new protocol message or capability is added;
- the Tauri chat/clipboard UI renders peer-controlled content;
- file transfer or a persistent trust/identity change is added;
- an Android/iOS plugin is added;
- a dependency is replaced or a cryptographic primitive changes;
- an external security report is received.

An independent audit remains recommended before a public 1.0 release.
