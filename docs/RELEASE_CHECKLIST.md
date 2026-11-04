# Release checklist (v0.2.0)

A release is a sequence of checks that each have an owner and an artifact. Tick
them in order; a failure stops the release rather than being noted and skipped.

## 1. Code and gates

| # | Check | Command | Evidence |
| --- | --- | --- | --- |
| 1 | Formatting | `cargo fmt --all --check` | exit 0 |
| 2 | Lints | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| 3 | Rust tests | `cargo test --workspace` | 302 passed, 2 ignored |
| 4 | Fuzz campaign | `FLUXY_FUZZ_ITERATIONS=35000000 cargo test -p fluxy-core --release --test decoder_fuzz` | no panic, ~10 minutes |
| 5 | Frontend | `bun run check && bun run test:frontend && bun run check:security && bun run build` | 90 tests pass |
| 6 | CLI acceptance | `bun run test:e2e` | 5 steps pass |
| 7 | Dependency policy | `cargo deny check licenses bans sources` | licenses/bans/sources ok |
| 8 | Advisories | `cargo deny check advisories` (or `--offline` against a cached database) | no vulnerabilities; five unmaintained crates reasoned in deny.toml |
| 9 | CI matrix | `rust`, `web`, `e2e` green on all three OSes | [RELEASE_MATRIX.md](RELEASE_MATRIX.md) |
| 9a | Version consistency | `bun run check:versions` | one version across Cargo.toml, package.json, tauri.conf.json |
| 9b | Soak | `bun run soak --minutes 1440 --size-mib 8 --interval-seconds 30` | 24 h, `failures: 0`, flat RSS |

## 2. Build and package

| # | Check | Command | Evidence |
| --- | --- | --- | --- |
| 10 | Release binary | `bun x tauri build --no-bundle` | `target/release/fluxy.exe`, version resource 0.2.0 |
| 11 | Windows installers | `bun x tauri build --bundles nsis,msi` | NSIS + two MSIs (en-US, zh-CN) |
| 12 | Install/uninstall | silent install to a temp dir, then uninstall | exit 0, directory gone |
| 13 | Signing | see [PACKAGING.md](PACKAGING.md) | `Get-AuthenticodeSignature` not `NotSigned` |
| 14 | Checksums | `Get-FileHash -Algorithm SHA256` on every artifact | hashes in the release notes |

## 3. Manual verification

| # | Check | Where |
| --- | --- | --- |
| 15 | Clean-machine install, launch, tray, uninstall, reinstall keeps the identity | [PACKAGING.md](PACKAGING.md) |
| 16 | Pair with a second device over a real LAN, then chat, clipboard, one file, one folder | a second host |
| 17 | Move a cable during a transfer (T-11) | a second host |
| 18 | Nested 10 GiB folder and a 10k-file folder (T-13) | a second host |
| 19 | Tray restore, close-to-tray, login start, notification delivery | each target OS |
| 20 | Network interface switch: sessions drop, transfers resume | one host with two adapters |

## 4. Documentation

| # | Check |
| --- | --- |
| 21 | [README](../README.md) describes install, features, platforms, and the security posture |
| 22 | [USER_GUIDE.md](USER_GUIDE.md) matches the shipped UI (sections, switch names, defaults) |
| 23 | [KNOWN_ISSUES.md](KNOWN_ISSUES.md) reflects what is still true for this build |
| 24 | [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) has the current test counts and gap list |
| 25 | [RELEASE_NOTES](RELEASE_NOTES_0.2.0.md) written, reviewed, and linked from the release |
| 26 | Every relative documentation link resolves |

## 5. Tag and publish

| # | Check |
| --- | --- |
| 27 | Version bumped in `Cargo.toml` (workspace and internal dependency versions), `package.json`, and `src-tauri/tauri.conf.json` - all three must agree |
| 28 | `CHANGELOG` or release notes cover every milestone since the last tag |
| 29 | Tag `v0.2.0` on the release commit; the bundle job runs on tags |
| 30 | Release created with the installers, checksums, and the known-issues link |

## Sign-off

| Role | Name | Date |
| --- | --- | --- |
| Release manager | | |
| Security reviewer | | |
| Desktop reviewer | | |
