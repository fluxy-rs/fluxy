# Known issues (v0.2.0)

Everything here is a real limitation of this build, not a bug report. Each entry
says what happens, what to do about it, and where it is tracked.

## Blocking a signed release

**Installers are unsigned.** The NSIS and MSI packages report `NotSigned`, so
Windows SmartScreen warns on first install and macOS Gatekeeper refuses a
quarantined bundle. Workaround for testers: install anyway, or build from source.
Tracked in [PACKAGING.md](PACKAGING.md), which documents the Windows certificate
and macOS notarization paths.

**Five dependencies are unmaintained.** `cargo deny check advisories` reports no
vulnerabilities but flags `paste`, `proc-macro-error`, `zbus_names`, `zvariant` and
`zvariant_derive` as no longer maintained upstream. Two are build-time macros that
never reach a binary; three ship only in the Linux build through the D-Bus
notification stack. None has a safe upgrade in this graph. Each is an explicit,
reasoned ignore entry in `deny.toml` with the re-check trigger recorded in
[SECURITY_AUDIT.md](SECURITY_AUDIT.md).

## Limitations in the product

**A folder with no files cannot be sent on its own.** Empty directories inside a
batch are recreated, but a folder that contains only directories has no file to
announce the batch. Workaround: put the empty folder inside a folder that has at
least one file.

**A batch is at most 10,000 entries.** This is a deliberate limit, not an
accident: it keeps the offer bounded. Larger trees have to be split.

**Symbolic links are refused, not followed.** A folder containing a link fails
with a message naming the link. Workaround: remove the link, or send the targets
separately.

**One listener at a time.** Fluxy binds a single local address, so a device on two
LANs is reachable on one of them. Workaround: switch the interface in settings, or
run two instances with different data directories (development only).

**No automatic retry after a restart.** An interrupted transfer resumes from its
checkpoint, but a message that was never acknowledged is not retried
automatically; retry it from the row.

**Pairing lockout is per-process.** Too many failed pairings are rate-limited
within a run, but the lockout counter is not persisted across restarts.

## Performance

**Loopback throughput is about 240-265 MiB/s, not the plan's 1.0 GiB/s.** The
limit is the single UDP socket's packet rate and the per-chunk control round trip;
streams, windows, MTU, socket buffers, and per-chunk copies were all measured and
none of them move the number. 1 GbE (105 MB/s) is met with margin, so LAN
transfers are unaffected. Tracked in [TESTING.md](TESTING.md) and
[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).

## Platform notes

**Windows toasts need an installed build.** A development run may show PowerShell
branding on the notification; the message content is still correct.

**WebView2 is required.** It ships with Windows 10 1803+ and Windows 11; older
machines get it from the installer's bootstrapper and need network access once.

**The firewall rule is per program, not per port.** The transfer port is
system-assigned by default (`listen_port: 0`), so the guidance allows the
Fluxy executable rather than a fixed port. Pinning the port is not implemented.

## Checks that need a second machine

These are not defects, but they are unverified in this development environment
and must be recorded before a release: a clean-machine install, the tray and
login-item behaviour on macOS and Linux, Windows toast delivery from an installed
build, and the two-machine transfer matrices T-11 (moving a cable) and T-13 (a
nested 10 GiB folder, 10k files, a symlinked tree). The matrix and the commands to
reproduce each row are in [RELEASE_MATRIX.md](RELEASE_MATRIX.md).
