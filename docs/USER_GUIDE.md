# Fluxy user guide

This is the manual for the desktop app. It assumes Fluxy is installed (see
[PACKAGING.md](PACKAGING.md) for installers) and that both devices are on the
same local network.

## First run

Fluxy keeps everything on the device. On first launch it creates an identity
(an Ed25519 key pair and a self-signed certificate) under the app data directory -
`%APPDATA%/com.thep0y.fluxy` on Windows, `~/Library/Application Support/com.thep0y.fluxy` on
macOS, `~/.local/share/com.thep0y.fluxy` on Linux - and starts three things:

1. A QUIC listener on a system-assigned UDP port.
2. mDNS advertising under `_fluxy._udp.local.`.
3. A UDP gateway/broadcast fallback for networks that filter multicast.

The window opens hidden-aware: a login start stays in the tray, a normal start
shows the window. The device page shows this device's id, listening port, and
discovery state.

## Pairing

Pairing is the only way to create trust, and it is mutual and explicit:

1. The other device appears under **Nearby**. Select it and choose **Pair**.
2. Both devices show the same six-digit code (SAS). Compare them and confirm on
   both. If the codes differ, cancel: that is a machine-in-the-middle.
3. Both sides store the peer's public key and certificate fingerprint. Future
   connections reuse that pin and do not ask again.

A peer whose certificate changes is refused and must be paired again; Fluxy never
accepts a new certificate silently.

## Chat

Chat is scoped to the devices you selected rather than to every trusted peer: the
device page opens a conversation with one peer, and text is delivered over the
same encrypted session as everything else. Messages are kept in memory only - they
are not written to disk, they are cleared on exit, and a failed message can be
retried from the row without re-pairing.

## Clipboard

Clipboard sharing is manual by default. **Share clipboard** offers the current text
(up to 1 MiB) to one device; the receiver sees a prompt unless it has opted into
auto-apply for that device.

| Per-device policy | Behaviour |
| --- | --- |
| Ask (default) | The receiver is prompted before the clipboard changes |
| Auto | The value is applied immediately, with a notification |
| Reject | Offers from this device are declined |

**Auto send clipboard** is a separate switch and off by default. When it is on,
the app reads the clipboard every 300 ms, waits for a stable value, and offers it
to online trusted devices. It never offers a value that arrived from a peer, never
offers the same value twice, and turning the switch on records whatever is already
on the clipboard instead of sending it.

## Files and folders

Three ways to send: drag a file or folder onto the window, use **Send file**, or
use **Send folder**. A confirmation shows what will travel and to which device.

- Files are sent one transfer per file, up to 100 per batch and with a
  configurable number of them active at once (1-4).
- A folder is a batch: the receiver recreates the tree, empty directories
  included, and each file keeps its own progress, checkpoint, and retry.
- Symbolic links are refused rather than followed, a batch is capped at 10,000
  entries, and any relative path that would escape the receive folder is
  rejected before anything is written.
- Received files land in the receive directory, which is configurable in settings
  and defaults to `received/` under the app data directory. A transfer already
  writing keeps its file; the new directory applies to the next one. A name that
  already exists is never overwritten silently.

### The transfer center

Every transfer shows progress, speed, remaining time, and a state. Per transfer
you can pause, resume, cancel, or retry; a failed transfer shows a localized
reason and its stable error code. Folder batches appear as one collapsible row
with batch-wide pause, resume, and cancel.

Transfers survive interruptions: a pause writes a checkpoint, a restart resumes
from the missing blocks, and a source file that changed after the transfer began
is refused with code 3002 rather than sending inconsistent bytes.

## Settings

| Section | What it controls |
| --- | --- |
| Appearance | Light, dark, or follow the system |
| Language | Chinese (default) or English; the tray and notifications follow it |
| Device name | The name peers see; changing it restarts discovery |
| Notifications | Desktop notifications for messages, clipboard, and transfers |
| Start at login | A per-user login item that launches hidden into the tray |
| Network interface | Automatic or manual adapter and address, with the diagnostics in the panel |
| Received files | Where incoming files land, with a folder picker and a reset to the default |
| Trusted devices | Revoke trust and set the per-device clipboard policy |
| Diagnostics | Copy a support payload; rotate the identity if it was compromised |

### Choosing a network interface

Automatic selection prefers the adapter that carries the default route and avoids
VPN and hypervisor adapters, re-checking every two seconds so a moved cable is
followed. Manual selection pins one adapter and address; if that address
disappears, Fluxy reports error 2003 and keeps waiting for it instead of moving to
another interface. Switching drops current sessions and pauses transfers in
flight, which resume from their checkpoint. When the platform cannot pin the
socket to the adapter, the panel says so, because traffic may still leave through
a VPN.

## Tray and background

Closing the window hides Fluxy to the tray; the process keeps receiving. The tray
menu opens the window, toggles **Auto send clipboard** and **Start at login**,
opens **Help and firewall**, and **Quit** - the only action that stops the
listener.

## Help and troubleshooting

The in-app **Help and firewall** page shows the live ports, copyable
per-platform firewall commands, and a short FAQ; [FIREWALL.md](FIREWALL.md) is the
standalone version.

| Symptom | Likely cause | What to try |
| --- | --- | --- |
| No device appears nearby | mDNS filtered, different subnets, or AP isolation | Allow UDP discovery, put both devices on the same Wi-Fi, or rely on the gateway fallback |
| A device appears but pairing or transfers fail | Inbound UDP blocked for the transfer port | Allow the program, or the shown port, in the firewall |
| Works at home, fails on a corporate LAN | Client isolation or NAC | Ask IT, or use a hotspot |
| Devices lose each other behind a VPN | The VPN captured the route | Select the LAN adapter manually in settings |
| A transfer fails with 3002 | The source file changed | Do not edit the file while it is being sent |
| A transfer fails with 3004 | The receiver is out of disk | Free space; the transfer can be retried |
| "Peer is busy" (2002) | The peer is already running its maximum transfers | Wait for one to finish |

## Privacy

Fluxy has no server, no account, and no telemetry. Traffic stays on the local
network and is encrypted with TLS 1.3 using keys exchanged during pairing.
Interface names, addresses, and peer names are local data; the interface list never
travels over the protocol. Logs redact message bodies and file paths, and the
support payload is what you copy yourself - nothing is uploaded.
