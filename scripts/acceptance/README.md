# Acceptance matrix (M2-09)

`scripts/acceptance/` drives the real `fluxy-cli` over UDP loopback and checks
the acceptance matrix from [docs/DEVELOPMENT_PLAN.md](../../docs/DEVELOPMENT_PLAN.md)
section 11.2. Everything here is process level: two data directories are paired
first, then one process sends while the other receives, so the driver can cut
the network, kill a receiver, or sample memory while a transfer runs.

## Prerequisites

- Rust toolchain (`cargo build -p fluxy-cli` is run by the driver unless `--no-build`).
- Node.js 18 or newer (the driver is plain `.mjs`, no dependencies).
- Enough free space for the payload: the 10 GiB benchmark needs about 20 GiB
  (source plus received copy) unless `--file` points at an existing file.

## Quick start

```powershell
# Windows
pwsh -File scripts/acceptance/run-acceptance.ps1 matrix

# Linux or macOS
./scripts/acceptance/run-acceptance.sh matrix
```

The driver prints one line per scenario and finishes with the matrix summary.
Exit code 0 means every automated check passed, 1 means a check failed, and 2
means the driver itself could not run (bad option, missing binary, pairing
failure).

## Scenarios

| Scenario | What it runs | Matrix rows |
| --- | --- | --- |
| `local` | pair two peers, transfer one file, compare root hashes | T-01 (small) |
| `outage` | cut the UDP path through a relay, then resume | T-02 |
| `restart` | kill the receiver mid-transfer, restart it, resume | T-03 |
| `bench` | transfer a large file with a receiver memory ceiling | T-01 |
| `faults` | `cargo test -p udp-chaos --test transfer_faults` | T-02, T-03, T-04, T-06 |
| `matrix` | faults, bench, outage, restart, plus the manual rows | T-01 .. T-06 |

`matrix` uses the `--size` payload (64 MiB by default) so it stays quick; the
real 10 GiB run is a separate command (see below). T-05 stays manual because it
needs an ACL change on the source file, and the desktop `WaitingForFileAccess`
state it feeds arrives with M4.

## Options

| Option | Meaning |
| --- | --- |
| `--bin PATH` | CLI binary to drive (default `target/debug/fluxy-cli[.exe]`) |
| `--no-build` | skip `cargo build -p fluxy-cli` |
| `--release` | build and drive the release profile (needed for the throughput target) |
| `--size SIZE` | payload size, e.g. `64MiB`, `10GiB` |
| `--file PATH` | existing file for `bench` instead of generating one |
| `--outage SECONDS` | outage length for `outage` (default 30) |
| `--timeout SECONDS` | receiver offer timeout (default 300) |
| `--transfer-id ID` | reuse a transfer id instead of a random one |
| `--data-dir DIR` | reuse a directory instead of a temporary one |
| `--keep` | keep the temporary data directory for inspection |
| `--json` | also print the report as JSON |

## T-01: the 10 GiB benchmark

```powershell
# Generates a 10 GiB file, transfers it, and samples the receiver's RSS.
# A debug build is far too slow here, so use the release profile.
pwsh -File scripts/acceptance/run-acceptance.ps1 bench --size 10GiB --release

# Or point at a file that already exists.
pwsh -File scripts/acceptance/run-acceptance.ps1 bench --file D:\bench\10gib.bin
```

The scenario passes when the transfer completes, both sides report the same
root hash, and the receiver's peak resident memory stays at or below 200 MB.
Windows sampling uses `Get-Process`, Linux and macOS use `/proc/<pid>/status`;
when no sample can be taken the memory check is reported as skipped.

The protocol-level half of T-01 is the ignored Rust test:

```powershell
$env:FLUXY_BENCH_FILE = "D:\bench\10gib.bin"
cargo test -p udp-chaos --test transfer_faults a_ten_gib -- --ignored --nocapture
```

It prints the pre-hash time, the transfer time, and the throughput, and fails
unless the received root hash matches the sender's.

### Cross-platform T-01 (manual)

The plan asks for the three platform pairs (Windows/Windows, Windows/Linux,
Linux/Linux) to agree on the root hash. Run the receiver on one host:

