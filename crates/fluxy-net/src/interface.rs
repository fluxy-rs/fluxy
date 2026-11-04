//! Local interface enumeration and address selection.
//!
//! Section 3.5 of the plan makes "which interface and which address" a real
//! setting: a laptop usually has Ethernet, Wi-Fi, a VPN, and a Hyper-V adapter at
//! the same time, and advertising the wrong one is what produces the classic
//! "the device is visible but the connection never works" report.
//!
//! Everything here is either pure (ranking, validation, formatting) or a thin
//! platform call, so the picker can be tested without a second network card.
//! Interface names and addresses never enter the protocol; they only reach the
//! local UI and the log.

use std::fmt;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};

use default_net::interface::InterfaceType;
use default_net::{get_interfaces, interface::get_default_interface_name};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use fluxy_core::{Error, ErrorCode};

/// How the local address is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// Pick the best LAN address and follow it when the network changes.
    Auto,
    /// Use the interface and address the user picked; never fall back silently.
    Manual,
}

impl SelectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }
}

/// Why the picker chose this address. Shown in the UI so a surprising choice is
/// explainable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    /// The interface that owns the default route.
    DefaultRoute,
    /// A private IPv4 address on an interface without the default route.
    PrivateAddress,
    /// Nothing better was available.
    FirstAvailable,
    /// The development loopback fallback.
    Loopback,
    /// The user picked it.
    Manual,
}

impl SelectionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DefaultRoute => "default_route",
            Self::PrivateAddress => "private_address",
            Self::FirstAvailable => "first_available",
            Self::Loopback => "loopback",
            Self::Manual => "manual",
        }
    }
}

/// Interface class, taken from the platform's own type so the picker never has
/// to guess from a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceKind {
    Ethernet,
    Wireless,
    Tunnel,
    Loopback,
    PointToPoint,
    Other,
}

impl InterfaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ethernet => "ethernet",
            Self::Wireless => "wireless",
            Self::Tunnel => "tunnel",
            Self::Loopback => "loopback",
            Self::PointToPoint => "point_to_point",
            Self::Other => "other",
        }
    }
}

/// One address on one interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressInfo {
    pub address: IpAddr,
    /// A link-local IPv6 address needs the interface index on the socket and the
    /// interface name in the UI.
    pub link_local: bool,
    pub private: bool,
}

impl AddressInfo {
    pub fn new(address: IpAddr) -> Self {
        let link_local = match address {
            IpAddr::V4(address) => address.is_link_local(),
            IpAddr::V6(address) => is_link_local_v6(address),
        };
        let private = match address {
            IpAddr::V4(address) => address.is_private(),
            IpAddr::V6(address) => is_unique_local_v6(address),
        };
        Self {
            address,
            link_local,
            private,
        }
    }

    /// How the address is shown: a link-local IPv6 address carries its scope.
    pub fn display(&self, interface: &str) -> String {
        format_address(self.address, self.link_local.then_some(interface))
    }
}

/// One local interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceInfo {
    pub name: String,
    /// Friendly name when the platform has one ("Wi-Fi" rather than "WLAN").
    pub label: String,
    /// Platform description ("Hyper-V Virtual Ethernet Adapter"). Used only to
    /// recognise virtual adapters; it never leaves the machine.
    pub description: Option<String>,
    pub index: u32,
    pub kind: InterfaceKind,
    pub up: bool,
    pub addresses: Vec<AddressInfo>,
}

impl InterfaceInfo {
    pub fn is_loopback(&self) -> bool {
        self.kind == InterfaceKind::Loopback || self.name.eq_ignore_ascii_case("lo")
    }

    pub fn is_tunnel(&self) -> bool {
        self.kind == InterfaceKind::Tunnel
    }

    pub fn is_point_to_point(&self) -> bool {
        self.kind == InterfaceKind::PointToPoint
    }

