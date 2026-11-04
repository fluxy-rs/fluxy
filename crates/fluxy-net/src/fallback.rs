//! UDP discovery fallback for links where mDNS multicast is filtered.
//!
//! The fallback sends a small advertisement to the limited broadcast address,
//! each directed subnet broadcast, and the default gateway. A unicast probe to
//! the gateway makes a hotspot host answer even when client isolation blocks
//! broader broadcast. Every response is still only a hint: the normal QUIC
//! certificate pin and SAS confirmation decide whether the peer is trusted.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use if_addrs::{IfAddr, get_if_addrs};
use thiserror::Error;
use tokio::net::UdpSocket;
use uuid::Uuid;

use fluxy_proto::PROTOCOL_VERSION;

use crate::DISCOVERY_PORT;
use crate::discovery::{PeerAdvertisement, PeerRecord, sanitize_text};

pub const FALLBACK_MAGIC: &[u8] = b"FLXYUDP1";
/// Bumped when the packet carries hardware hints. A v2 packet still parses as
/// v1 up to the platform field, so this is a "readers accept older, ignore
/// newer" arrangement: an old peer drops our packets and falls back to mDNS,
/// which is exactly the behaviour it had before hints existed.
const FALLBACK_WIRE_VERSION: u8 = 2;
/// Hints are display-only; 48 bytes covers "Windows 11 Pro (26H2)" with room.
const MAX_HINT_BYTES: usize = 48;
const MAX_DATAGRAM: usize = 512;
const MAX_NAME_BYTES: usize = 64;
const MAX_PLATFORM_BYTES: usize = 32;
const MAX_PENDING_NONCES: usize = 64;
const MAX_RATE_ENTRIES: usize = 128;
const RESPONSE_MIN_INTERVAL: Duration = Duration::from_millis(500);
const PENDING_NONCE_TTL: Duration = Duration::from_secs(30);
const PRUNE_INTERVAL: Duration = Duration::from_secs(1);

