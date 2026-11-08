# Release matrix (M5-02)

Plan 9.7 asks for a Windows/macOS/Linux regression matrix with every row green.
This document is the matrix, where each row is verified, and how to reproduce it.

## The matrix

| Check | Windows | macOS | Linux |
| --- | --- | --- | --- |
| `cargo fmt --all --check` | Local, green | CI | CI |
| `cargo clippy --workspace --all-targets -- -D warnings` | Local, green | CI | CI |
| `cargo test --workspace` (302 tests) | Local, green | CI | CI |
| Decoder fuzz (20k per run, 35M campaign) | Local, green | CI | CI |
| Frontend check, tests, security scan, build | Local, green | CI | CI |
| End-to-end CLI acceptance (5 steps) | Local, green | CI | CI |
| Release binary (`tauri build --no-bundle`) | Local, green | CI (on demand) | CI (on demand) |
| Installer (NSIS + MSI) | Local, green (install and uninstall verified) | n/a (DMG) | n/a (AppImage, deb) |
| Installer signature | Not signed yet | Not signed yet | n/a |
| Cross-target type check of `fluxy-proto` and `fluxy-core` | n/a | not attempted | Local, green (`--target x86_64-unknown-linux-gnu`) |
| Multi-interface binding | Local, green (WLAN pinned, mDNS advertised) | needs a host | needs a host |
| Android APK build and 16 KB check | n/a | n/a | Local, green (arm64 debug APK, 0x4000 LOAD align) |
| Soak (`bun run soak`) | Local, green (short runs); 24 h run is a release gate | Same harness | Same harness |
| Two-machine transfer matrix (T-11, T-13) | Manual, needs two hosts | Manual | Manual |

"Local" means this repository's development machine, a Windows 11 host. "CI"
means `.github/workflows/ci.yml`, which runs the same commands on
`ubuntu-latest`, `macos-latest` and `windows-latest` for every pull request.
A merge to main does not repeat the matrix - the merged tree is the one the pull
request just tested - so `.github/workflows/warm-cache.yml` refills the cargo and
bun caches after a dependency change instead. A platform is release-ready
when its CI rows are green and its manual rows are recorded.

## What changed for this milestone

The pipeline used to run the Rust gates on Linux only and the end-to-end
scenario on all three, which left the two platforms that ship a different
installer largely unverified. `ci.yml` now runs format, lint, the full test
suite, and the fuzz harness on all three platforms, keeps the web job on Linux
(the frontend is platform-independent), and adds an on-demand bundle job that
runs `tauri build --no-bundle` on each platform so the packaging chain is
exercised without needing signing certificates or installer toolchains.

One ordering detail matters: `tauri-build` embeds the frontend during the
Rust build, so every matrix leg installs Bun and builds the frontend before the
first cargo command. Without that, a clean checkout cannot even run `cargo test`.

## Cross-target checking from one host

A Windows host cannot run a Linux binary, but it can type-check the platform-
independent crates against the Linux standard library:

~~~text
rustup target add x86_64-unknown-linux-gnu
cargo check -p fluxy-proto -p fluxy-core --target x86_64-unknown-linux-gnu
~~~

Both pass here, which covers the protocol, file, transfer, batch, and path
safety layers. `fluxy-net` cannot be checked that way from this host: its
`ring` dependency has a C build script that needs a Linux cross-compiler. That
is a toolchain gap, not a code gap, and it is why the CI Linux leg exists.

## Platform-specific code to watch

| Area | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Listener pinning | `IP_UNICAST_IF` / `IPV6_UNICAST_IF`, index in network byte order | `IP_BOUND_IF` / `IPV6_BOUND_IF` | `SO_BINDTODEVICE` |
| Tray and close behaviour | `TaskDialogIndirect` via a Common Controls v6 manifest | native menu bar | AppIndicator |
| Login item | `HKCU\\...\\Run` value written with `reg.exe` | launchd plist | XDG desktop entry |
| Clipboard | `arboard` (Win32) | `arboard` (NSPasteboard) | `arboard` (X11) |
| Notifications | WinRT toast (needs an installed build) | `notify-rust` | `notify-rust` (D-Bus) |

Each of these has a unit test that runs on every platform (the file-based login
backends, the pin argument builders, the notification gating), and each needs the
manual check in docs/TESTING.md on a host of that platform before a release.

## Release gate

1. `rust` matrix green on all three OSes.
2. `web` and `e2e` green on all three OSes.
3. The on-demand `bundle` job green for the platforms being released.
4. Manual rows recorded: clean-machine install (docs/PACKAGING.md), tray and
   login item, notifications, and the two-machine transfer matrix.
5. `cargo deny check advisories` run on a networked machine
   (docs/SECURITY_AUDIT.md).