    /// Virtual adapters (VPN, hypervisor, container, Bluetooth) are the ones that
    /// make a device visible but unreachable when they are advertised. They are
    /// only *deprioritised*, never excluded: a host whose only path is a VPN
    /// still gets a listener, and the UI explains the choice.
    pub fn is_virtual(&self) -> bool {
        if self.is_loopback() {
            return true;
        }
        let name = self.name.to_ascii_lowercase();
        let description = self
            .description
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        VIRTUAL_HINTS
            .iter()
            .any(|hint| name.starts_with(hint) || description.contains(hint))
    }

    /// The address this interface would contribute to an automatic pick:
    /// a private IPv4 address first, then a unique-local or global IPv6
    /// address, then a link-local address, then whatever is left.
    pub fn preferred_address(&self) -> Option<AddressInfo> {
        let rank = |address: &AddressInfo| match address.address {
            IpAddr::V4(address) if address.is_private() || address.is_loopback() => 0,
            IpAddr::V4(_) => 3,
            IpAddr::V6(address) if address.is_loopback() => 1,
            IpAddr::V6(address) if is_link_local_v6(address) => 2,
            IpAddr::V6(_) => 1,
        };
        self.addresses
            .iter()
            .copied()
            .filter(|address| !address.address.is_unspecified())
            .min_by_key(|address| rank(address))
    }

    /// The name the mDNS layer (mdns-sd over if-addrs) uses for this
    /// interface. On Windows if-addrs reports the adapter's friendly name while
    /// the platform identifier is an adapter GUID, so the two libraries
    /// disagree; everywhere else they agree on the platform name.
    pub fn discovery_name(&self) -> &str {
        #[cfg(windows)]
        if !self.label.is_empty() {
            return &self.label;
        }
        &self.name
    }

    /// Whether a user-supplied identifier names this interface. Both the
    /// platform name and the friendly name are accepted, because the friendly
    /// name is what the UI shows and the platform name is what the OS uses
    /// (on Windows it is an adapter GUID).
    pub fn matches(&self, id: &str) -> bool {
        self.name.eq_ignore_ascii_case(id) || self.label.eq_ignore_ascii_case(id)
    }

    pub fn address(&self, wanted: IpAddr) -> Option<AddressInfo> {
        self.addresses
            .iter()
            .copied()
            .find(|address| address.address == wanted)
    }
}

/// A chosen local address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub interface: InterfaceInfo,
    pub address: IpAddr,
    pub source: SelectionSource,
}

impl Selection {
    /// The scope id the socket needs: the interface index for link-local IPv6,
    /// and zero otherwise.
    pub fn scope_id(&self) -> u32 {
        match self.address {
            IpAddr::V6(address) if is_link_local_v6(address) => self.interface.index,
            _ => 0,
        }
    }

    /// The address as the UI shows it: "192.168.1.20", or "fe80::1%Wi-Fi".
    pub fn display_address(&self) -> String {
        format_address(
            self.address,
            is_link_local_v6_addr(self.address).then_some(self.interface.label.as_str()),
        )
    }

    pub fn socket_addr(&self, port: u16) -> SocketAddr {
        match self.address {
            IpAddr::V4(address) => SocketAddr::from((address, port)),
            IpAddr::V6(address) => {
                SocketAddr::V6(SocketAddrV6::new(address, port, 0, self.scope_id()))
            }
        }
    }

    /// The interface a socket should be pinned to.
    pub fn binding(&self) -> InterfaceBinding {
        InterfaceBinding {
            index: self.interface.index,
            name: self.interface.name.clone(),
            ipv6: self.address.is_ipv6(),
        }
    }
}

impl fmt::Display for Selection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} on {} ({})",
            self.display_address(),
            self.interface.label,
            self.source.as_str()
        )
    }
}

/// The interface a socket is pinned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceBinding {
    pub index: u32,
    pub name: String,
    pub ipv6: bool,
}