/// The mDNS peer TTL is deliberately longer than the fallback TTL: a UDP
/// advertisement is cheap to repeat, while a lost hotspot peer should disappear
/// quickly enough for the nearby list to stay honest.
pub const FALLBACK_PEER_TTL: Duration = Duration::from_secs(30);
pub const FALLBACK_PROBE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum FallbackError {
    #[error("UDP fallback socket failed: {0}")]
    Io(#[from] io::Error),
    #[error("selected interface is unavailable: {0}")]
    Interface(String),
    #[error("invalid UDP fallback packet: {0}")]
    Packet(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketKind {
    Probe,
    Announce,
    Response,
}

impl PacketKind {
    fn as_u8(self) -> u8 {
        match self {
            Self::Probe => 1,
            Self::Announce => 2,
            Self::Response => 3,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Probe),
            2 => Some(Self::Announce),
            3 => Some(Self::Response),
            _ => None,
        }
    }
}

/// A single bounded UDP datagram. It carries the same non-secret fields as the
/// mDNS TXT record, never a key, token, message, or clipboard body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackPacket {
    pub kind: PacketKind,
    pub device_id: Uuid,
    pub nonce: [u8; 16],
    pub port: u16,
    pub cert_fingerprint: [u8; 32],
    pub protocol_version: u16,
    pub name: String,
    pub platform: String,
    /// Display-only hardware hints; empty on a version 1 packet.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
}

impl FallbackPacket {
    pub fn new(
        kind: PacketKind,
        device_id: Uuid,
        nonce: [u8; 16],
        port: u16,
        cert_fingerprint: [u8; 32],
        name: &str,
        platform: &str,
    ) -> Self {
        let info = fluxy_sysinfo::cached();
        Self {
            kind,
            device_id,
            nonce,
            port,
            cert_fingerprint,
            protocol_version: PROTOCOL_VERSION as u16,
            name: sanitize_text(name, MAX_NAME_BYTES),
            platform: sanitize_text(platform, MAX_PLATFORM_BYTES),
            manufacturer: info.manufacturer.clone(),
            model: info.model.clone(),
            os_version: info.display_os(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let sanitized_name = sanitize_text(&self.name, MAX_NAME_BYTES);
        let name = truncate_utf8(&sanitized_name, MAX_NAME_BYTES);
        let sanitized_platform = sanitize_text(&self.platform, MAX_PLATFORM_BYTES);
        let platform = truncate_utf8(&sanitized_platform, MAX_PLATFORM_BYTES);
        let mut out = Vec::with_capacity(96 + name.len() + platform.len());
        out.extend_from_slice(FALLBACK_MAGIC);
        out.push(FALLBACK_WIRE_VERSION);
        out.push(self.kind.as_u8());
        out.extend_from_slice(self.device_id.as_bytes());
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.port.to_be_bytes());
        out.extend_from_slice(&self.protocol_version.to_be_bytes());
        out.extend_from_slice(&self.cert_fingerprint);
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
        out.push(platform.len() as u8);
        out.extend_from_slice(platform.as_bytes());
        // Wire version 2: three length-prefixed display strings. A v1 reader
        // rejects the packet on the version byte before it ever reaches here.
        for value in [&self.manufacturer, &self.model, &self.os_version] {
            let sanitized = sanitize_text(value, MAX_HINT_BYTES);
            let text = truncate_utf8(&sanitized, MAX_HINT_BYTES);
            out.push(text.len() as u8);
            out.extend_from_slice(text.as_bytes());
        }
        out
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, FallbackError> {
        if bytes.len() < 80 || bytes.len() > MAX_DATAGRAM {
            return Err(FallbackError::Packet("size out of range".into()));
        }
        if !bytes.starts_with(FALLBACK_MAGIC) {
            return Err(FallbackError::Packet("bad magic".into()));
        }
        let version = bytes[FALLBACK_MAGIC.len()];
        if version == 0 || version > FALLBACK_WIRE_VERSION {
            return Err(FallbackError::Packet("unsupported wire version".into()));
        }
        let kind = PacketKind::from_u8(bytes[FALLBACK_MAGIC.len() + 1])
            .ok_or_else(|| FallbackError::Packet("unknown packet kind".into()))?;

        let mut offset = FALLBACK_MAGIC.len() + 2;
        let device_id = Uuid::from_bytes(take_array::<16>(bytes, &mut offset)?);
        if device_id.is_nil() {
            return Err(FallbackError::Packet("nil device id".into()));
        }
        let nonce = take_array::<16>(bytes, &mut offset)?;
        let port = u16::from_be_bytes(take_array::<2>(bytes, &mut offset)?);
        if port == 0 {
            return Err(FallbackError::Packet("zero port".into()));
        }
        let protocol_version = u16::from_be_bytes(take_array::<2>(bytes, &mut offset)?);
        let cert_fingerprint = take_array::<32>(bytes, &mut offset)?;
        if cert_fingerprint.iter().all(|byte| *byte == 0) {
            return Err(FallbackError::Packet("empty fingerprint".into()));
        }
        let name_len = take_array::<1>(bytes, &mut offset)?[0] as usize;
        if name_len > MAX_NAME_BYTES {
            return Err(FallbackError::Packet("name too long".into()));
        }
        let name = std::str::from_utf8(take_slice(bytes, &mut offset, name_len)?)
            .map_err(|_| FallbackError::Packet("name is not UTF-8".into()))?;
        let platform_len = take_array::<1>(bytes, &mut offset)?[0] as usize;
        if platform_len > MAX_PLATFORM_BYTES {
            return Err(FallbackError::Packet("platform too long".into()));
        }
        let platform = std::str::from_utf8(take_slice(bytes, &mut offset, platform_len)?)
            .map_err(|_| FallbackError::Packet("platform is not UTF-8".into()))?;
        let mut hints = [String::new(), String::new(), String::new()];
        if version >= 2 {
            for slot in hints.iter_mut() {
                let len = take_array::<1>(bytes, &mut offset)?[0] as usize;
                if len > MAX_HINT_BYTES {
                    return Err(FallbackError::Packet("hint too long".into()));
                }
                let text = std::str::from_utf8(take_slice(bytes, &mut offset, len)?)
                    .map_err(|_| FallbackError::Packet("hint is not UTF-8".into()))?;
                *slot = sanitize_text(text, MAX_HINT_BYTES);
            }
        }
        if offset != bytes.len() {
            return Err(FallbackError::Packet("trailing bytes".into()));
        }

        let [manufacturer, model, os_version] = hints;
        Ok(Self {
            kind,
            device_id,
            nonce,
            port,
            cert_fingerprint,
            protocol_version,
            name: sanitize_text(name, MAX_NAME_BYTES),
            platform: sanitize_text(platform, MAX_PLATFORM_BYTES),
            manufacturer,
            model,
            os_version,
        })
    }

    fn to_advertisement(
        &self,
        source: IpAddr,
        allow_loopback: bool,
        interface: Option<String>,
    ) -> Result<PeerAdvertisement, FallbackError> {
        if self.protocol_version as u32 != PROTOCOL_VERSION {
            return Err(FallbackError::Packet(
                "incompatible protocol version".into(),
            ));
        }
        if source.is_unspecified() || source.is_multicast() {
            return Err(FallbackError::Packet("unusable source address".into()));
        }
        if source.is_loopback() && !allow_loopback {
            return Err(FallbackError::Packet("loopback is disabled".into()));
        }
        Ok(PeerAdvertisement {
            device_id: self.device_id,
            // Wire version 2 carries the hints; a version 1 sender leaves them
            // empty and the UI degrades to the platform name.
            manufacturer: self.manufacturer.clone(),
            model: self.model.clone(),
            os_version: self.os_version.clone(),
            name: if self.name.is_empty() {
                "Fluxy device".into()
            } else {
                self.name.clone()
            },
            platform: if self.platform.is_empty() {
                "unknown".into()
            } else {
                self.platform.clone()
            },
            protocol_version: self.protocol_version as u32,
            features: vec!["chat".into()],
            cert_fingerprint: self.cert_fingerprint,
            addresses: vec![source],
            port: self.port,
            interface,
        })
    }
}

fn take_slice<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], FallbackError> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| FallbackError::Packet("length overflow".into()))?;
    if end > bytes.len() {
        return Err(FallbackError::Packet("truncated packet".into()));
    }
    let slice = &bytes[*offset..end];
    *offset = end;
    Ok(slice)
}

