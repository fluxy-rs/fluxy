//! mDNS/DNS-SD discovery and the in-memory peer table.
//!
//! Discovery records are hints only. A resolved peer is never trusted until the
//! authenticated QUIC handshake and SAS confirmation succeed.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use if_addrs::get_if_addrs;
use mdns_sd::{IfKind, Receiver, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use thiserror::Error;
use uuid::Uuid;

use fluxy_proto::PROTOCOL_VERSION;

/// DNS-SD service type advertised by Fluxy.
pub const SERVICE_TYPE: &str = "_fluxy._udp.local.";
/// Default time a resolved peer remains online without a refresh.
pub const DEFAULT_PEER_TTL: Duration = Duration::from_secs(120);

pub const TXT_DEVICE_ID: &str = "id";
pub const TXT_NAME: &str = "name";
pub const TXT_PLATFORM: &str = "os";
pub const TXT_VERSION: &str = "v";
pub const TXT_FEATURES: &str = "features";
pub const TXT_FINGERPRINT: &str = "fp";
/// Hardware hints. Display only: absent on older peers, which is why every
/// read has a fallback.
pub const TXT_MANUFACTURER: &str = "mfr";
pub const TXT_MODEL: &str = "model";
pub const TXT_OS_VERSION: &str = "osv";

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mDNS daemon failed: {0}")]
    Daemon(String),
    #[error("no usable network interface for discovery")]
    NoInterface,
    #[error("selected interface is unavailable: {0}")]
    Interface(String),
    #[error("invalid peer advertisement: {0}")]
    Advertisement(String),
    #[error("peer protocol version is incompatible")]
    Incompatible,
}

impl From<mdns_sd::Error> for DiscoveryError {
    fn from(value: mdns_sd::Error) -> Self {
        DiscoveryError::Daemon(value.to_string())
    }
}

/// The subset of a DNS-SD record that Fluxy uses. None of these fields are
/// authentication evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAdvertisement {
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    /// Empty when the peer is an older build that does not advertise them.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
    pub protocol_version: u32,
    pub features: Vec<String>,
    pub cert_fingerprint: [u8; 32],
    pub addresses: Vec<IpAddr>,
    pub port: u16,
    pub interface: Option<String>,
}

/// A display hint that older peers simply do not send.
fn hint(resolved: &ResolvedService, key: &str) -> String {
    resolved
        .get_property_val_str(key)
        .map(|value| sanitize_text(value, 64))
        .unwrap_or_default()
}

impl PeerAdvertisement {
    pub fn from_resolved(
        resolved: &ResolvedService,
        interface: Option<String>,
        allow_loopback: bool,
    ) -> Result<Self, DiscoveryError> {
        let id = resolved
            .get_property_val_str(TXT_DEVICE_ID)
            .ok_or_else(|| DiscoveryError::Advertisement("missing id".into()))?;
        let device_id = Uuid::parse_str(id)
            .map_err(|_| DiscoveryError::Advertisement("invalid device id".into()))?;
        if device_id.is_nil() {
            return Err(DiscoveryError::Advertisement("nil device id".into()));
        }

        let version = resolved
            .get_property_val_str(TXT_VERSION)
            .ok_or_else(|| DiscoveryError::Advertisement("missing version".into()))?
            .parse::<u32>()
            .map_err(|_| DiscoveryError::Advertisement("invalid version".into()))?;
        if version != PROTOCOL_VERSION {
            return Err(DiscoveryError::Incompatible);
        }

        let fingerprint_text = resolved
            .get_property_val_str(TXT_FINGERPRINT)
            .ok_or_else(|| DiscoveryError::Advertisement("missing fingerprint".into()))?;
        let fingerprint_bytes = hex::decode(fingerprint_text)
            .map_err(|_| DiscoveryError::Advertisement("invalid fingerprint".into()))?;
        let cert_fingerprint: [u8; 32] = fingerprint_bytes
            .try_into()
            .map_err(|_| DiscoveryError::Advertisement("invalid fingerprint length".into()))?;

        let name = sanitize_text(
            resolved
                .get_property_val_str(TXT_NAME)
                .unwrap_or("Fluxy device"),
            128,
        );
        let name = if name.is_empty() {
            "Fluxy device".to_string()
        } else {
            name
        };
        let platform = sanitize_text(
            resolved
                .get_property_val_str(TXT_PLATFORM)
                .unwrap_or("unknown"),
            64,
        );
        let manufacturer = hint(resolved, TXT_MANUFACTURER);
        let model = hint(resolved, TXT_MODEL);
        let os_version = hint(resolved, TXT_OS_VERSION);
        let features = resolved
            .get_property_val_str(TXT_FEATURES)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|feature| !feature.is_empty())
            .take(16)
            .map(|feature| sanitize_text(feature, 32))
            .collect::<Vec<_>>();