/// Why no address could be chosen. Every variant maps to the plan's 2003
/// "network unavailable" code so the UI can offer an actionable fix.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SelectionError {
    #[error("no usable LAN interface: {0}")]
    Unavailable(String),
    #[error("interface {0} is not present")]
    MissingInterface(String),
    #[error("{address} is not an address of {interface}")]
    MissingAddress { address: String, interface: String },
    #[error("loopback addresses require the development loopback switch")]
    LoopbackNotAllowed,
}

impl From<SelectionError> for Error {
    fn from(value: SelectionError) -> Self {
        Error::new(ErrorCode::NetworkUnavailable, value.to_string(), true)
    }
}

/// Every interface this host has, ordered by index.
pub fn enumerate() -> Result<Vec<InterfaceInfo>, SelectionError> {
    let mut interfaces: Vec<InterfaceInfo> = get_interfaces().into_iter().map(convert).collect();
    interfaces.sort_by(|left, right| {
        left.index
            .cmp(&right.index)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(interfaces)
}

/// The interface that owns the default route, when the platform reports one.
pub fn default_route_interface() -> Option<String> {
    get_default_interface_name()
}

fn convert(interface: default_net::interface::Interface) -> InterfaceInfo {
    let up = interface.is_up();
    let kind = match interface.if_type {
        InterfaceType::Loopback => InterfaceKind::Loopback,
        InterfaceType::Ethernet
        | InterfaceType::Ethernet3Megabit
        | InterfaceType::FastEthernetT
        | InterfaceType::FastEthernetFx
        | InterfaceType::GigabitEthernet => InterfaceKind::Ethernet,
        InterfaceType::Wireless80211 | InterfaceType::Wman | InterfaceType::Wwanpp => {
            InterfaceKind::Wireless
        }
        InterfaceType::Tunnel => InterfaceKind::Tunnel,
        InterfaceType::Ppp | InterfaceType::Slip => InterfaceKind::PointToPoint,
        _ => InterfaceKind::Other,
    };
    let name = interface.name;
    let mut addresses = interface
        .ipv4
        .iter()
        .map(|address| AddressInfo::new(IpAddr::V4(address.addr)))
        .chain(
            interface
                .ipv6
                .iter()
                .map(|address| AddressInfo::new(IpAddr::V6(address.addr))),
        )
        .collect::<Vec<_>>();
    addresses.sort_by_key(|address| address.address);
    addresses.dedup_by_key(|address| address.address);
    let label = interface
        .friendly_name
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| name.clone());
    let description = interface.description.filter(|text| !text.trim().is_empty());
    InterfaceInfo {
        name,
        label,
        description,
        index: interface.index,
        kind,
        up,
        addresses,
    }
}

/// Interfaces that could carry LAN traffic, best candidate first. The UI uses
/// this order for its list, and the picker walks it from the front.
pub fn display_order(
    interfaces: &[InterfaceInfo],
    default_route: Option<&str>,
) -> Vec<InterfaceInfo> {
    let mut ordered = interfaces
        .iter()
        .filter(|interface| interface.up && !interface.addresses.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    ordered.sort_by_key(|interface| candidate_rank(interface, default_route));
    ordered
}

/// Names and descriptions that identify a virtual adapter. Matching is a
/// prefix on the interface name and a substring on the platform description, so
/// "vEthernet (Default Switch)" and "Hyper-V Virtual Ethernet Adapter" both
/// match while a real adapter named "Ethernet" does not.
const VIRTUAL_HINTS: &[&str] = &[
    "vethernet",
    "hyper-v",
    "vmware",
    "vmnet",
    "virtualbox",
    "vbox",
    "docker",
    "br-",
    "virbr",
    "veth",
    "wsl",
    "tailscale",
    "zerotier",
    "nordlynx",
    "wireguard",
    "openvpn",
    "softether",
    "anyconnect",
    "globalprotect",
    "fortinet",
    "sangfor",
    "easyconnect",
    "clash",
    "v2ray",
    "sing-box",
    "wintun",
    "tap",
    "tun",
    "utun",
    "wg",
    "ppp",
    "hamachi",
    "radmin",
    "npcap",
    "bluetooth",
];

/// The smallest key wins: a real adapter before a virtual one, then a
/// LAN-capable kind, then the default route, then a private address, then the
/// platform index for a stable order.
fn candidate_rank(
    interface: &InterfaceInfo,
    default_route: Option<&str>,
) -> (u8, u8, u8, u8, u32, String) {
    let is_default = default_route.is_some_and(|name| name.eq_ignore_ascii_case(&interface.name));
    let preferred = interface.preferred_address();
    let kind_rank = match interface.kind {
        InterfaceKind::Ethernet | InterfaceKind::Wireless => 0,
        InterfaceKind::Other => 1,
        InterfaceKind::PointToPoint => 2,
        InterfaceKind::Tunnel => 3,
        InterfaceKind::Loopback => 4,
    };
    let address_rank = match preferred {
        Some(address) if address.private && address.address.is_ipv4() => 0,
        Some(address) if address.private => 1,
        Some(_) => 2,
        None => 3,
    };
    (
        u8::from(interface.is_virtual()),
        kind_rank,
        u8::from(!is_default),
        address_rank,
        interface.index,
        interface.name.clone(),
    )
}

/// The automatic pick from the plan: the default-route interface first, then a
/// private IPv4 address, Ethernet and Wi-Fi ranked equally, and a tunnel only
/// when nothing else is usable.
pub fn select_auto(
    interfaces: &[InterfaceInfo],
    default_route: Option<&str>,
    allow_loopback: bool,
) -> Result<Selection, SelectionError> {
    let ordered = display_order(interfaces, default_route);
    let lan = ordered
        .iter()
        .find(|interface| {
            !interface.is_loopback() && !interface.is_point_to_point() && !interface.is_tunnel()
        })
        .or_else(|| {
            // A machine whose only usable path is a VPN tunnel still gets a
            // listener; the UI reports the choice.
            ordered.iter().find(|interface| !interface.is_loopback())
        });

    if let Some(interface) = lan
        && let Some(address) = interface.preferred_address()
    {
        let is_default =
            default_route.is_some_and(|name| name.eq_ignore_ascii_case(&interface.name));
        let source = if is_default {
            SelectionSource::DefaultRoute
        } else if address.private && address.address.is_ipv4() {
            SelectionSource::PrivateAddress
        } else {
            SelectionSource::FirstAvailable
        };
        return Ok(Selection {
            interface: interface.clone(),
            address: address.address,
            source,
        });
    }

    // Loopback is never part of the automatic choice; the development switch
    // only makes it a last resort so a machine with no LAN interface can still
    // run two instances.
    if allow_loopback
        && let Some(interface) = ordered.iter().find(|interface| interface.is_loopback())
        && let Some(address) = interface.preferred_address()
    {
        return Ok(Selection {
            interface: interface.clone(),
            address: address.address,
            source: SelectionSource::Loopback,
        });
    }

    Err(SelectionError::Unavailable(
        "connect Ethernet or Wi-Fi, or pick an interface manually".into(),
    ))
}

/// The user's pick, validated against the interfaces that exist right now. A
/// missing interface or address is an error rather than a silent fallback.
pub fn select_manual(
    interfaces: &[InterfaceInfo],
    interface_id: &str,
    address: &str,
    allow_loopback: bool,
) -> Result<Selection, SelectionError> {
    let (wanted, _scope) = parse_address(address)?;
    let interface = interfaces
        .iter()
        .find(|interface| interface.matches(interface_id))
        .ok_or_else(|| SelectionError::MissingInterface(interface_id.to_string()))?;
    if wanted.is_loopback() && !allow_loopback {
        return Err(SelectionError::LoopbackNotAllowed);
    }
    let address = interface
        .address(wanted)
        .ok_or_else(|| SelectionError::MissingAddress {
            address: address.to_string(),
            interface: interface.name.clone(),
        })?;
    Ok(Selection {
        interface: interface.clone(),
        address: address.address,
        source: SelectionSource::Manual,
    })
}

/// Parse an address that may carry a link-local scope: "fe80::1%Wi-Fi".
pub fn parse_address(text: &str) -> Result<(IpAddr, Option<String>), SelectionError> {
    let trimmed = text.trim();
    let (address, scope) = match trimmed.split_once('%') {
        Some((address, scope)) => (address, Some(scope.to_string())),
        None => (trimmed, None),
    };
    let address = address
        .parse::<IpAddr>()
        .map_err(|_| SelectionError::Unavailable(format!("{trimmed} is not a local address")))?;
    if address.is_unspecified() {
        return Err(SelectionError::Unavailable(format!(
            "{trimmed} is not a concrete address"
        )));
    }
    Ok((address, scope))
}

/// Format an address the way the UI shows it, adding the interface name for a
/// link-local IPv6 address.
pub fn format_address(address: IpAddr, interface: Option<&str>) -> String {
    match (address, interface) {
        (IpAddr::V6(address), Some(interface)) if is_link_local_v6(address) => {
            format!("{address}%{interface}")
        }
        _ => address.to_string(),
    }
}

/// Pin a UDP socket to one interface.
///
/// Returns `Ok(false)` when the platform has no supported option, so the caller
/// can tell the user that routing may still send traffic elsewhere instead of
/// pretending the pin happened.
pub fn bind_socket_to_interface(
    socket: &socket2::Socket,
    binding: &InterfaceBinding,
) -> std::io::Result<bool> {
    platform::bind(socket, binding)
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::os::windows::io::AsRawSocket;

    use socket2::Socket;
    use windows_sys::Win32::Networking::WinSock::{
        IP_UNICAST_IF, IPPROTO_IP, IPPROTO_IPV6, IPV6_UNICAST_IF, setsockopt,
    };

    use super::InterfaceBinding;

    /// Windows pins a socket with IP_UNICAST_IF / IPV6_UNICAST_IF; the value is
    /// the interface index as a 4-byte integer in network byte order. A
    /// host-order value is accepted by nothing and fails with WSAEADDRNOTAVAIL,
    /// which is worth remembering when reading a failed pin in the log.
    pub(super) fn bind(socket: &Socket, binding: &InterfaceBinding) -> io::Result<bool> {
        let value = binding.index.to_be_bytes();
        let (level, name) = if binding.ipv6 {
            (IPPROTO_IPV6, IPV6_UNICAST_IF)
        } else {
            (IPPROTO_IP, IP_UNICAST_IF)
        };
        let rc = unsafe {
            setsockopt(
                socket.as_raw_socket() as usize,
                level,
                name,
                value.as_ptr(),
                value.len() as i32,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod platform {
    use std::io;
    use std::os::unix::io::AsRawFd;

    use socket2::Socket;

    use super::InterfaceBinding;

    /// macOS pins a socket with IP_BOUND_IF / IPV6_BOUND_IF, whose value is the
    /// interface index.
    pub(super) fn bind(socket: &Socket, binding: &InterfaceBinding) -> io::Result<bool> {
        let index = binding.index as libc::c_int;
        let (level, name) = if binding.ipv6 {
            (libc::IPPROTO_IPV6, libc::IPV6_BOUND_IF)
        } else {
            (libc::IPPROTO_IP, libc::IP_BOUND_IF)
        };
        let rc = unsafe {
            libc::setsockopt(
                socket.as_raw_fd(),
                level,
                name,
                std::ptr::from_ref(&index).cast(),
                std::mem::size_of_val(&index) as libc::socklen_t,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod platform {
    use std::io;
    use std::os::unix::io::AsRawFd;

    use socket2::Socket;

    use super::InterfaceBinding;

    /// Linux pins a socket with SO_BINDTODEVICE, which takes the interface name.
    pub(super) fn bind(socket: &Socket, binding: &InterfaceBinding) -> io::Result<bool> {
        let mut name = binding.name.as_bytes().to_vec();
        name.push(0);
        let rc = unsafe {
            libc::setsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_BINDTODEVICE,
                name.as_ptr().cast(),
                name.len() as libc::socklen_t,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }
}

#[cfg(not(any(
    windows,
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
mod platform {
    use std::io;

    use socket2::Socket;

    use super::InterfaceBinding;

    /// This platform has no interface pinning option; the caller keeps working
    /// with plain routing.
    pub(super) fn bind(_socket: &Socket, _binding: &InterfaceBinding) -> io::Result<bool> {
        Ok(false)
    }
}

fn is_link_local_v6_addr(address: IpAddr) -> bool {
    matches!(address, IpAddr::V6(address) if is_link_local_v6(address))
}

fn is_link_local_v6(address: Ipv6Addr) -> bool {
    let octets = address.octets();
    octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80
}

fn is_unique_local_v6(address: Ipv6Addr) -> bool {
    (address.octets()[0] & 0xfe) == 0xfc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(text: &str) -> AddressInfo {
        AddressInfo::new(text.parse().expect("test address"))
    }

    fn interface(
        name: &str,
        index: u32,
        kind: InterfaceKind,
        up: bool,
        addresses: &[&str],
    ) -> InterfaceInfo {
        InterfaceInfo {
            name: name.to_string(),
            label: name.to_string(),
            description: None,
            index,
            kind,
            up,
            addresses: addresses.iter().map(|text| address(text)).collect(),
        }
    }

    fn wifi() -> InterfaceInfo {
        interface(
            "Wi-Fi",
            12,
            InterfaceKind::Wireless,
            true,
            &["192.168.1.20", "fe80::20"],
        )
    }

    fn ethernet() -> InterfaceInfo {
        interface("Ethernet", 7, InterfaceKind::Ethernet, true, &["10.0.0.5"])
    }

    fn vpn() -> InterfaceInfo {
        interface("WireGuard", 30, InterfaceKind::Tunnel, true, &["10.7.0.2"])
    }

    fn hyperv() -> InterfaceInfo {
        interface(
            "vEthernet (Default Switch)",
            21,
            InterfaceKind::Other,
            true,
            &["172.20.48.1"],
        )
    }

    fn loopback() -> InterfaceInfo {
        interface(
            "Loopback",
            1,
            InterfaceKind::Loopback,
            true,
            &["127.0.0.1", "::1"],
        )
    }

    #[test]
    fn the_discovery_name_follows_the_platform_convention() {
        let interface = wifi();
        // mdns-sd matches names through if-addrs, which on Windows reports the
        // friendly name while the platform identifier is an adapter GUID.
        #[cfg(windows)]
        assert_eq!(interface.discovery_name(), interface.label);
        #[cfg(not(windows))]
        assert_eq!(interface.discovery_name(), interface.name);
    }

    #[test]
    fn the_default_route_interface_wins() {
        let interfaces = vec![ethernet(), wifi(), vpn()];
        let selection = select_auto(&interfaces, Some("Wi-Fi"), false).expect("a selection");
        assert_eq!(selection.interface.name, "Wi-Fi");
        assert_eq!(selection.address, "192.168.1.20".parse::<IpAddr>().unwrap());
        assert_eq!(selection.source, SelectionSource::DefaultRoute);
        assert_eq!(selection.display_address(), "192.168.1.20");
    }

    #[test]
    fn ethernet_and_wifi_rank_equally_without_a_default_route() {
        let interfaces = vec![wifi(), ethernet()];
        let selection = select_auto(&interfaces, None, false).expect("a selection");
        // Both are private IPv4 addresses; the platform index keeps the order
        // stable instead of preferring one medium.
        assert_eq!(selection.interface.name, "Ethernet");
        assert_eq!(selection.source, SelectionSource::PrivateAddress);
    }

    #[test]
    fn a_tunnel_is_only_used_when_nothing_else_is_up() {
        let with_lan = vec![vpn(), hyperv(), ethernet()];
        let selection = select_auto(&with_lan, None, false).expect("a selection");
        assert_eq!(selection.interface.name, "Ethernet");

        let tunnel_only = vec![vpn(), loopback()];
        let selection = select_auto(&tunnel_only, None, false).expect("a selection");
        assert_eq!(selection.interface.name, "WireGuard");
        assert_eq!(selection.source, SelectionSource::PrivateAddress);
    }

    #[test]
    fn a_down_interface_is_not_a_candidate() {
        let mut down = ethernet();
        down.up = false;
        let selection = select_auto(&[down, wifi()], None, false).expect("a selection");
        assert_eq!(selection.interface.name, "Wi-Fi");
    }

    #[test]
    fn loopback_needs_the_development_switch_and_never_wins_on_its_own() {
        let interfaces = vec![loopback()];
        assert_eq!(
            select_auto(&interfaces, None, false),
            Err(SelectionError::Unavailable(
                "connect Ethernet or Wi-Fi, or pick an interface manually".into()
            ))
        );
        let selection = select_auto(&interfaces, None, true).expect("a selection");
        assert_eq!(selection.address, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(selection.source, SelectionSource::Loopback);

        // A LAN interface always beats the loopback fallback.
        let selection = select_auto(&[loopback(), ethernet()], None, true).expect("a selection");
        assert_eq!(selection.interface.name, "Ethernet");
    }

    #[test]
    fn manual_selection_is_validated_against_the_live_table() {
        let interfaces = vec![ethernet(), wifi()];
        let selection =
            select_manual(&interfaces, "Wi-Fi", "192.168.1.20", false).expect("a selection");
        assert_eq!(selection.address, "192.168.1.20".parse::<IpAddr>().unwrap());
        assert_eq!(selection.source, SelectionSource::Manual);

        let missing_interface = select_manual(&interfaces, "Ethernet 2", "10.0.0.5", false)
            .expect_err("a missing interface");
        assert_eq!(
            missing_interface,
            SelectionError::MissingInterface("Ethernet 2".into())
        );
        assert_eq!(
            missing_interface.to_string(),
            "interface Ethernet 2 is not present"
        );

        let missing_address = select_manual(&interfaces, "Ethernet", "10.0.0.9", false)
            .expect_err("a missing address");
        assert_eq!(
            missing_address,
            SelectionError::MissingAddress {
                address: "10.0.0.9".into(),
                interface: "Ethernet".into()
            }
        );

        // Every validation failure is the plan's 2003 code.
        let error: Error = missing_address.into();
        assert_eq!(error.code, ErrorCode::NetworkUnavailable);
        assert_eq!(error.code_number, 2003);
    }

    #[test]
    fn a_manual_loopback_pick_still_needs_the_development_switch() {
        let interfaces = vec![loopback()];
        let error = select_manual(&interfaces, "Loopback", "127.0.0.1", false)
            .expect_err("loopback is off");
        assert_eq!(error, SelectionError::LoopbackNotAllowed);
        let selection =
            select_manual(&interfaces, "Loopback", "127.0.0.1", true).expect("a selection");
        assert_eq!(selection.interface.name, "Loopback");
    }

    #[test]
    fn a_link_local_address_carries_its_scope() {
        let interfaces = vec![wifi()];
        let selection =
            select_manual(&interfaces, "Wi-Fi", "fe80::20%Wi-Fi", false).expect("a selection");
        assert_eq!(selection.scope_id(), 12);
        assert_eq!(selection.display_address(), "fe80::20%Wi-Fi");
        match selection.socket_addr(42420) {
            SocketAddr::V6(address) => {
                assert_eq!(address.port(), 42420);
                assert_eq!(address.scope_id(), 12);
            }
            SocketAddr::V4(_) => panic!("expected an IPv6 socket address"),
        }
        // A global address needs no scope.
        let selection =
            select_manual(&interfaces, "Wi-Fi", "192.168.1.20", false).expect("a selection");
        assert_eq!(selection.scope_id(), 0);
        assert_eq!(selection.socket_addr(1).to_string(), "192.168.1.20:1");
    }

    #[test]
    fn addresses_round_trip_through_the_scoped_form() {
        assert_eq!(
            parse_address(" fe80::20%Wi-Fi ").expect("parsed"),
            (
                "fe80::20".parse::<IpAddr>().unwrap(),
                Some("Wi-Fi".to_string())
            )
        );
        assert_eq!(
            parse_address("192.168.1.20").expect("parsed"),
            ("192.168.1.20".parse::<IpAddr>().unwrap(), None)
        );
        assert!(parse_address("not-an-address").is_err());
        assert!(parse_address("0.0.0.0").is_err());
        assert_eq!(
            format_address("fe80::20".parse().unwrap(), Some("Wi-Fi")),
            "fe80::20%Wi-Fi"
        );
        assert_eq!(
            format_address("192.168.1.20".parse().unwrap(), Some("Wi-Fi")),
            "192.168.1.20"
        );
    }

    #[test]
    fn a_virtual_adapter_does_not_win_over_a_real_one() {
        // Hyper-V reports an Ethernet-typed virtual adapter that even owns the
        // default route; the name and description are the only signals the
        // platform gives, and they are enough to prefer the real Wi-Fi link.
        let hyperv = InterfaceInfo {
            description: Some("Hyper-V Virtual Ethernet Adapter".into()),
            ..interface(
                "vEthernet (Default Switch)",
                21,
                InterfaceKind::Ethernet,
                true,
                &["172.20.48.1"],
            )
        };
        let selection = select_auto(
            &[hyperv.clone(), wifi()],
            Some("vEthernet (Default Switch)"),
            false,
        )
        .expect("a selection");
        assert_eq!(selection.interface.name, "Wi-Fi");
        assert!(hyperv.is_virtual());
        assert!(!wifi().is_virtual());
        assert!(!ethernet().is_virtual());

        // A host whose only path is virtual still gets a listener.
        let selection = select_auto(&[hyperv], None, false).expect("a selection");
        assert_eq!(selection.interface.name, "vEthernet (Default Switch)");
    }

    #[test]
    fn the_display_order_puts_the_best_candidate_first() {
        let interfaces = vec![loopback(), hyperv(), vpn(), wifi(), ethernet()];
        let ordered = display_order(&interfaces, Some("Ethernet"));
        assert_eq!(
            ordered.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec![
                "Ethernet",
                "Wi-Fi",
                "vEthernet (Default Switch)",
                "WireGuard",
                "Loopback"
            ]
        );
    }

    #[test]
    fn this_machine_reports_interfaces() {
        let interfaces = enumerate().expect("interface enumeration");
        assert!(
            interfaces.iter().any(|interface| interface.is_loopback()),
            "every machine has a loopback interface"
        );
        for interface in &interfaces {
            assert!(!interface.name.is_empty());
            assert!(!interface.label.is_empty());
        }
    }

    #[test]
    fn a_socket_can_be_pinned_to_an_interface() {
        let Ok(interfaces) = enumerate() else {
            return;
        };
        // A real unicast interface, not loopback: Windows rejects
        // IP_UNICAST_IF for the loopback adapter with WSAEADDRNOTAVAIL, which is
        // exactly why the runtime treats a rejected pin as a warning.
        let Some(candidate) = select_auto(&interfaces, default_route_interface().as_deref(), false)
            .ok()
            .filter(|selection| selection.address.is_ipv4())
        else {
            return;
        };
        let binding = candidate.binding();
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )
        .expect("socket");
        // The pin must be accepted on every platform this ships to; a platform
        // without the option reports false instead of failing.
        let pinned = bind_socket_to_interface(&socket, &binding);
        assert!(pinned.is_ok(), "the pin was rejected: {pinned:?}");
        assert!(pinned.unwrap());
    }
}