fn take_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], FallbackError> {
    let slice = take_slice(bytes, offset, N)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[derive(Debug, Clone)]
pub struct FallbackConfig {
    /// UDP port to bind. Zero asks the OS for an ephemeral test port.
    pub bind_port: u16,
    pub interface: Option<String>,
    pub allow_loopback: bool,
    pub peer_ttl: Duration,
    pub probe_interval: Duration,
    /// Send announcements to 255.255.255.255 and directed subnet broadcasts.
    pub broadcast: bool,
    /// Default gateway. Callers normally pass default_gateway().
    pub gateway: Option<Ipv4Addr>,
    /// Extra unicast probe targets, used by tests and manual diagnostics.
    pub probe_targets: Vec<SocketAddr>,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            bind_port: DISCOVERY_PORT,
            interface: None,
            allow_loopback: false,
            peer_ttl: FALLBACK_PEER_TTL,
            probe_interval: FALLBACK_PROBE_INTERVAL,
            broadcast: true,
            gateway: None,
            probe_targets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FallbackDiagnostics {
    pub bound_addr: Option<String>,
    pub gateway: Option<String>,
    pub targets: Vec<String>,
    pub probes_sent: u64,
    pub announcements_sent: u64,
    pub announcements_received: u64,
    pub probes_received: u64,
    pub responses_received: u64,
    pub rejected: u64,
    pub rate_limited: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FallbackEvent {
    Found(Box<PeerRecord>),
    Lost(Uuid),
}
pub struct FallbackDiscovery {
    socket: UdpSocket,
    own_id: Uuid,
    name: String,
    platform: String,
    port: u16,
    cert_fingerprint: [u8; 32],
    allow_loopback: bool,
    interface: Option<String>,
    peer_ttl: Duration,
    probe_interval: Duration,
    broadcast_targets: Vec<SocketAddr>,
    unicast_targets: Vec<SocketAddr>,
    next_probe: Instant,
    table: HashMap<Uuid, TableEntry>,
    pending: HashMap<[u8; 16], Instant>,
    responses: HashMap<IpAddr, Instant>,
    diagnostics: Arc<Mutex<FallbackDiagnostics>>,
}

#[derive(Debug, Clone)]
struct TableEntry {
    record: PeerRecord,
}

impl FallbackDiscovery {
    pub async fn start(
        device_id: Uuid,
        name: &str,
        platform: &str,
        port: u16,
        cert_fingerprint: [u8; 32],
        config: FallbackConfig,
        diagnostics: Arc<Mutex<FallbackDiagnostics>>,
    ) -> Result<Self, FallbackError> {
        if port == 0 {
            return Err(FallbackError::Packet("zero advertised port".into()));
        }
        let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.bind_port);
        let mut attempts = 0;
        let socket = loop {
            match UdpSocket::bind(bind).await {
                Ok(socket) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse && attempts < 10 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => return Err(error.into()),
            }
        };
        socket.set_broadcast(true)?;
        let local = socket.local_addr()?;

        let broadcast_targets = if config.broadcast {
            collect_broadcast_targets(&config, local.port())?
        } else {
            Vec::new()
        };
        let mut unicast_targets = Vec::new();
        if let Some(gateway) = config.gateway
            && !gateway.is_unspecified()
            && !gateway.is_broadcast()
        {
            unicast_targets.push(SocketAddr::new(IpAddr::V4(gateway), local.port()));
        }
        unicast_targets.extend(config.probe_targets.iter().copied());
        unicast_targets.sort();
        unicast_targets.dedup();

        let gateway = config.gateway.map(|value| value.to_string());
        let targets = broadcast_targets
            .iter()
            .chain(unicast_targets.iter())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if let Ok(mut stats) = diagnostics.lock() {
            stats.bound_addr = Some(local.to_string());
            stats.gateway = gateway;
            stats.targets = targets;
            stats.last_error = None;
        }

        let mut discovery = Self {
            socket,
            own_id: device_id,
            name: sanitize_text(name, MAX_NAME_BYTES),
            platform: sanitize_text(platform, MAX_PLATFORM_BYTES),
            port,
            cert_fingerprint,
            allow_loopback: config.allow_loopback,
            interface: config.interface,
            peer_ttl: config.peer_ttl,
            probe_interval: config.probe_interval.max(Duration::from_millis(50)),
            broadcast_targets,
            unicast_targets,
            next_probe: Instant::now(),
            table: HashMap::new(),
            pending: HashMap::new(),
            responses: HashMap::new(),
            diagnostics,
        };
        discovery.send_round().await;
        Ok(discovery)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn diagnostics(&self) -> FallbackDiagnostics {
        self.diagnostics
            .lock()
            .map(|stats| stats.clone())
            .unwrap_or_default()
    }

    pub async fn next_event(&mut self) -> Option<FallbackEvent> {
        loop {
            let now = Instant::now();
            let until_probe = self.next_probe.saturating_duration_since(now);
            let wait = until_probe.min(PRUNE_INTERVAL);
            tokio::select! {
                received = self.receive_one() => {
                    if let Some(event) = received {
                        return Some(event);
                    }
                }
                _ = tokio::time::sleep(wait) => {
                    if Instant::now() >= self.next_probe {
                        self.send_round().await;
                    }
                    if let Some(device_id) = self.prune() {
                        return Some(FallbackEvent::Lost(device_id));
                    }
                }
            }
        }
    }

    async fn receive_one(&mut self) -> Option<FallbackEvent> {
        let mut buffer = [0u8; MAX_DATAGRAM + 1];
        let (length, source) = match self.socket.recv_from(&mut buffer).await {
            Ok(value) => value,
            Err(error) => {
                self.set_error(&format!("receive failed: {error}"));
                tokio::time::sleep(Duration::from_millis(100)).await;
                return None;
            }
        };
        if length > MAX_DATAGRAM {
            self.increment(|stats| stats.rejected += 1);
            self.set_error("oversized datagram");
            return None;
        }
        let packet = match FallbackPacket::decode(&buffer[..length]) {
            Ok(packet) => packet,
            Err(error) => {
                self.increment(|stats| stats.rejected += 1);
                self.set_error(&error.to_string());
                return None;
            }
        };
        if packet.device_id == self.own_id {
            return None;
        }
        if packet.kind == PacketKind::Response && self.pending.remove(&packet.nonce).is_none() {
            self.increment(|stats| stats.rejected += 1);
            return None;
        }
        let advertisement =
            match packet.to_advertisement(source.ip(), self.allow_loopback, self.interface.clone())
            {
                Ok(value) => value,
                Err(error) => {
                    self.increment(|stats| stats.rejected += 1);
                    self.set_error(&error.to_string());
                    return None;
                }
            };

        match packet.kind {
            PacketKind::Probe => {
                self.increment(|stats| stats.probes_received += 1);
                if self.should_respond(source.ip(), Instant::now()) {
                    self.send_response(&packet, source).await;
                }
            }
            PacketKind::Announce => {
                self.increment(|stats| stats.announcements_received += 1);
            }
            PacketKind::Response => {
                self.increment(|stats| stats.responses_received += 1);
            }
        }

        let now = Instant::now();
        let first_seen = self
            .table
            .get(&advertisement.device_id)
            .map(|entry| entry.record.first_seen)
            .unwrap_or(now);
        let record = PeerRecord {
            device_id: advertisement.device_id,
            name: advertisement.name,
            platform: advertisement.platform,
            manufacturer: advertisement.manufacturer,
            model: advertisement.model,
            os_version: advertisement.os_version,
            protocol_version: advertisement.protocol_version,
            features: advertisement.features,
            cert_fingerprint: advertisement.cert_fingerprint,
            addresses: advertisement.addresses,
            port: advertisement.port,
            interface: advertisement.interface,
            first_seen,
            last_seen: now,
            expires_at: now + self.peer_ttl,
        };
        self.table.insert(
            record.device_id,
            TableEntry {
                record: record.clone(),
            },
        );
        Some(FallbackEvent::Found(Box::new(record)))
    }

    async fn send_round(&mut self) {
        self.next_probe = Instant::now() + self.probe_interval;
        let announce = self.own_packet(PacketKind::Announce, [0u8; 16]);
        let bytes = announce.encode();
        for target in self.broadcast_targets.clone() {
            match self.socket.send_to(&bytes, target).await {
                Ok(_) => self.increment(|stats| stats.announcements_sent += 1),
                Err(error) => self.set_error(&format!("announce to {target} failed: {error}")),
            }
        }

        if self.unicast_targets.is_empty() {
            return;
        }
        let nonce = *Uuid::now_v7().as_bytes();
        let probe = self.own_packet(PacketKind::Probe, nonce);
        let bytes = probe.encode();
        self.pending.insert(nonce, Instant::now());
        if self.pending.len() > MAX_PENDING_NONCES {
            let now = Instant::now();
            self.pending
                .retain(|_, sent| now.duration_since(*sent) < PENDING_NONCE_TTL);
        }
        for target in self.unicast_targets.clone() {
            match self.socket.send_to(&bytes, target).await {
                Ok(_) => self.increment(|stats| stats.probes_sent += 1),
                Err(error) => self.set_error(&format!("probe to {target} failed: {error}")),
            }
        }
    }

    async fn send_response(&mut self, probe: &FallbackPacket, target: SocketAddr) {
        let response = self.own_packet(PacketKind::Response, probe.nonce);
        let bytes = response.encode();
        if let Err(error) = self.socket.send_to(&bytes, target).await {
            self.set_error(&format!("response to {target} failed: {error}"));
        }
    }

    fn own_packet(&self, kind: PacketKind, nonce: [u8; 16]) -> FallbackPacket {
        FallbackPacket::new(
            kind,
            self.own_id,
            nonce,
            self.port,
            self.cert_fingerprint,
            &self.name,
            &self.platform,
        )
    }

    fn prune(&mut self) -> Option<Uuid> {
        let now = Instant::now();
        let expired = self
            .table
            .iter()
            .find(|(_, entry)| now >= entry.record.expires_at)
            .map(|(device_id, _)| *device_id)?;
        self.table.remove(&expired);
        Some(expired)
    }

    fn should_respond(&mut self, source: IpAddr, now: Instant) -> bool {
        if self.responses.len() > MAX_RATE_ENTRIES {
            self.responses
                .retain(|_, last| now.duration_since(*last) < Duration::from_secs(5));
        }
        if let Some(last) = self.responses.get(&source)
            && now.duration_since(*last) < RESPONSE_MIN_INTERVAL
        {
            self.increment(|stats| stats.rate_limited += 1);
            return false;
        }
        self.responses.insert(source, now);
        true
    }

    fn increment(&self, update: impl FnOnce(&mut FallbackDiagnostics)) {
        if let Ok(mut stats) = self.diagnostics.lock() {
            update(&mut stats);
        }
    }

    fn set_error(&self, message: &str) {
        if let Ok(mut stats) = self.diagnostics.lock() {
            stats.last_error = Some(message.to_string());
        }
    }
}

fn ipv4_broadcast(ip: Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    let octets = ip.octets();
    let mask = netmask.octets();
    Ipv4Addr::new(
        octets[0] | !mask[0],
        octets[1] | !mask[1],
        octets[2] | !mask[2],
        octets[3] | !mask[3],
    )
}

fn collect_broadcast_targets(
    config: &FallbackConfig,
    port: u16,
) -> Result<Vec<SocketAddr>, FallbackError> {
    let interfaces = get_if_addrs()?;
    let mut targets = Vec::new();
    let mut selected_found = false;
    for interface in interfaces {
        if let Some(selected) = &config.interface {
            // Accept both the friendly name and the Windows adapter GUID, the same
            // way discovery does.
            let named = &interface.name == selected;
            #[cfg(windows)]
            let named = named || &interface.adapter_name == selected;
            if !named {
                continue;
            }
            selected_found = true;
        }
        if interface.is_loopback() && !config.allow_loopback {
            continue;
        }
        if !interface.is_oper_up() {
            continue;
        }
        if let IfAddr::V4(address) = interface.addr {
            if address.ip.is_unspecified() || address.ip.is_broadcast() {
                continue;
            }
            let broadcast = address
                .broadcast
                .filter(|value| !value.is_unspecified() && !value.is_broadcast())
                .unwrap_or_else(|| ipv4_broadcast(address.ip, address.netmask));
            if !broadcast.is_broadcast() && broadcast != address.ip {
                targets.push(SocketAddr::new(IpAddr::V4(broadcast), port));
            }
        }
    }
    if config.interface.is_some() && !selected_found {
        return Err(FallbackError::Interface(
            config.interface.clone().unwrap_or_default(),
        ));
    }
    targets.sort();
    targets.dedup();
    if config.broadcast && !targets.is_empty() {
        targets.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port));
    }
    Ok(targets)
}

/// Default IPv4 gateway used by the active route. This is only a discovery
/// target; a probe response carries no trust.
pub fn default_gateway() -> Option<Ipv4Addr> {
    let gateway = default_net::get_default_gateway().ok()?;
    match gateway.ip_addr {
        IpAddr::V4(address)
            if !address.is_unspecified() && !address.is_loopback() && !address.is_broadcast() =>
        {
            Some(address)
        }
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn packet(kind: PacketKind) -> FallbackPacket {
        FallbackPacket::new(
            kind,
            Uuid::now_v7(),
            [7; 16],
            42431,
            [9; 32],
            "Studio\n\x1b[31m",
            "windows",
        )
    }

    #[test]
    fn hints_round_trip_through_the_packet() {
        let packet = FallbackPacket::new(
            PacketKind::Announce,
            Uuid::now_v7(),
            [3u8; 16],
            4242,
            [5u8; 32],
            "Studio",
            "windows",
        );
        let decoded = FallbackPacket::decode(&packet.encode()).unwrap();
        let info = fluxy_sysinfo::cached();
        assert_eq!(decoded.manufacturer, info.manufacturer);
        assert_eq!(decoded.model, info.model);
        assert_eq!(decoded.os_version, info.display_os());
    }

    #[test]
    fn a_version_one_packet_still_decodes_without_hints() {
        let packet = FallbackPacket::new(
            PacketKind::Announce,
            Uuid::now_v7(),
            [3u8; 16],
            4242,
            [5u8; 32],
            "Old",
            "linux",
        );
        let mut bytes = packet.encode();
        // Rewrite the header as a v1 packet and drop the appended hints, which
        // is exactly what a build from before this change would have sent.
        bytes[FALLBACK_MAGIC.len()] = 1;
        let platform_end = {
            let mut offset = FALLBACK_MAGIC.len() + 2 + 16 + 16 + 2 + 2 + 32;
            let name_len = bytes[offset] as usize;
            offset += 1 + name_len;
            offset + 1 + bytes[offset] as usize
        };
        bytes.truncate(platform_end);

        let decoded = FallbackPacket::decode(&bytes).unwrap();
        assert_eq!(decoded.name, "Old");
        assert_eq!(decoded.platform, "linux");
        assert!(decoded.manufacturer.is_empty());
        assert!(decoded.model.is_empty());
        assert!(decoded.os_version.is_empty());
    }

    #[test]
    fn packet_round_trips_and_strips_controls() {
        let packet = packet(PacketKind::Probe);
        let decoded = FallbackPacket::decode(&packet.encode()).unwrap();
        assert_eq!(decoded, packet);
        assert!(!decoded.name.contains('\n'));
        assert!(!decoded.name.contains('\x1b'));
    }

    #[test]
    fn malformed_packets_are_rejected() {
        assert!(FallbackPacket::decode(b"bad").is_err());
        let packet = packet(PacketKind::Announce);
        let mut encoded = packet.encode();
        encoded.push(0);
        assert!(FallbackPacket::decode(&encoded).is_err());
        encoded.pop();
        let mut oversized = packet.encode();
        oversized.resize(MAX_DATAGRAM + 1, 0);
        assert!(FallbackPacket::decode(&oversized).is_err());
        let index = FALLBACK_MAGIC.len() + 2 + 16 + 16;
        encoded[index] = 0;
        encoded[index + 1] = 0;
        assert!(FallbackPacket::decode(&encoded).is_err());
    }

    #[test]
    fn incompatible_protocol_is_rejected_as_an_advertisement() {
        let mut packet = packet(PacketKind::Announce);
        packet.protocol_version = 999;
        assert!(matches!(
            packet.to_advertisement("192.168.137.2".parse().unwrap(), false, None),
            Err(FallbackError::Packet(_))
        ));
    }

    #[test]
    fn broadcast_uses_the_interface_mask() {
        assert_eq!(
            ipv4_broadcast(
                "192.168.137.10".parse().unwrap(),
                "255.255.255.0".parse().unwrap()
            ),
            "192.168.137.255".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(
            ipv4_broadcast("10.0.0.5".parse().unwrap(), "255.255.0.0".parse().unwrap()),
            "10.0.255.255".parse::<Ipv4Addr>().unwrap()
        );
    }

    #[tokio::test]
    async fn loopback_probe_finds_peer_and_replies() {
        let second_id = Uuid::now_v7();
        let second_stats = Arc::new(Mutex::new(FallbackDiagnostics::default()));
        let mut second = FallbackDiscovery::start(
            second_id,
            "second",
            "linux",
            42432,
            [2; 32],
            FallbackConfig {
                bind_port: 0,
                allow_loopback: true,
                peer_ttl: Duration::from_secs(5),
                probe_interval: Duration::from_secs(3600),
                broadcast: false,
                gateway: None,
                probe_targets: Vec::new(),
                ..FallbackConfig::default()
            },
            second_stats,
        )
        .await
        .unwrap();
        let second_port = second.local_addr().unwrap().port();

        let first_id = Uuid::now_v7();
        let first_stats = Arc::new(Mutex::new(FallbackDiagnostics::default()));
        let mut first = FallbackDiscovery::start(
            first_id,
            "first",
            "windows",
            42431,
            [1; 32],
            FallbackConfig {
                bind_port: 0,
                allow_loopback: true,
                peer_ttl: Duration::from_secs(5),
                probe_interval: Duration::from_secs(3600),
                broadcast: false,
                gateway: None,
                probe_targets: vec![SocketAddr::new("127.0.0.1".parse().unwrap(), second_port)],
                ..FallbackConfig::default()
            },
            first_stats,
        )
        .await
        .unwrap();

        let reply = tokio::time::timeout(Duration::from_secs(3), second.next_event())
            .await
            .unwrap()
            .expect("second peer event");
        match reply {
            FallbackEvent::Found(record) => assert_eq!(record.device_id, first_id),
            FallbackEvent::Lost(_) => panic!("unexpected lost event"),
        }

        let found = tokio::time::timeout(Duration::from_secs(3), first.next_event())
            .await
            .unwrap()
            .expect("first peer event");
        match found {
            FallbackEvent::Found(record) => {
                assert_eq!(record.device_id, second_id);
                assert_eq!(record.port, 42432);
                assert_eq!(
                    record.addresses,
                    vec!["127.0.0.1".parse::<IpAddr>().unwrap()]
                );
            }
            FallbackEvent::Lost(_) => panic!("unexpected lost event"),
        }
        assert_eq!(first.diagnostics().probes_sent, 1);
        assert_eq!(first.diagnostics().responses_received, 1);
        assert_eq!(second.diagnostics().probes_received, 1);
    }
}