        let mut addresses = resolved
            .get_addresses()
            .iter()
            .filter(|address| allow_loopback || !address.is_loopback())
            .map(|address| address.to_ip_addr())
            .collect::<Vec<_>>();
        addresses.sort();
        addresses.dedup();
        addresses.truncate(16);
        if addresses.is_empty() {
            return Err(DiscoveryError::Advertisement("no usable address".into()));
        }
        if resolved.port == 0 {
            return Err(DiscoveryError::Advertisement("zero port".into()));
        }

        Ok(Self {
            device_id,
            name,
            platform,
            manufacturer,
            model,
            os_version,
            protocol_version: version,
            features,
            cert_fingerprint,
            addresses,
            port: resolved.port,
            interface,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRecord {
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    /// Empty when the peer does not advertise them.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
    pub protocol_version: u32,
    pub features: Vec<String>,
    pub cert_fingerprint: [u8; 32],
    pub addresses: Vec<IpAddr>,
    pub port: u16,
    pub interface: Option<String>,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub expires_at: Instant,
}

impl PeerRecord {
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

/// Deduplicates resolved services by stable device id and expires stale peers.
#[derive(Debug, Clone)]
pub struct PeerTable {
    peers: HashMap<Uuid, PeerRecord>,
    ttl: Duration,
}

impl PeerTable {
    pub fn new(ttl: Duration) -> Self {
        Self {
            peers: HashMap::new(),
            ttl,
        }
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn get(&self, device_id: Uuid) -> Option<&PeerRecord> {
        self.peers.get(&device_id)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    pub fn upsert(&mut self, advertisement: PeerAdvertisement) -> PeerRecord {
        self.upsert_at(advertisement, Instant::now())
    }

    fn upsert_at(&mut self, advertisement: PeerAdvertisement, now: Instant) -> PeerRecord {
        match self.peers.get_mut(&advertisement.device_id) {
            Some(record) => {
                record.name = advertisement.name;
                record.platform = advertisement.platform;
                record.protocol_version = advertisement.protocol_version;
                record.features = advertisement.features;
                record.cert_fingerprint = advertisement.cert_fingerprint;
                record.addresses = advertisement.addresses;
                record.port = advertisement.port;
                record.interface = advertisement.interface;
                record.last_seen = now;
                record.expires_at = now + self.ttl;
                record.clone()
            }
            None => {
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
                    first_seen: now,
                    last_seen: now,
                    expires_at: now + self.ttl,
                };
                self.peers.insert(record.device_id, record.clone());
                record
            }
        }
    }

    pub fn remove(&mut self, device_id: Uuid) -> Option<PeerRecord> {
        self.peers.remove(&device_id)
    }

    pub fn prune(&mut self) -> Vec<Uuid> {
        self.prune_at(Instant::now())
    }

    fn prune_at(&mut self, now: Instant) -> Vec<Uuid> {
        let expired = self
            .peers
            .values()
            .filter(|record| record.is_expired(now))
            .map(|record| record.device_id)
            .collect::<Vec<_>>();
        for device_id in &expired {
            self.peers.remove(device_id);
        }
        expired
    }

    pub fn snapshot(&self) -> Vec<PeerRecord> {
        let now = Instant::now();
        let mut records = self
            .peers
            .values()
            .filter(|record| !record.is_expired(now))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.device_id);
        records
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Restrict multicast to one interface by name.
    pub interface: Option<String>,
    /// Required for loopback-only tests and desktop development.
    pub allow_loopback: bool,
    pub peer_ttl: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            interface: None,
            allow_loopback: false,
            peer_ttl: DEFAULT_PEER_TTL,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    Found(Box<PeerRecord>),
    Lost(Uuid),
}

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    receiver: Option<Receiver<ServiceEvent>>,
    own_id: Uuid,
    table: PeerTable,
    by_fullname: HashMap<String, Uuid>,
    allow_loopback: bool,
    selected_interface: Option<String>,
    fullname: Option<String>,
}

impl MdnsDiscovery {
    /// Advertise this device and browse for peers.
    pub async fn start(
        device_id: Uuid,
        name: &str,
        platform: &str,
        port: u16,
        cert_fingerprint: [u8; 32],
        config: DiscoveryConfig,
    ) -> Result<Self, DiscoveryError> {
        Self::open(
            device_id,
            Some((name, platform, port, cert_fingerprint)),
            config,
            true,
        )
        .await
    }

    /// Advertise this device without opening a browse stream.
    pub async fn advertise(
        device_id: Uuid,
        name: &str,
        platform: &str,
        port: u16,
        cert_fingerprint: [u8; 32],
        config: DiscoveryConfig,
    ) -> Result<Self, DiscoveryError> {
        Self::open(
            device_id,
            Some((name, platform, port, cert_fingerprint)),
            config,
            false,
        )
        .await
    }

    /// Link-local IPv4 (APIPA) and IPv6 (fe80::/10) cannot be dialled without a
    /// scope, so they are not worth advertising.
    fn is_link_local(address: &std::net::IpAddr) -> bool {
        match address {
            std::net::IpAddr::V4(v4) => v4.is_link_local(),
            std::net::IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
        }
    }

    /// Browse for peers without advertising this device.
    pub async fn browse(device_id: Uuid, config: DiscoveryConfig) -> Result<Self, DiscoveryError> {
        Self::open(device_id, None, config, true).await
    }

    async fn open(
        device_id: Uuid,
        advertisement: Option<(&str, &str, u16, [u8; 32])>,
        config: DiscoveryConfig,
        browse: bool,
    ) -> Result<Self, DiscoveryError> {
        let addresses = local_addresses(&config)?;
        let daemon = ServiceDaemon::new()?;
        configure_interfaces(&daemon, &config)?;

        let fullname = if let Some((name, platform, port, cert_fingerprint)) = advertisement {
            // Link-local addresses need a scope id to be reachable, and Android's
            // NsdManager prefers an AAAA record over A, so publishing fe80::...
            // makes a phone resolve an address it cannot dial. Advertise the
            // routable ones and fall back only if there is nothing else.
            let routable: Vec<&std::net::IpAddr> = addresses
                .iter()
                .filter(|address| !Self::is_link_local(address))
                .collect();
            let advertised = if routable.is_empty() {
                addresses.iter().collect::<Vec<_>>()
            } else {
                routable
            };
            let ip_list = advertised
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let instance = device_id.to_string();
            let host = format!("{device_id}.local.");
            // The advertisement is where an unpaired peer first sees what this
            // machine is; reading it here keeps the pairing list readable before
            // any handshake happens.
            let info = fluxy_sysinfo::cached();
            let properties = [
                (TXT_DEVICE_ID, device_id.to_string()),
                (TXT_NAME, name.to_string()),
                (TXT_PLATFORM, platform.to_string()),
                (TXT_VERSION, PROTOCOL_VERSION.to_string()),
                (TXT_FEATURES, "chat".to_string()),
                (TXT_FINGERPRINT, hex::encode(cert_fingerprint)),
                (TXT_MANUFACTURER, info.manufacturer.clone()),
                (TXT_MODEL, info.model.clone()),
                (TXT_OS_VERSION, info.display_os()),
            ];
            let info = ServiceInfo::new(
                SERVICE_TYPE,
                &instance,
                &host,
                ip_list.as_str(),
                port,
                &properties[..],
            )?;
            daemon.register(info)?;
            Some(format!("{instance}.{SERVICE_TYPE}"))
        } else {
            None
        };

        let receiver = if browse {
            Some(daemon.browse(SERVICE_TYPE)?)
        } else {
            None
        };

        Ok(Self {
            daemon,
            receiver,
            own_id: device_id,
            table: PeerTable::new(config.peer_ttl),
            by_fullname: HashMap::new(),
            allow_loopback: config.allow_loopback,
            selected_interface: config.interface,
            fullname,
        })
    }

    pub fn table(&self) -> &PeerTable {
        &self.table
    }

    pub async fn next_event(&mut self) -> Option<DiscoveryEvent> {
        let receiver = self.receiver.as_ref()?.clone();
        loop {
            let prune_after = self
                .table
                .ttl()
                .clamp(Duration::from_millis(1), Duration::from_secs(5));
            let event = tokio::select! {
                event = receiver.recv_async() => event,
                _ = tokio::time::sleep(prune_after) => {
                    let expired = self.table.prune();
                    if expired.is_empty() {
                        continue;
                    }
                    self.by_fullname
                        .retain(|_, device_id| !expired.contains(device_id));
                    return Some(DiscoveryEvent::Lost(expired[0]));
                }
            };
            match event {
                Ok(ServiceEvent::ServiceResolved(resolved)) => {
                    let advertisement = match PeerAdvertisement::from_resolved(
                        &resolved,
                        self.selected_interface.clone(),
                        self.allow_loopback,
                    ) {
                        Ok(advertisement) => advertisement,
                        Err(DiscoveryError::Incompatible) => continue,
                        Err(error) => {
                            tracing::debug!(%error, fullname = %resolved.fullname, "ignoring mDNS peer");
                            continue;
                        }
                    };
                    if advertisement.device_id == self.own_id {
                        continue;
                    }
                    self.by_fullname
                        .insert(resolved.fullname.clone(), advertisement.device_id);
                    let record = self.table.upsert(advertisement);
                    return Some(DiscoveryEvent::Found(Box::new(record)));
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    if let Some(device_id) = self.by_fullname.remove(&fullname) {
                        self.table.remove(device_id);
                        return Some(DiscoveryEvent::Lost(device_id));
                    }
                }
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    pub async fn next_found(&mut self, timeout: Duration) -> Option<PeerRecord> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match tokio::time::timeout(remaining, self.next_event()).await {
                Ok(Some(DiscoveryEvent::Found(record))) => return Some(*record),
                Ok(Some(DiscoveryEvent::Lost(_))) => continue,
                Ok(None) | Err(_) => return None,
            }
        }
    }

    pub fn shutdown(&self) {
        if let Some(fullname) = &self.fullname {
            let _ = self.daemon.unregister(fullname);
        }
        let _ = self.daemon.shutdown();
    }
}

pub(crate) fn sanitize_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

fn configure_interfaces(
    daemon: &ServiceDaemon,
    config: &DiscoveryConfig,
) -> Result<(), DiscoveryError> {
    if let Some(interface) = &config.interface {
        daemon.disable_interface(IfKind::All)?;
        daemon.enable_interface(IfKind::Name(interface.clone()))?;
    } else if !config.allow_loopback {
        daemon.disable_interface(IfKind::LoopbackV4)?;
        daemon.disable_interface(IfKind::LoopbackV6)?;
    }
    // Keep same-host instances visible without publishing loopback addresses.
    // This is multicast receive-loop behavior, not the loopback interface.
    daemon.set_multicast_loop_v4(true)?;
    daemon.set_multicast_loop_v6(true)?;
    Ok(())
}

fn local_addresses(config: &DiscoveryConfig) -> Result<Vec<IpAddr>, DiscoveryError> {
    let interfaces = get_if_addrs().map_err(|error| DiscoveryError::Daemon(error.to_string()))?;
    let mut addresses = Vec::new();
    let mut selected_found = false;
    for interface in interfaces {
        if let Some(selected) = &config.interface {
            // On Windows if-addrs reports the friendly name in `name` and the
            // adapter GUID in `adapter_name`; accept either so a caller can pass
            // what the interface list showed it.
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
        addresses.push(interface.ip());
    }
    addresses.sort();
    addresses.dedup();
    if config.interface.is_some() && !selected_found {
        return Err(DiscoveryError::Interface(
            config.interface.clone().unwrap_or_default(),
        ));
    }
    if addresses.is_empty() {
        return Err(DiscoveryError::NoInterface);
    }
    Ok(addresses)
}

impl Drop for MdnsDiscovery {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_local_addresses_are_not_advertised() {
        // A phone resolving an AAAA record it cannot dial was the discovery
        // failure this filter exists for.
        assert!(MdnsDiscovery::is_link_local(&"fe80::1".parse().unwrap()));
        assert!(MdnsDiscovery::is_link_local(
            &"169.254.1.1".parse().unwrap()
        ));
        assert!(!MdnsDiscovery::is_link_local(
            &"192.168.1.8".parse().unwrap()
        ));
        assert!(!MdnsDiscovery::is_link_local(
            &"2409:8a04::1".parse().unwrap()
        ));
        assert!(!MdnsDiscovery::is_link_local(&"::1".parse().unwrap()));
    }
    use std::time::Duration;

    fn advertisement(id: Uuid, ip: &str, port: u16) -> PeerAdvertisement {
        PeerAdvertisement {
            device_id: id,
            name: "Desk".into(),
            platform: "windows".into(),
            manufacturer: String::new(),
            model: String::new(),
            os_version: String::new(),
            protocol_version: PROTOCOL_VERSION,
            features: vec!["chat".into()],
            cert_fingerprint: [7; 32],
            addresses: vec![ip.parse().unwrap()],
            port,
            interface: None,
        }
    }

    #[test]
    fn peer_table_deduplicates_by_device_id() {
        let id = Uuid::now_v7();
        let now = Instant::now();
        let mut table = PeerTable::new(Duration::from_secs(30));

        let first = table.upsert_at(advertisement(id, "192.168.1.10", 42421), now);
        assert_eq!(table.len(), 1);
        assert_eq!(first.first_seen, now);

        let second = table.upsert_at(
            advertisement(id, "192.168.1.11", 42422),
            now + Duration::from_secs(5),
        );
        assert_eq!(table.len(), 1);
        assert_eq!(second.first_seen, first.first_seen);
        assert_eq!(
            second.addresses,
            vec!["192.168.1.11".parse::<IpAddr>().unwrap()]
        );
        assert_eq!(second.port, 42422);
    }

    #[test]
    fn peer_table_expires_stale_records() {
        let now = Instant::now();
        let mut table = PeerTable::new(Duration::from_secs(3));
        let id = Uuid::now_v7();
        table.upsert_at(advertisement(id, "192.168.1.10", 42421), now);
        assert!(table.prune_at(now + Duration::from_secs(2)).is_empty());
        assert_eq!(table.prune_at(now + Duration::from_secs(3)), vec![id]);
        assert!(table.is_empty());
    }

    #[test]
    fn txt_record_is_parsed() {
        let id = Uuid::now_v7();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &id.to_string(),
            "host.local.",
            "192.168.1.10",
            42421,
            &[
                (TXT_DEVICE_ID, id.to_string()),
                (TXT_NAME, "Studio".to_string()),
                (TXT_PLATFORM, "linux".to_string()),
                (TXT_VERSION, PROTOCOL_VERSION.to_string()),
                (TXT_FEATURES, "chat,clipboard".to_string()),
                (TXT_FINGERPRINT, hex::encode([9u8; 32])),
            ][..],
        )
        .unwrap()
        .as_resolved_service();

        let peer = PeerAdvertisement::from_resolved(&info, None, false).unwrap();
        assert_eq!(peer.device_id, id);
        assert_eq!(peer.name, "Studio");
        assert_eq!(peer.platform, "linux");
        assert_eq!(peer.features, vec!["chat", "clipboard"]);
        assert_eq!(peer.cert_fingerprint, [9u8; 32]);
        assert_eq!(peer.port, 42421);
    }

    #[test]
    fn hardware_hints_survive_the_txt_record() {
        let id = Uuid::now_v7();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &id.to_string(),
            "host.local.",
            "192.168.1.10",
            42421,
            &[
                (TXT_DEVICE_ID, id.to_string()),
                (TXT_NAME, "Studio".to_string()),
                (TXT_PLATFORM, "windows".to_string()),
                (TXT_VERSION, PROTOCOL_VERSION.to_string()),
                (TXT_FEATURES, "chat".to_string()),
                (TXT_FINGERPRINT, hex::encode([9u8; 32])),
                (TXT_MANUFACTURER, "LENOVO".to_string()),
                (TXT_MODEL, "21LF".to_string()),
                (TXT_OS_VERSION, "Windows 11 Pro (26H2)".to_string()),
            ][..],
        )
        .unwrap()
        .as_resolved_service();

        let peer = PeerAdvertisement::from_resolved(&info, None, false).unwrap();
        assert_eq!(peer.manufacturer, "LENOVO");
        assert_eq!(peer.model, "21LF");
        assert_eq!(peer.os_version, "Windows 11 Pro (26H2)");
    }

    #[test]
    fn an_older_peer_without_hints_still_parses() {
        let id = Uuid::now_v7();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &id.to_string(),
            "host.local.",
            "192.168.1.10",
            42421,
            &[
                (TXT_DEVICE_ID, id.to_string()),
                (TXT_NAME, "Old".to_string()),
                (TXT_PLATFORM, "linux".to_string()),
                (TXT_VERSION, PROTOCOL_VERSION.to_string()),
                (TXT_FEATURES, "chat".to_string()),
                (TXT_FINGERPRINT, hex::encode([9u8; 32])),
            ][..],
        )
        .unwrap()
        .as_resolved_service();

        let peer = PeerAdvertisement::from_resolved(&info, None, false).unwrap();
        assert!(peer.manufacturer.is_empty());
        assert!(peer.model.is_empty());
        assert!(peer.os_version.is_empty());
        assert_eq!(peer.platform, "linux");
    }

    #[test]
    fn incompatible_txt_version_is_rejected() {
        let id = Uuid::now_v7();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            "instance",
            "host.local.",
            "192.168.1.10",
            42421,
            &[
                (TXT_DEVICE_ID, id.to_string()),
                (TXT_VERSION, "999".to_string()),
            ][..],
        )
        .unwrap()
        .as_resolved_service();
        assert!(matches!(
            PeerAdvertisement::from_resolved(&info, None, false),
            Err(DiscoveryError::Incompatible)
        ));
    }

    #[test]
    fn missing_interface_is_reported() {
        let config = DiscoveryConfig {
            interface: Some("fluxy-no-such-interface".into()),
            allow_loopback: false,
            peer_ttl: Duration::from_secs(30),
        };
        assert!(matches!(
            local_addresses(&config),
            Err(DiscoveryError::Interface(_))
        ));
    }

    #[test]
    fn selected_interface_limits_advertised_addresses() {
        let interfaces = get_if_addrs().unwrap();
        let Some(selected) = interfaces
            .iter()
            .find(|interface| !interface.is_loopback() && interface.is_oper_up())
        else {
            return;
        };
        let config = DiscoveryConfig {
            interface: Some(selected.name.clone()),
            allow_loopback: false,
            peer_ttl: Duration::from_secs(30),
        };
        let addresses = local_addresses(&config).unwrap();
        assert!(!addresses.is_empty());
        assert!(addresses.iter().all(|address| {
            interfaces
                .iter()
                .any(|interface| interface.name == selected.name && interface.ip() == *address)
        }));
    }

    #[test]
    fn default_discovery_excludes_loopback() {
        let config = DiscoveryConfig::default();
        match local_addresses(&config) {
            Ok(addresses) => assert!(addresses.iter().all(|address| !address.is_loopback())),
            Err(DiscoveryError::NoInterface) => {}
            Err(error) => panic!("unexpected discovery error: {error}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn discovers_loopback_peer_within_three_seconds() {
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();
        let config = || DiscoveryConfig {
            interface: None,
            allow_loopback: true,
            peer_ttl: Duration::from_secs(30),
        };

        let first = MdnsDiscovery::start(first_id, "first", "windows", 42431, [1; 32], config())
            .await
            .unwrap();
        let mut second =
            MdnsDiscovery::start(second_id, "second", "linux", 42432, [2; 32], config())
                .await
                .unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let found = loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break None;
            }
            match tokio::time::timeout(remaining, second.next_event()).await {
                Ok(Some(DiscoveryEvent::Found(record))) if record.device_id == first_id => {
                    break Some(record);
                }
                Ok(Some(_)) => continue,
                Ok(None) | Err(_) => break None,
            }
        };

        first.shutdown();
        second.shutdown();
        let record = found.expect("peer was not discovered within three seconds");
        assert_eq!(record.name, "first");
        assert_eq!(record.port, 42431);
    }
}
