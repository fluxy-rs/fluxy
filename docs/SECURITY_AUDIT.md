# Security audit (M5-03)

Plan 9.7 asks for a dependency audit, a fuzz run, and a list of known items with
explanations. This document is that list, and every command in it is reproducible.

## What was run

~~~text
cargo deny check licenses bans sources     # policy, see deny.toml
cargo deny check advisories                # RustSec database (needs network)
cargo test -p fluxy-core --release --test decoder_fuzz            # 20k mutations
FLUXY_FUZZ_ITERATIONS=10000000 cargo test -p fluxy-core --release --test decoder_fuzz
~~~

## Dependency policy

`deny.toml` is checked in and pins three things:

- **Licenses.** Everything linked into a shipped binary must be redistributable
  under Fluxy's own terms. The allow list is MIT, MIT-0, Apache-2.0, Apache-2.0
  WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0,
  Unicode-DFS-2016, CC0-1.0, BSL-1.0, MPL-2.0, and Unlicense.
- **Sources.** Only crates.io; an unknown registry or a git dependency fails the
  check.
- **Bans.** Wildcard dependencies are denied, which is why the workspace crates
  and `udp-chaos` now carry an explicit version on their path dependencies.
  Duplicate versions are warnings rather than errors, because the tree is mostly
  transitive.

Result on this tree: **licenses ok, sources ok, bans ok** with duplicate-version
warnings only. The inventory is 622 packages and no package lacks a license
field. Two license families needed a decision:

| License | Packages | Decision |
| --- | --- | --- |
| MPL-2.0 | cssparser, cssparser-macros, dtoa-short, option-ext, selectors | Accepted. File-level copyleft; the files are used unmodified through the CSS stack, so there is no obligation to publish our sources. |
| LGPL-2.1-or-later | r-efi 5.3.0 and 6.0.0 | Not applicable. Both offer MIT OR Apache-2.0 OR LGPL-2.1-or-later; we rely on the permissive arms, so no LGPL obligation attaches. |

Roughly 45 crates appear in two or more versions (for example `syn` 1 and 2,
three `getrandom` majors, four `windows-sys`). They come from the Tauri
toolchain. Collapsing them would mean patching upstream manifests for no security
benefit, so they are tracked as warnings instead of forced.

## Advisories

~~~text
cargo deny check advisories            # needs the RustSec database
cargo deny --offline check advisories  # same, against a cached database
~~~

The run reports **no vulnerability advisories** and five **unmaintained**
warnings. Unmaintained is not a vulnerability: it means the upstream crate has
been archived or merged, so it will not receive fixes. All five are recorded in
`deny.toml` with a reason, and the table below is the analysis behind those
entries. Two of them never reach a shipped binary because they are build-time
macros; three ship only in the Linux build through the D-Bus notification stack.

| Crate | Advisory | Reached through | Targets | Ships in a binary? |
| --- | --- | --- | --- | --- |
| `paste` 1.0.15 | RUSTSEC-2024-0436 | specta 2.0.0-rc.25 (Tauri command bindings) | all three | No - proc-macro, compiled and discarded |
| `proc-macro-error` 1.0.4 | RUSTSEC-2024-0370 | glib-macros 0.18 - gtk3 | Linux | No - proc-macro |
| `zbus_names` 4.3.5 | RUSTSEC-2026-0371 | notify-rust 4.18.2 - tauri-plugin-notification | Linux | Yes, Linux only (D-Bus names) |
| `zvariant` 5.15.1 | RUSTSEC-2026-0370 | same chain | Linux | Yes, Linux only (D-Bus wire format) |
| `zvariant_derive` 5.15.1 | RUSTSEC-2026-0372 | same chain | Linux | No - proc-macro, but it generates the Linux code above |

None of them has a safe upgrade in this dependency graph: `cargo deny` reports
`No safe upgrade is available` for each, which matches the pins - `tauri-specta`
is at an exact release candidate, the gtk3 stack comes from Tauri's Linux
backend, and `tauri-plugin-notification 2.7.0` pins the notify-rust generation that
still uses zbus 5.

Re-check triggers, so this analysis does not age silently:

- a `tauri-specta` release outside the 2.0.0-rc line retires `paste`;
- a Tauri Linux backend on gtk4/glib 0.19 retires `proc-macro-error`;
- a `tauri-plugin-notification` release on notify-rust with zbus 6 retires the three
  zbus/zvariant crates.

When the upgrade lands, delete the ignore entry and run the check again: an
ignore that outlives its reason is how a real advisory gets missed.
## Unsafe code

The workspace contains five `unsafe` blocks:

| Location | Purpose | Safety argument |
| --- | --- | --- |
| `crates/fluxy-net/src/interface.rs` (3 blocks) | `setsockopt` for `IP_UNICAST_IF`, `IP_BOUND_IF`, `SO_BINDTODEVICE` | The option name and length are compile-time constants for the current target, the value pointer is a stack buffer that outlives the call, and a non-zero return becomes an `io::Error` that is treated as an unpinned warning rather than a failure. |
| `src-tauri/src/notify.rs` | `GetForegroundWindow()` to decide whether a notification is needed | The call takes no pointers, has no memory preconditions, and its possibly-null result is checked before use. |
| `crates/fluxy-proto/build.rs` | `env::set_var("PROTOC", ...)` | Runs in a build script before any other thread exists, which is the precondition `set_var` needs in the 2024 edition. |

No `unsafe` exists in the protocol codecs, the file layer, the transfer state
machine, or the frontend.

## Fuzzing

`cargo-fuzz` needs nightly and libFuzzer; this environment has only stable, so
the harness is a deterministic generator with the same property: the decoders
must never panic on random or mutated input. It covers `decode_frame`,
`decode_payload::<Hello>`, `ChunkHeader::decode`, `Manifest::from_bytes`,
`TransferCheckpoint::from_bytes`, `BatchPlan::from_json`, `validate_relative_path`,
and `sanitize_root_name`. About three quarters of the iterations mutate a
valid sample (bit flips plus truncation) so the length, offset, and hash
arithmetic is actually reached instead of stopping at the magic check.

## Known items

1. **Five unmaintained crates, no vulnerabilities.** Analysed above; two are
   build-time macros and three ship only in the Linux build. Each has a re-check
   trigger listed with it, and each is an explicit `ignore` entry in deny.toml
   rather than a silently lowered severity.
2. **Duplicate dependency versions** (~45 crates). Accepted: they come from the
   Tauri toolchain, are warnings in `deny.toml`, and forcing one version would
   mean patching upstream manifests.
3. **Fuzzing runs on stable, not libFuzzer.** Coverage-guided fuzzing would reach
   deeper paths; the harness trades coverage for reproducibility on the supported
   toolchain. Re-evaluate when a nightly CI lane exists.
4. **No external penetration test.** The threat model in docs/SECURITY.md is
   reviewed against the implementation, not against an attacker with unlimited
   time.
