# Fluxy firewall and network guide

Fluxy never talks to a server and never relays through the internet: two devices
on the same LAN talk directly. That still means the operating system firewall has
to let Fluxy's UDP traffic in. When it does not, the symptom is almost always the
same — devices see each other in the nearby list, but pairing, chat, clipboard,
or transfers never connect.

This document covers the ports, the per-platform rules, and how to tell which
side is blocking. The same information, with the ports of the running instance
already substituted, is available inside the app: **tray menu → Help and
firewall** (also reachable from the sidebar).

## Ports

| Purpose | Transport | Port | Configurable |
| --- | --- | --- | --- |
| Device discovery (mDNS + gateway probe fallback) | UDP | `42420` | `discovery_port` in `config.json` |
| Chat, clipboard, and file transfer (QUIC) | UDP | the port the listener bound | `listen_port`; `0` means the system assigns one |

Both are inbound. Fluxy opens no TCP listener and does not need a rule for
outbound traffic — the host firewall usually allows that already.

Two consequences matter for rule authoring:

- **Discovery uses a fixed port.** A rule for UDP 42420 is stable across runs.
- **The transfer port can change.** With the default `listen_port: 0` the
  operating system picks an ephemeral port each start, so a rule for one port
  number goes stale. Allow the **program** instead of a single port, or pin
  `listen_port` to a value you keep in the firewall rule.

The ports the running instance actually uses are shown on the in-app help page
and in the diagnostics payload (**Settings → Diagnostics → Copy diagnostics**).

## Windows

Run PowerShell **as administrator**:

```powershell
# Discovery: fixed port
New-NetFirewallRule -DisplayName "Fluxy discovery" -Direction Inbound `
  -Protocol UDP -LocalPort 42420 -Action Allow

# Transfers: allow the program so the ephemeral port is covered
New-NetFirewallRule -DisplayName "Fluxy" -Direction Inbound `
  -Program "$env:LOCALAPPDATA\Fluxy\fluxy.exe" -Action Allow
```

If you prefer to pin the transfer port, set `listen_port` to a fixed value in
`config.json` and add `-LocalPort <port>` instead of `-Program`.

Notes:

- **First launch prompt.** Windows shows a "Windows Defender Firewall has blocked
  some features" dialog the first time Fluxy listens. Tick **Private networks**
  and allow access. If the dialog was dismissed with Cancel, the block rule stays
  until you add an allow rule manually.
- **Network profile.** A network Windows classifies as *Public* blocks inbound
  traffic by default, even with the rules above. Either switch the profile to
  *Private* (Settings → Network & internet → Ethernet/Wi-Fi → Network profile) or
  add the rule for the Public profile too (`-Profile Any`).
- **Verify:** `Get-NetFirewallRule -DisplayName "Fluxy*" | Format-Table Name,Enabled,Profile,Action`
- **Remove:** `Remove-NetFirewallRule -DisplayName "Fluxy*"`
- **Third-party security suites** (Kaspersky, ESET, 360, ...) keep their own
  filter and ignore Windows Firewall rules; allow Fluxy in that product as well.

## macOS

Either allow Fluxy in **System Settings → Network → Firewall → Options**, or use
`socketfilterfw`:

```sh
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /Applications/Fluxy.app
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp /Applications/Fluxy.app
```

- **Verify:** `sudo /usr/libexec/ApplicationFirewall/socketfilterfw --listapps | grep -A2 Fluxy`
- **Local network privacy (macOS 15+).** The first inbound connection also
  triggers a *local network* permission prompt; denying it blocks discovery even
  when the firewall is open. Re-enable it under **Privacy & Security → Local
  Network**.
- If the firewall is switched off entirely, no rule is needed.

## Linux

ufw:

```sh
sudo ufw allow 42420/udp     # discovery
sudo ufw allow 53000/udp     # repeat with your pinned listen_port if you set one
```

firewalld (rule names are arbitrary):

```sh
sudo firewall-cmd --permanent --add-port=42420/udp
sudo firewall-cmd --permanent --add-port=53000/udp   # the pinned transfer port
sudo firewall-cmd --reload
```

nftables/iptables users: allow inbound UDP on the two ports above on the LAN
interface. On distributions that ship no firewall (most desktop installs), no
rule is needed.

- **Verify (ufw):** `sudo ufw status verbose`
- **Verify (firewalld):** `sudo firewall-cmd --list-ports`

## When it still does not work

| Symptom | Likely cause | What to do |
| --- | --- | --- |
| No device ever appears nearby | mDNS multicast blocked, devices on different subnets, or AP/guest isolation | Allow UDP 42420, put both devices on the same subnet, turn off client isolation. Fluxy also probes the gateway as a fallback. |
| A device appears, but pairing or transfers fail | Inbound UDP to the transfer port is blocked | Allow the Fluxy program (or the pinned port) as described above. |
| Works after the first launch prompt, fails after a reboot | The block rule from a dismissed prompt persists | Add the allow rules explicitly. |
| Works at home, fails on a corporate or campus LAN | Managed firewall or NAC blocks peer-to-peer UDP | Ask IT to allow the two ports; a hotspot or home LAN is the fallback. |
| Devices lose each other behind a VPN | The VPN captured the route and discovery moved to its interface | Disable the VPN or wait for discovery to select the new interface; check the interface name in Settings → Diagnostics. |
| Discovery fails on a phone hotspot | Hotspots commonly block multicast | Fluxy falls back to gateway probes; if that also fails, connect both devices to the same router. |
| Transfers stall at the start | MTU or a stateful firewall dropping the QUIC handshake | Check the transfer error code; retry after allowing the port. |

## Which address is bound

The transfer rule has to allow the address the app actually binds. Automatic
selection prefers the interface that carries the default route, avoids VPN and
hypervisor adapters, and re-checks the table every two seconds, so the bound
address can change when a cable is moved or Wi-Fi is joined. A manual selection
pins one interface and address instead; if that address disappears, Fluxy reports
it (error 2003) and keeps waiting for it rather than moving to another interface
behind your back. The interface list and the current choice are visible in the
help page and from `cargo run -p fluxy-cli -- interfaces`, which is the quickest
way to confirm that a rule and a listener agree on the same address.

The in-app help page shows the live listener address, the mDNS service name, and
whether this device is advertising and browsing, which distinguishes "the
firewall blocks us" from "discovery never started".

## What Fluxy deliberately does not do

- No port forwarding, no UPnP/NAT-PMP, no relay, no cloud rendezvous. Traffic
  stays on the local link and is encrypted with the paired device's key.
- No listener on a public interface: the app refuses to advertise on loopback
  unless the development flag `dev_allow_loopback` is set. Automatic selection
  never picks loopback, and manual mode validates the chosen address against the
  interface table before binding.
- No silent clipboard writes: a received clipboard value is only applied after
  the per-device policy allows it.

## Background operation

Closing the window hides Fluxy to the system tray instead of quitting, so a
closed window keeps receiving chat, clipboard offers, and transfers. The tray
menu carries the two switches — *Auto send clipboard* and *Start at login* —
plus the help entry and Quit. Quit is the only action that stops the process and
closes the listener; use it before removing firewall rules.

*Start at login* registers a login item that launches Fluxy with `--hidden`, so
a login start lands in the tray:

- Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Fluxy`
- macOS: `~/Library/LaunchAgents/dev.fluxy.app.plist`
- Linux: `~/.config/autostart/fluxy.desktop`

If a cleaner removes the login item while the preference is still on, Fluxy
re-creates it on the next start.
