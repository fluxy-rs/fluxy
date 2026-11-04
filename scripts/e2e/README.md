# M1-09 end-to-end acceptance

This directory contains the Windows, Linux, and macOS scripts that reproduce the
M1 main path: discovery-adjacent first pairing, trust persistence across a
restart, chat delivery with an application ACK, the clipboard consent gate, and
rejection of an untrusted device.

The driver is [e2e.mjs](./e2e.mjs). The two wrappers only locate Node and forward
arguments:

- [run-e2e.sh](./run-e2e.sh) for Linux and macOS.
- [run-e2e.ps1](./run-e2e.ps1) for Windows PowerShell.

## Prerequisites

- A stable Rust toolchain with `cargo` on `PATH`.
- Node.js 22 or newer.
- Windows, Linux, or macOS. The local scenario uses loopback and does not need a
  second machine.

The driver builds `fluxy-cli` with `cargo build -p fluxy-cli` unless
`--no-build` is passed. It then uses a fresh temporary data directory for each
peer and removes it on success.

## Local run

Linux or macOS:

~~~text
bash scripts/e2e/run-e2e.sh local
~~~

Windows PowerShell:

~~~text
pwsh -NoProfile -File scripts/e2e/run-e2e.ps1 local
~~~

Or through the package script:

~~~text
bun run test:e2e
~~~

The local scenario covers five steps:

1. First pairing over loopback, with a six-digit SAS prompt on both sides.
2. Server restart from the same data directory; the device id and certificate
   fingerprint are unchanged, chat resumes with no second SAS prompt, and the
   message receives a `delivered=true` ACK.
3. Clipboard offer reaching the headless receiver, which rejects it with
   `cli_no_consent` because the CLI never writes the OS clipboard.
4. An unknown device id is rejected with an explicit untrusted error.
5. A loopback listener without `--dev-loopback` is refused.

## Cross-host run (Windows to Linux/macOS)

Run the server on one host:

~~~text
bash scripts/e2e/run-e2e.sh serve --yes
~~~

or on Windows:

~~~text
pwsh -NoProfile -File scripts/e2e/run-e2e.ps1 serve --yes
~~~

The server prints machine-readable lines, including one `connect=` line per
non-loopback IPv4 address (or one loopback line for a same-host rehearsal):

~~~text
FLUXY_E2E_SERVER listen=0.0.0.0:41234
FLUXY_E2E_SERVER connect=192.168.1.20:41234
FLUXY_E2E_SERVER device=01a1...
FLUXY_E2E_SERVER fingerprint=...
~~~

On the other host, pass those values to `join`:

~~~text
bash scripts/e2e/run-e2e.sh join \
  --addr 192.168.1.20:41234 \
  --fingerprint <64-hex> \
  --device <uuid> \
  --yes
~~~

The server prints `SAS: <six digits>`, `message: ...`, and
`clipboard-offer ...`; the joining side prints `PASS` after the chat ACK and a
terminal clipboard acknowledgement. Both hosts must be on the same LAN and the
server firewall must allow the QUIC UDP port printed by `serve`.

For a same-host rehearsal, use `serve --listen 127.0.0.1:0 --yes` and add
`--dev-loopback` to `join`.

## Security warning

The driver writes `yes` when it sees the SAS prompt. That makes the automated
path deterministic, but it bypasses the human comparison of the six digits.
Run these scripts only on an isolated test network, and never use this mode to
pair production devices. The M1-10 phone-hotspot check and the manual SAS
comparison remain separate release gates.

## Options

| Option | Meaning |
| --- | --- |
| `--bin PATH` | Use a specific `fluxy-cli` binary. |
| `--no-build` | Skip `cargo build -p fluxy-cli`. |
| `--keep` | Keep the temporary data directories for inspection. |
| `--data-dir PATH` | Override the data directory for `serve` or `join`. |
| `--listen ADDR` | Server listen address, default `0.0.0.0:0`. |
| `--addr ADDR` | Remote peer address for `join`. |
| `--fingerprint HEX` | Remote certificate fingerprint for `join`. |
| `--device UUID` | Remote device id for `join`. |
| `--dev-loopback` | Allow loopback addresses in `join` for a local rehearsal. |
| `--yes` | Required for `serve` and `join`; auto-confirms the SAS. |

Exit code `0` means every step passed, `1` means a step failed, and `2` means
the usage was invalid.

## Continuous integration

The [CI workflow](../../.github/workflows/ci.yml) runs the local scenario on
`ubuntu-latest`, `macos-latest`, and `windows-latest`. That proves the
per-platform scripts and the shared protocol path; it does not replace the
physical Windows-to-Linux/macOS LAN run, which stays a manual acceptance step.