```bash
fluxy-cli --data-dir /tmp/fluxy-recv transfer-recv --listen 0.0.0.0:0
```

Pair the second host once (`fluxy-cli --data-dir <dir> pair <addr> --fingerprint <fp>`,
confirming the SAS on both sides), then:

```bash
fluxy-cli --data-dir /tmp/fluxy-send transfer-send <host>:<port> --device <receiver-id> --file /data/10gib.bin
```

Both sides print `FLUXY_XFER role=... root_hash=...`; the hashes must match, and
the receiver's RSS should stay under 200 MB. Record the host pairs in the
milestone notes.

## T-05: revoked file access (manual)

M2 has no UI waiting state, so this stays a manual check of the error mapping
and the partial-file guarantee.

1. Start a transfer of a file large enough to still be running after the ACL
   change (for example 1 GiB).
2. While it runs, deny read access to the source file:
   - Windows: `icacls <file> /deny "%USERNAME%:(R)"`
   - Linux: `chmod 000 <file>`
3. Expect the sender to fail with `code=file_access code_number=3003
   retryable=true` (a permission failure is retryable on purpose, so the app can
   wait for access instead of failing the transfer outright).
4. Expect the receiver to keep its partial: `received/<name>.part` and
   `checkpoints/<transfer-id>.json` must still exist, and no complete file must
   appear in `received/`.
5. Restore access (`icacls <file> /remove:d "%USERNAME%"` or `chmod 644 <file>`)
   and retry the same transfer id; the receiver resumes instead of starting over.

The `WaitingForFileAccess` state that surfaces this in the desktop UI is M4
work; until then the code and the partial file are the observable behaviour.

## Recorded run (Windows, debug build, loopback)

A full `matrix --size 64MiB` run on the development machine reported:

~~~text
T-02  PASS  short outage recovers in place           ok
T-03  PASS  restart resumes missing blocks           ok
T-04  PASS  changed source refused with 3002         ok
T-06  PASS  chat p95 under load                      p95 31.9ms, 100 messages
T-01  PASS  large transfer with a memory ceiling     64.0 MiB in 0.9s (68 MiB/s), receiver peak RSS 35 MiB
T-02  PASS  link outage and resume                   cut 30s, dropped 88 datagrams, resumed 52.0 MiB of 64.0 MiB
T-03  PASS  receiver restart and resume              killed after 16.0 MiB, resumed 48.0 MiB of 64.0 MiB
T-05  MANUAL revoked file access keeps the partial   see scripts/acceptance/README.md
~~~

A 10 GiB run with `--release` on the same machine reported `10.00 GiB in 72.5s
(141 MiB/s), prehash 5250ms, receiver peak RSS 51 MiB (120 samples)`; after the
pull window started refilling on chunk completion it reached about 150 MiB/s.
The debug binary reaches about 70 MiB/s, so the 1 GiB/s loopback throughput
target in section 11.3 is measured with `--release`; the resume and fault
behaviour is identical in both profiles. See
[docs/TESTING.md](../../docs/TESTING.md) for why the loopback target is still
open.

## What is automated where

- `faults` covers the protocol-level behaviour of T-02, T-03, T-04 and T-06:
  a short outage that recovers in place, a restarted receiver that resumes only
  the missing blocks, a source change that is refused with 3002 and leaves no
  complete file, and chat that stays under a 250 ms p95 while the link is
  saturated.
- `outage` and `restart` cover the same ground through real processes and the
  real CLI, including killing a receiver outright.
- T-01 is covered by `bench` (process level, with the memory ceiling) and the
  ignored Rust benchmark (protocol level, 10 GiB, hash equality).
- T-05, the cross-platform host pairs, and the T-11 network matrix stay manual
  and are recorded in the milestone notes.
- The T-11 rows that can be read off one machine are now scriptable: @cargo run
  -p fluxy-cli -- interfaces [--json]@ lists every adapter with its kind, index,
  up flag, virtual flag, default-route flag, addresses, and the interface the
  automatic pick would choose, and @serve --interface/--address@ binds and pins
  that choice. What still needs two hosts is the pair-and-transfer step across a
  real cable move; docs/TESTING.md records the procedure.
