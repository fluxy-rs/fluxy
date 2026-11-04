# Fluxy 0.2.0

The first desktop release. Fluxy moves files, folders, clipboard text, and chat
between devices on the same local network, with no server, no account, and no
telemetry.

## Highlights

**Transfer that survives the network.** Files are pulled by the receiver in 4 MiB
chunks over QUIC with TLS 1.3, verified per chunk and again at the root hash, and
resumable after a pause, a crash, or a restart. An interrupted transfer continues
from the missing blocks.

**Folders, not a zip.** Drag a folder and the receiver rebuilds the tree, empty
directories included, while every file keeps its own progress and retry. Relative
paths are validated before anything is written: no absolute paths, no traversal,
no control or bidirectional characters, no alternate data streams, no reserved
device names, no case-only duplicates, and no path that escapes the receive
folder. Symbolic links are refused rather than followed.

**Pairing you can check.** A six-digit code shown on both devices, a pinned
certificate per peer, and a refusal to accept a changed certificate silently.

**Clipboard with consent.** Manual sharing by default, a per-device policy
(ask, auto, reject), and an automatic watcher that is off until you enable it and
never re-sends what it received.

**Knows which adapter to use.** Automatic selection follows the default route and
avoids VPN and hypervisor adapters, re-checking every two seconds; manual selection
pins one adapter and address and reports error 2003 if that address disappears
instead of silently moving. The settings panel explains what was chosen and why.

**Fits a desktop app.** Tray with close-to-tray, start-at-login, native
notifications, light/dark themes, Chinese and English, keyboard-complete pairing,
and a help page with copyable firewall rules for the ports actually in use.

## Install

Windows: NSIS installer (per-user, no elevation) or MSI, in English or Simplified
Chinese. macOS: `.dmg`. Linux: AppImage and deb. The installers are **unsigned**;
see the known issues below.

## Known issues

- Installers are unsigned, so SmartScreen warns and Gatekeeper refuses a
  quarantined macOS bundle.
- Five dependencies are unmaintained upstream (no known vulnerabilities). Two are
  build-time macros, three ship only in the Linux notification stack; each has a
  documented re-check trigger in docs/SECURITY_AUDIT.md.
- A folder that contains no files cannot be sent on its own (empty directories
  inside a batch are fine).
- The receive directory is configurable; transfers already writing stay where they
  started.
- Loopback throughput is about 240-265 MiB/s rather than the internal 1.0 GiB/s
  target; 1 GbE is met with margin.
- The transfer port is system-assigned by default (the firewall guidance allows the
  program rather than a port).

docs/KNOWN_ISSUES.md has the full list with workarounds.

## Checksums

SHA-256 of the Windows artifacts from this build (`target/release/bundle/SHA256SUMS.txt`):

~~~text
85d1ee644f0e8a809825dd6eec8233ad18462998f3aacd381dc1cb9150e8cfe5  msi/Fluxy_0.2.0_x64_en-US.msi
ddf46b4c085f369a78652115a1cbbb1acadf047a259590a58431fb7365b6d85b  msi/Fluxy_0.2.0_x64_zh-CN.msi
1bfc224ae8ade7c846508d6a504f1d3d9198ce100718bffae471d4c446f84a09  nsis/Fluxy_0.2.0_x64-setup.exe
~~~

Verify with `Get-FileHash -Algorithm SHA256 <file>` before installing.

## Compatibility

Protocol version 1. A 0.2.0 device pairs with another 0.2.0 device; an older
build is refused with code 1002 rather than half-working. Device identities are
stable across upgrades, so pairings survive a reinstall.

## Under the hood

| Area | Implementation |
| --- | --- |
| Transport | QUIC 0.11 with rustls 0.23 and pinned certificates |
| Integrity | BLAKE3 per chunk, per block, and over the whole file |
| Identity | Ed25519 keys, self-signed certificate, per-device pin |
| Discovery | mDNS with a UDP gateway/broadcast fallback |
| Shell | Tauri 2.12 with a SolidJS frontend |
| Tests | 304 Rust tests, 91 frontend tests, a decoder fuzz harness, a soak harness, and a CLI acceptance scenario |

## Thanks

This release is the work of the Fluxy contributors listed in the repository
history. File an issue for anything that does not behave as described here.
