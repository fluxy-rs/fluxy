//! LAN discovery, pairing challenges, and the trusted-peer handshake runtime.
//!
//! This module deliberately keeps all key material in Rust. The WebView only
//! receives peer display data, a six-digit SAS, and stable error codes.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fluxy_app::{AppConfig, Engine};
use fluxy_core::{TrustEntry, TrustStore};
#[cfg(not(target_os = "android"))]
use fluxy_net::default_gateway;
#[cfg(not(target_os = "android"))]
use fluxy_net::interface::select_auto;
#[cfg(not(target_os = "android"))]
use fluxy_net::interface::select_manual;
use fluxy_net::interface::{
    InterfaceInfo, Selection, SelectionError, SelectionMode, default_route_interface,
    display_order, enumerate,
};
use fluxy_net::{
    DEFAULT_PEER_TTL, DiscoveryConfig, DiscoveryEvent, FALLBACK_PEER_TTL, FALLBACK_PROBE_INTERVAL,
    FallbackConfig, FallbackDiagnostics, FallbackDiscovery, FallbackEvent, HandshakeError,
    MdnsDiscovery, PairingLimiter, PairingPrompt, PeerRecord, PeerSession, SERVICE_TYPE,
    TlsIdentity, accept, client_endpoint, client_pair_async, connect, server_endpoint_bound,
    server_pair_async,
};
use tokio::sync::{Mutex as AsyncMutex, Notify, oneshot, watch};
use uuid::Uuid;

use crate::chat::ChatRuntime;
use crate::contract::{
    AppEvent, ClipboardReceivePolicy, EngineEvent, EventHub, InterfaceOptionView, IpcError,
    NetworkStatusView, PairingEvent, PeerEvent, PeerSummary,
};

const PAIRING_TTL: Duration = Duration::from_secs(120);

/// The plan asks for a two-second look at the interface table so a cable pulled
/// out mid-session is noticed long before the user wonders why nothing connects.
const INTERFACE_POLL_INTERVAL: Duration = Duration::from_secs(2);

fn keeps_mdns(existing: Option<DiscoverySource>, incoming: DiscoverySource) -> bool {
    existing == Some(DiscoverySource::Mdns) && incoming == DiscoverySource::Gateway
}

#[derive(Debug, Clone)]
struct NetworkState {
    state: String,
    detail: String,
    interface: Option<String>,
    local_address: Option<String>,
    listen_addr: Option<String>,
    discovery_port: u16,
    advertised: bool,
    browsing: bool,
    local_addresses: Vec<String>,
    dev_loopback: bool,
    selection_mode: String,
    selection_source: String,
    interface_index: Option<u32>,
    interface_pinned: bool,
    rebind_count: u32,
    last_rebind_ms: Option<i64>,
    interface_options: Vec<InterfaceOptionView>,
}

impl NetworkState {
    fn starting(dev_loopback: bool, discovery_port: u16) -> Self {
        Self {
            state: "starting".into(),
            detail: "Preparing the LAN listener and discovery.".into(),
            interface: None,
            local_address: None,
            listen_addr: None,
            discovery_port,
            advertised: false,
            browsing: false,
            local_addresses: Vec::new(),
            dev_loopback,
            selection_mode: SelectionMode::Auto.as_str().into(),
            selection_source: String::new(),
            interface_index: None,
            interface_pinned: false,
            rebind_count: 0,
            last_rebind_ms: None,
            interface_options: Vec::new(),
        }
    }

    /// Copy the selection facts that every state transition carries along.
    fn with_selection(mut self, binding: &Binding, pinned: bool) -> Self {
        self.interface = binding.interface();
        self.local_address = Some(binding.selection.display_address());
        self.local_addresses = binding.local_addresses.clone();
        self.selection_mode = binding.mode.as_str().into();
        self.selection_source = binding.selection.source.as_str().into();
        self.interface_index = Some(binding.selection.interface.index);
        self.interface_pinned = pinned;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoverySource {
    Mdns,
    Gateway,
    /// Typed in by the user; the counterpart of a network that hides multicast.
    Manual,
    /// Resolved by the platform's NsdManager through the mobile plugin.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    Nsd,
}

#[derive(Debug, Clone)]
pub struct NearbyPeer {
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    /// Display-only hardware hints; empty for peers that do not advertise them,
    /// and for entries typed in by hand.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
    pub addresses: Vec<IpAddr>,
    pub port: u16,
    pub cert_fingerprint: [u8; 32],
    pub last_seen_ms: i64,
    pub interface: Option<String>,
    source: DiscoverySource,
}

impl NearbyPeer {
    /// Build a peer from what the user typed when discovery cannot see the
    /// other side (a NATed emulator, a network that filters multicast). The
    /// device id is a placeholder until the handshake reports the real one.
    pub fn manual(address: SocketAddr, cert_fingerprint: [u8; 32]) -> Self {
        Self {
            device_id: Uuid::nil(),
            name: address.to_string(),
            platform: "manual".into(),
            // Nothing is known about a hand-typed address.
            manufacturer: String::new(),
            model: String::new(),
            os_version: String::new(),
            addresses: vec![address.ip()],
            port: address.port(),
            cert_fingerprint,
            last_seen_ms: 0,
            interface: None,
            source: DiscoverySource::Manual,
        }
    }

    fn summary(&self) -> PeerSummary {
        PeerSummary {
            device_id: self.device_id.to_string(),
            name: self.name.clone(),
            platform: self.platform.clone(),
            manufacturer: self.manufacturer.clone(),
            model: self.model.clone(),
            os_version: self.os_version.clone(),
            trusted: false,
            online: true,
            clipboard_policy: ClipboardReceivePolicy::Ask,
            auto_accept: crate::contract::DeviceAutoAccept::Inherit,
            paired_at_ms: 0,
            last_seen_ms: Some(self.last_seen_ms),
            addresses: self.addresses.iter().map(ToString::to_string).collect(),
        }
    }

    pub fn socket_addr(&self) -> Option<SocketAddr> {
        let address = self
            .addresses
            .iter()
            .copied()
            .find(|address| address.is_ipv4() && !address.is_loopback())
            .or_else(|| {
                self.addresses
                    .iter()
                    .copied()
                    .find(|address| !address.is_loopback())
            })
            .or_else(|| self.addresses.first().copied())?;
        Some(SocketAddr::new(address, self.port))
    }
}

#[derive(Debug)]
struct PendingPairing {
    challenge_id: String,
    peer_device_id: String,
    peer_name: String,
    peer_platform: String,
    direction: String,
    sas: Option<String>,
    expires_at: Option<Instant>,
    sender: Option<oneshot::Sender<bool>>,
}

pub struct NetworkRuntime {
    engine: Arc<Mutex<Engine>>,
    /// The mobile plugin handle, installed by the shell on Android.
    #[cfg(target_os = "android")]
    mobile: std::sync::OnceLock<Arc<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>>,
    #[cfg(not(target_os = "android"))]
    #[allow(dead_code)]
    mobile: std::sync::OnceLock<()>,
    hub: Arc<EventHub>,
    chat: Arc<ChatRuntime>,
    state: Mutex<NetworkState>,
    /// Shared with the chat runtime's address book, which needs the current
    /// address of a peer without owning the discovery state.
    nearby: Arc<Mutex<BTreeMap<Uuid, NearbyPeer>>>,
    fallback_stats: Arc<Mutex<FallbackDiagnostics>>,
    fallback_active: AtomicBool,
    challenges: Mutex<HashMap<String, PendingPairing>>,
    limiter: AsyncMutex<PairingLimiter>,
    pairing_active: AtomicBool,
    started: AtomicBool,
    running: AtomicBool,
    shutdown: Mutex<watch::Sender<bool>>,
    stopped: Notify,
    /// Called before the listener is rebound so active transfers can pause and
    /// keep their checkpoints. The transfer runtime is wired in by AppState.
    rebind_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl NetworkRuntime {
    pub fn new(
        engine: Arc<Mutex<Engine>>,
        hub: Arc<EventHub>,
        chat: Arc<ChatRuntime>,
    ) -> Arc<Self> {
        let nearby: Arc<Mutex<BTreeMap<Uuid, NearbyPeer>>> = Arc::new(Mutex::new(BTreeMap::new()));
        // The chat runtime asks here before every dial, so a session can never
        // hold an address discovery has already replaced.
        {
            let lookup = nearby.clone();
            chat.set_address_book(Arc::new(move |device_id: Uuid| {
                let peers = lookup.lock().ok()?;
                let peer = peers.get(&device_id)?;
                // The listener binds an IPv4 address, so dialling an IPv6 peer
                // fails with "invalid remote address" - which is what happened
                // whenever discovery reported the phone's IPv6 forms first.
                // Prefer IPv4 and only fall back to whatever else is there.
                let addr = peer
                    .addresses
                    .iter()
                    .copied()
                    .find(|addr| addr.is_ipv4())
                    .or_else(|| peer.addresses.first().copied())?;
                Some(crate::chat::ChatPeer {
                    device_id,
                    name: peer.name.clone(),
                    platform: peer.platform.clone(),
                    addr: SocketAddr::new(addr, peer.port),
                })
            }));
        }
        let dev_loopback = engine
            .lock()
            .map(|engine| engine.config().dev_allow_loopback)
            .unwrap_or(false);
        let discovery_port = engine
            .lock()
            .map(|engine| engine.config().discovery_port)
            .unwrap_or(fluxy_net::DISCOVERY_PORT);
        let (shutdown, _) = watch::channel(false);
        Arc::new(Self {
            engine,
            #[cfg(target_os = "android")]
            mobile: std::sync::OnceLock::new(),
            #[cfg(not(target_os = "android"))]
            mobile: std::sync::OnceLock::new(),
            hub,
            chat,
            state: Mutex::new(NetworkState::starting(dev_loopback, discovery_port)),
            nearby,
            fallback_stats: Arc::new(Mutex::new(FallbackDiagnostics::default())),
            fallback_active: AtomicBool::new(false),
            challenges: Mutex::new(HashMap::new()),
            limiter: AsyncMutex::new(PairingLimiter::default()),
            pairing_active: AtomicBool::new(false),
            started: AtomicBool::new(false),
            running: AtomicBool::new(false),
            shutdown: Mutex::new(shutdown),
            stopped: Notify::new(),
            rebind_hook: Mutex::new(None),
        })
    }

    /// Hand the runtime the mobile plugin handle, so it can ask the platform for
    /// the gateway and the discovered peers.
    #[cfg(target_os = "android")]
    pub fn set_mobile(&self, handle: Arc<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>) {
        let _ = self.mobile.set(handle);
    }

    /// Wire the pre-rebind hook. It runs before a network change tears the
    /// listener down, so an active transfer pauses with its checkpoint instead of
    /// failing on a dead socket.
    pub fn attach_rebind_hook(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut current) = self.rebind_hook.lock() {
            *current = Some(hook);
        }
    }

    fn run_rebind_hook(&self) {
        let hook = self
            .rebind_hook
            .lock()
            .ok()
            .and_then(|current| current.clone());
        if let Some(hook) = hook {
            hook();
        }
    }

    pub fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let (shutdown, receiver) = watch::channel(false);
        if let Ok(mut current) = self.shutdown.lock() {
            *current = shutdown;
        }
        // The reset state reports the mode the user chose, not the mode that
        // was bound a moment ago. Building it from `starting()` alone said
        // "automatic" for the whole rebind, so applying a manual selection
        // flipped the panel back to automatic before the new address landed.
        let chosen = self
            .engine
            .lock()
            .map(|engine| engine.config().network.mode)
            .unwrap_or(SelectionMode::Auto);
        if let Ok(mut state) = self.state.lock() {
            let mut next = NetworkState::starting(state.dev_loopback, state.discovery_port);
            next.selection_mode = chosen.as_str().into();
            *state = next;
        }
        self.running.store(true, Ordering::SeqCst);
        self.fallback_active.store(false, Ordering::SeqCst);
        if let Ok(mut stats) = self.fallback_stats.lock() {
            *stats = FallbackDiagnostics::default();
        }
        let runtime = self.clone();
        let guard = self.clone();
        tauri::async_runtime::spawn(async move {
            runtime.run(receiver).await;
            guard.running.store(false, Ordering::SeqCst);
            guard.stopped.notify_waiters();
        });
    }

    pub fn stop(&self) {
        self.started.store(false, Ordering::SeqCst);
        self.fallback_active.store(false, Ordering::SeqCst);
        if let Ok(shutdown) = self.shutdown.lock() {
            let _ = shutdown.send(true);
        }
        self.chat.reset();
        self.clear_nearby();
        self.state
            .lock()
            .map(|mut state| {
                state.state = "stopped".into();
                state.detail = "The LAN runtime is stopped.".into();
                state.advertised = false;
                state.browsing = false;
            })
            .ok();
    }

    /// Restart the whole LAN runtime so a rotated identity replaces both the
    /// QUIC certificate and the mDNS advertisement fingerprint.
    pub fn restart(self: &Arc<Self>) {
        self.stop();
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            // The old run owns the QUIC endpoint and mDNS daemon. Wait for it to
            // release both before binding again; a fixed short grace period is
            // enough for the two local socket handles.
            for _ in 0..20 {
                if !runtime.running.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            runtime.start();
        });
    }

    pub fn pairing_in_progress(&self) -> bool {
        self.pairing_active.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> NetworkStatusView {
        let state = self
            .state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|_| NetworkState::starting(false, fluxy_net::DISCOVERY_PORT));
        let (nearby_count, fallback_peers) = self
            .nearby
            .lock()
            .map(|peers| {
                (
                    peers.len() as u32,
                    peers
                        .values()
                        .filter(|peer| peer.source == DiscoverySource::Gateway)
                        .count() as u32,
                )
            })
            .unwrap_or((0, 0));
        let fallback = self
            .fallback_stats
            .lock()
            .map(|stats| stats.clone())
            .unwrap_or_default();
        NetworkStatusView {
            state: state.state,
            detail: state.detail,
            interface: state.interface,
            local_address: state.local_address,
            listen_addr: state.listen_addr,
            discovery_port: state.discovery_port,
            mdns_service: SERVICE_TYPE.to_string(),
            advertised: state.advertised,
            browsing: state.browsing,
            nearby_count,
            local_addresses: state.local_addresses,
            dev_loopback: state.dev_loopback,
            selection_mode: state.selection_mode,
            selection_source: state.selection_source,
            interface_index: state.interface_index,
            interface_pinned: state.interface_pinned,
            rebind_count: state.rebind_count,
            last_rebind_ms: state.last_rebind_ms,
            interface_options: state.interface_options,
            fallback_active: self.fallback_active.load(Ordering::SeqCst),
            fallback_gateway: fallback.gateway,
            fallback_targets: fallback.targets,
            fallback_probes_sent: fallback.probes_sent,
            fallback_responses: fallback.responses_received,
            fallback_peers,
            fallback_last_error: fallback.last_error,
        }
    }

    pub fn peer_summaries(&self, trusted: &[TrustEntry]) -> Vec<PeerSummary> {
        let mut merged: BTreeMap<String, PeerSummary> = BTreeMap::new();
        for entry in trusted {
            merged.insert(entry.device_id.to_string(), PeerSummary::from(entry));
        }
        if let Ok(peers) = self.nearby.lock() {
            for peer in peers.values() {
                let id = peer.device_id.to_string();
                let nearby = peer.summary();
                match merged.get_mut(&id) {
                    Some(existing) => {
                        existing.online = true;
                        existing.last_seen_ms = nearby.last_seen_ms;
                        existing.addresses = nearby.addresses;
                        // Discovery reports what the peer calls itself right now;
                        // the trust entry only holds the name it used at pairing
                        // time, so a rename or a reinstall would otherwise leave
                        // the list showing a stale label forever.
                        if !nearby.name.is_empty() {
                            existing.name = nearby.name;
                        }
                        // The trust store does not persist the hardware hints,
                        // so a live advertisement is the only source for a paired
                        // peer. An older peer sends none, hence the emptiness
                        // check: a known value must never be overwritten with
                        // nothing just because the peer went quiet.
                        for (target, source) in [
                            (&mut existing.manufacturer, &nearby.manufacturer),
                            (&mut existing.model, &nearby.model),
                            (&mut existing.os_version, &nearby.os_version),
                        ] {
                            if !source.is_empty() {
                                target.clone_from(source);
                            }
                        }
                        if !existing.trusted {
                            existing.platform = nearby.platform;
                        }
                    }
                    None => {
                        merged.insert(id, nearby);
                    }
                }
            }
        }
        merged.into_values().collect()
    }

    /// Everything discovery has seen recently, newest first.
    pub fn nearby_snapshot(&self) -> Vec<NearbyPeer> {
        let now = now_ms();
        tracing::info!(
            total = self.nearby.lock().map(|peers| peers.len()).unwrap_or(0),
            "nearby snapshot requested"
        );
        let mut peers: Vec<NearbyPeer> = self
            .nearby
            .lock()
            .map(|peers| {
                peers
                    .values()
                    .filter(|peer| now - peer.last_seen_ms < DEFAULT_PEER_TTL.as_millis() as i64)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        peers.sort_by_key(|peer| std::cmp::Reverse(peer.last_seen_ms));
        peers
    }

    pub fn nearby_peer(&self, device_id: &str) -> Result<NearbyPeer, IpcError> {
        let id = Uuid::parse_str(device_id).map_err(|_| IpcError::internal("invalid peer id"))?;
        self.nearby
            .lock()
            .map_err(|_| IpcError::internal("peer table unavailable"))?
            .get(&id)
            .cloned()
            .ok_or_else(|| IpcError {
                code: "peer_not_found".into(),
                code_number: 4104,
                message: "peer is no longer nearby".into(),
                retryable: true,
            })
    }

    pub fn begin_outbound(
        &self,
        peer: &NearbyPeer,
    ) -> Result<(String, oneshot::Receiver<bool>), IpcError> {
        if self
            .pairing_active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(IpcError {
                code: "pairing_busy".into(),
                code_number: 4105,
                message: "another pairing is already in progress".into(),
                retryable: true,
            });
        }
        let challenge_id = Uuid::now_v7().to_string();
        let (sender, receiver) = oneshot::channel();
        let pending = PendingPairing {
            challenge_id: challenge_id.clone(),
            peer_device_id: peer.device_id.to_string(),
            peer_name: peer.name.clone(),
            peer_platform: peer.platform.clone(),
            direction: "initiator".into(),
            sas: None,
            expires_at: None,
            sender: Some(sender),
        };
        let mut challenges = self
            .challenges
            .lock()
            .map_err(|_| IpcError::internal("pairing registry unavailable"))?;
        challenges.insert(challenge_id.clone(), pending);
        Ok((challenge_id, receiver))
    }

    pub fn confirm(&self, challenge_id: &str, accepted: bool) -> Result<(), IpcError> {
        let mut challenges = self
            .challenges
            .lock()
            .map_err(|_| IpcError::internal("pairing registry unavailable"))?;
        let Some(pending) = challenges.get_mut(challenge_id) else {
            return Err(IpcError {
                code: "pairing_unknown".into(),
                code_number: 4106,
                message: "pairing challenge is no longer active".into(),
                retryable: true,
            });
        };
        if pending
            .expires_at
            .is_some_and(|expires| Instant::now() >= expires)
        {
            return Err(IpcError {
                code: "pairing_expired".into(),
                code_number: 4107,
                message: "pairing challenge expired".into(),
                retryable: true,
            });
        }
        let Some(sender) = pending.sender.take() else {
            return Err(IpcError {
                code: "pairing_already_answered".into(),
                code_number: 4108,
                message: "pairing challenge was already answered".into(),
                retryable: false,
            });
        };
        let _ = sender.send(accepted);
        Ok(())
    }

    pub fn cancel(&self, challenge_id: &str) -> Result<(), IpcError> {
        let pending = self
            .challenges
            .lock()
            .map_err(|_| IpcError::internal("pairing registry unavailable"))?
            .remove(challenge_id);
        let Some(mut pending) = pending else {
            return Ok(());
        };
        if let Some(sender) = pending.sender.take() {
            let _ = sender.send(false);
        }
        self.pairing_active.store(false, Ordering::SeqCst);
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Pairing {
            data: PairingEvent::Cancelled {
                challenge_id: pending.challenge_id,
            },
        });
        Ok(())
    }

    pub async fn pair_outbound(
        self: Arc<Self>,
        peer: NearbyPeer,
        challenge_id: String,
        confirmation: oneshot::Receiver<bool>,
    ) -> Result<PeerSession, (String, String)> {
        let Some(address) = peer.socket_addr() else {
            return Err(("peer_not_found".into(), "peer has no usable address".into()));
        };
        let (identity, tls, trust, _) = self.network_identity()?;
        let local_ip: IpAddr = if address.is_ipv4() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        };
        let endpoint = client_endpoint(SocketAddr::new(local_ip, 0), &tls, peer.cert_fingerprint)
            .await
            .map_err(|error| ("connect_failed".into(), error.to_string()))?;
        tracing::info!(%address, "dialling a peer for pairing");
        let connection = connect(&endpoint, address).await.map_err(|error| {
            tracing::warn!(%address, %error, "pairing connection failed");
            ("connect_failed".into(), error.to_string())
        })?;
        tracing::info!("pairing connection established");
        let receiver = Arc::new(AsyncMutex::new(Some(confirmation)));
        let runtime = self.clone();
        let challenge_for_confirm = challenge_id.clone();
        let mut trust = trust;
        let result = client_pair_async(&connection, &identity, &tls, &mut trust, move |prompt| {
            let runtime = runtime.clone();
            let challenge = challenge_for_confirm.clone();
            let receiver = receiver.clone();
            async move {
                if !runtime.ready(&challenge, &prompt) {
                    return false;
                }
                let mut guard = receiver.lock().await;
                match guard.take() {
                    Some(receiver) => receiver.await.unwrap_or(false),
                    None => false,
                }
            }
        })
        .await
        .map_err(handshake_failure);
        // Give the peer a moment to consume PairReply or PairRejection before the
        // QUIC close frame. A close immediately after the final stream write can
        // race the peer's read and turn a clean outcome into "connection lost".
        tokio::time::sleep(Duration::from_millis(150)).await;
        connection.close(0u32.into(), b"pair-finished");
        endpoint.wait_idle().await;
        result
    }

    /// What the mobile shell reports about the active link, if anything.
    #[cfg(target_os = "android")]
    async fn mobile_network_info(&self) -> Option<tauri_plugin_fluxy_mobile::LocalNetworkInfo> {
        let Some(handle) = self.mobile.get() else {
            tracing::warn!("no mobile plugin handle; the platform network facts stay unknown");
            return None;
        };
        match handle.local_network_info().await {
            Ok(info) => {
                tracing::info!(
                    gateway = ?info.gateway,
                    interface = ?info.interface,
                    addresses = ?info.addresses,
                    wifi = info.wifi,
                    "platform network facts"
                );
                Some(info)
            }
            Err(error) => {
                tracing::warn!(%error, "the platform network facts could not be read");
                None
            }
        }
    }

    /// The gateway the mobile shell reports, if any.
    #[cfg(target_os = "android")]
    async fn mobile_network_gateway(&self) -> Option<std::net::Ipv4Addr> {
        self.mobile_network_info()
            .await
            .and_then(|info| info.gateway)
            .and_then(|text| text.parse::<std::net::Ipv4Addr>().ok())
    }

    async fn run(self: Arc<Self>, shutdown: watch::Receiver<bool>) {
        self.run_inner(shutdown).await;
    }

    async fn run_inner(self: Arc<Self>, shutdown: watch::Receiver<bool>) {
        let (config, identity, tls) = match self.startup_identity() {
            Ok(value) => value,
            Err(error) => {
                self.set_state(NetworkState {
                    state: "unavailable".into(),
                    detail: error,
                    dev_loopback: false,
                    discovery_port: fluxy_net::DISCOVERY_PORT,
                    ..NetworkState::starting(false, fluxy_net::DISCOVERY_PORT)
                });
                return;
            }
        };

        let interfaces = enumerate().unwrap_or_default();
        let default_route = default_route_interface();
        #[cfg(target_os = "android")]
        let mobile_info = self.mobile_network_info().await;
        #[cfg(not(target_os = "android"))]
        let mobile_info: Option<tauri_plugin_fluxy_mobile::LocalNetworkInfo> = None;
        let binding = match choose_binding(
            &config,
            &interfaces,
            default_route.as_deref(),
            mobile_info.as_ref(),
        ) {
            Ok(binding) => binding,
            Err(error) => {
                // The plan's 2003: the chosen interface or address is not usable,
                // and the message says what to do about it.
                self.set_state(NetworkState {
                    state: "unavailable".into(),
                    detail: error.to_string(),
                    selection_mode: config.network.mode.as_str().into(),
                    interface_options: interface_options(
                        &interfaces,
                        default_route.as_deref(),
                        None,
                    ),
                    dev_loopback: config.dev_allow_loopback,
                    discovery_port: config.discovery_port,
                    ..NetworkState::starting(config.dev_allow_loopback, config.discovery_port)
                });
                return;
            }
        };
        let options = interface_options(
            &interfaces,
            default_route.as_deref(),
            Some(&binding.selection.interface.name),
        );
        let listen = SocketAddr::new(binding.selection.address, config.listen_port);
        let (endpoint, pinned) =
            match server_endpoint_bound(&tls, listen, Some(&binding.selection.binding())).await {
                Ok(value) => value,
                Err(error) => {
                    self.set_state(
                        NetworkState {
                            state: "unavailable".into(),
                            detail: format!("listener failed: {error}"),
                            interface_options: options,
                            dev_loopback: config.dev_allow_loopback,
                            discovery_port: config.discovery_port,
                            ..NetworkState::starting(
                                config.dev_allow_loopback,
                                config.discovery_port,
                            )
                        }
                        .with_selection(&binding, false),
                    );
                    return;
                }
            };
        // Remember the real port: the config usually says 0 ("let the OS pick")
        // and the UI needs something a peer can dial.
        if let Ok(mut engine) = self.engine.lock()
            && let Ok(actual) = endpoint.local_addr()
        {
            engine.set_bound_port(actual.port());
        }
        let actual = match endpoint.local_addr() {
            Ok(actual) => actual,
            Err(error) => {
                self.set_state(
                    NetworkState {
                        state: "unavailable".into(),
                        detail: format!("listener address unavailable: {error}"),
                        interface_options: options,
                        dev_loopback: config.dev_allow_loopback,
                        discovery_port: config.discovery_port,
                        ..NetworkState::starting(config.dev_allow_loopback, config.discovery_port)
                    }
                    .with_selection(&binding, pinned),
                );
                return;
            }
        };

        let mut base_state = NetworkState {
            state: "listening".into(),
            detail: "The LAN listener is ready; discovery is starting.".into(),
            interface: None,
            local_address: None,
            listen_addr: Some(actual.to_string()),
            discovery_port: config.discovery_port,
            advertised: false,
            browsing: false,
            local_addresses: Vec::new(),
            dev_loopback: config.dev_allow_loopback,
            interface_options: options,
            ..NetworkState::starting(config.dev_allow_loopback, config.discovery_port)
        }
        .with_selection(&binding, pinned);
        self.set_state(base_state.clone());

        let discovery_config = DiscoveryConfig {
            interface: Some(binding.selection.interface.discovery_name().to_string()),
            allow_loopback: binding.loopback(),
            peer_ttl: DEFAULT_PEER_TTL,
        };
        let mdns = MdnsDiscovery::start(
            identity.device_id(),
            &config.device_name,
            std::env::consts::OS,
            actual.port(),
            tls.fingerprint,
            discovery_config,
        )
        .await;
        let mdns_error = mdns.as_ref().err().map(ToString::to_string);
        if let Some(error) = &mdns_error {
            tracing::warn!(%error, "mDNS discovery is unavailable");
        }

        // Android hides the interface table and the default route from apps, so
        // the gateway the fallback probe aims at comes from the plugin; desktop
        // reads it from the OS.
        #[cfg(target_os = "android")]
        let gateway = self.mobile_network_gateway().await;
        #[cfg(not(target_os = "android"))]
        let gateway = default_gateway();
        tracing::info!(
            gateway = ?gateway,
            interface = %binding.selection.interface.label,
            "starting the fallback probe"
        );
        // On Android the interface table is unreadable and the selection is a
        // wildcard, so its name ("any") is not a device the fallback could bind
        // to; leave the choice to the OS.
        #[cfg(target_os = "android")]
        let interface = None;
        #[cfg(not(target_os = "android"))]
        let interface = Some(binding.selection.interface.discovery_name().to_string());
        let fallback_config = FallbackConfig {
            bind_port: config.discovery_port,
            interface,
            allow_loopback: binding.loopback(),
            peer_ttl: FALLBACK_PEER_TTL,
            probe_interval: FALLBACK_PROBE_INTERVAL,
            broadcast: true,
            gateway,
            probe_targets: Vec::new(),
        };
        let fallback = FallbackDiscovery::start(
            identity.device_id(),
            &config.device_name,
            std::env::consts::OS,
            actual.port(),
            tls.fingerprint,
            fallback_config,
            self.fallback_stats.clone(),
        )
        .await;
        let fallback_error = fallback.as_ref().err().map(ToString::to_string);
        if let Some(error) = &fallback_error {
            tracing::warn!(%error, "UDP fallback discovery could not start");
        }
        let mdns_ok = mdns.is_ok();
        let fallback_ok = fallback.is_ok();
        self.fallback_active.store(fallback_ok, Ordering::SeqCst);
        if let Some(error) = &fallback_error
            && let Ok(mut stats) = self.fallback_stats.lock()
        {
            stats.last_error = Some(format!("start failed: {error}"));
        }

        base_state.advertised = mdns_ok || fallback_ok;
        base_state.browsing = mdns_ok || fallback_ok;
        base_state.state = if mdns_ok || fallback_ok {
            "discovering".into()
        } else {
            "listener_only".into()
        };
        base_state.detail = match (&mdns_error, &fallback_error) {
            (None, None) => "mDNS discovery and UDP gateway fallback are active on the LAN.".into(),
            (None, Some(error)) => {
                format!("mDNS discovery is active; UDP fallback unavailable: {error}")
            }
            (Some(error), None) => {
                format!("mDNS unavailable: {error}; UDP gateway fallback is active.")
            }
            (Some(mdns), Some(fallback)) => {
                format!("mDNS unavailable: {mdns}; UDP fallback unavailable: {fallback}")
            }
        };
        self.set_state(base_state);

        self.spawn_accept(endpoint.clone(), shutdown.clone());
        if let Ok(discovery) = mdns {
            self.spawn_discovery(discovery, shutdown.clone());
        }
        if let Ok(fallback) = fallback {
            let gateway = self
                .fallback_stats
                .lock()
                .ok()
                .and_then(|stats| stats.gateway.clone());
            tracing::info!(
                gateway = ?gateway,
                port = config.discovery_port,
                "UDP gateway discovery fallback is active"
            );
            self.spawn_fallback(fallback, shutdown.clone());
        }
        // Android has no mDNS socket and no interface table, so the platform
        // resolver feeds the nearby table instead.
        #[cfg(target_os = "android")]
        self.spawn_platform_discovery(actual.port(), shutdown.clone());
        self.spawn_monitor(binding, config.dev_allow_loopback, shutdown);
    }

    /// Watch the interface table every two seconds.
    ///
    /// Automatic mode follows the network. Manual mode reports 2003 when the
    /// user's address disappears and waits for it to come back, so a laptop that
    /// sleeps and wakes on the same network recovers without a silent downgrade.
    fn spawn_monitor(
        self: &Arc<Self>,
        binding: Binding,
        allow_loopback: bool,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let runtime = self.clone();
        let mut current = binding.selection;
        let mode = binding.mode;
        tauri::async_runtime::spawn(async move {
            let mut ticker = tokio::time::interval(INTERFACE_POLL_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            let mut lost = false;
            loop {
                tokio::select! {
                    _ = shutdown.changed() => return,
                    _ = ticker.tick() => {}
                }
                let interfaces = match enumerate() {
                    Ok(interfaces) => interfaces,
                    Err(error) => {
                        tracing::warn!(%error, "interface enumeration failed");
                        continue;
                    }
                };
                let default_route = default_route_interface();
                let options = interface_options(
                    &interfaces,
                    default_route.as_deref(),
                    Some(&current.interface.name),
                );
                runtime.update_interface_options(options);
                match reconcile(
                    &interfaces,
                    default_route.as_deref(),
                    mode,
                    &current,
                    allow_loopback,
                ) {
                    Reconcile::Keep => {
                        if lost {
                            lost = false;
                            tracing::info!("the chosen address is back");
                        }
                    }
                    Reconcile::Rebind(selection) => {
                        current = *selection;
                        runtime.rebind(&current, "the network changed");
                        return;
                    }
                    Reconcile::Lost(detail) => {
                        if !lost {
                            lost = true;
                            tracing::warn!(%detail, "the bound address is gone");
                        }
                        runtime.mark_lost(&current, &detail);
                    }
                }
            }
        });
    }

    /// Rebind the listener after a network change. Active transfers pause first so
    /// they keep their checkpoints and resume through the normal flow.
    fn rebind(self: &Arc<Self>, selection: &Selection, reason: &str) {
        tracing::info!(
            address = %selection.display_address(),
            interface = %selection.interface.name,
            label = %selection.interface.label,
            reason,
            "rebinding the LAN listener");
        self.run_rebind_hook();
        if let Ok(mut state) = self.state.lock() {
            state.rebind_count = state.rebind_count.saturating_add(1);
            state.last_rebind_ms = Some(now_ms());
        }
        self.restart();
    }

    /// Report a vanished address without changing the selection (2003).
    fn mark_lost(&self, selection: &Selection, detail: &str) {
        let options = self
            .state
            .lock()
            .map(|state| state.interface_options.clone())
            .unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.state = "unavailable".into();
            state.detail = format!(
                "{detail}; pick another interface in settings, or plug the network back in"
            );
            state.local_address = Some(selection.display_address());
            state.interface = Some(selection.interface.label.clone());
            state.interface_index = Some(selection.interface.index);
            state.selection_source = selection.source.as_str().into();
            state.interface_options = options;
        }
        self.hub.bump_snapshot();
    }

    fn update_interface_options(&self, options: Vec<InterfaceOptionView>) {
        if let Ok(mut state) = self.state.lock() {
            state.interface_options = options;
        }
        self.hub.bump_snapshot();
    }

    /// Change the selection from the settings page: persist it, then restart the
    /// LAN runtime so the new address is bound immediately.
    pub fn apply_selection(&self, selection: fluxy_net::NetworkSelection) -> Result<(), IpcError> {
        {
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| IpcError::internal("engine state poisoned"))?;
            let preference = fluxy_app::NetworkPreference {
                mode: selection.mode,
                interface_id: selection.interface_id.clone(),
                address: selection.address.map(|address| {
                    fluxy_net::interface::format_address(address, selection.interface_id.as_deref())
                }),
            };
            engine
                .set_network_selection(preference)
                .map_err(|error| IpcError {
                    code: "network_unavailable".into(),
                    code_number: error.code_number,
                    message: error.to_string(),
                    retryable: true,
                })?;
        }
        // Publish the mode the user just chose before the runtime moves.
        // The restart is asynchronous, so a snapshot taken here would still
        // describe the previous binding - and the settings panel seeds itself
        // from that snapshot, which is how an applied manual selection flipped
        // back to automatic for a second.
        if let Ok(mut state) = self.state.lock() {
            state.selection_mode = selection.mode.as_str().into();
        }
        self.hub.bump_snapshot();
        Ok(())
    }

    fn startup_identity(&self) -> Result<(AppConfig, fluxy_core::Identity, TlsIdentity), String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine state poisoned".to_string())?;
        Ok((
            engine.config().clone(),
            engine.identity().clone(),
            engine.tls().clone(),
        ))
    }

    fn network_identity(
        &self,
    ) -> Result<(fluxy_core::Identity, TlsIdentity, TrustStore, Uuid), (String, String)> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| ("internal".into(), "engine state poisoned".into()))?;
        Ok((
            engine.identity().clone(),
            engine.tls().clone(),
            engine.trust_snapshot(),
            engine.identity().device_id(),
        ))
    }

    fn spawn_accept(
        self: &Arc<Self>,
        endpoint: quinn::Endpoint,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tracing::info!("the accept loop is running");
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            tracing::info!("the accept loop stopping");
                            break;
                        }
                    }
                    connection = accept(&endpoint) => {
                        match connection {
                            Ok(connection) => {
                                tracing::info!(
                                    remote = %connection.remote_address(),
                                    "the accept loop took a connection"
                                );
                                let runtime = runtime.clone();
                                tauri::async_runtime::spawn(async move {
                                    runtime.handle_incoming(connection).await;
                                });
                            }
                            Err(error) => {
                                tracing::info!(%error, "incoming connection was not accepted");
                                // A closed endpoint never recovers; looping here would
                                // spin forever and every later dial would time out with
                                // no explanation.
                                if error.to_string().contains("endpoint closed") {
                                    tracing::warn!("the accept loop stopped: the endpoint is closed");
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    fn spawn_discovery(
        self: &Arc<Self>,
        mut discovery: MdnsDiscovery,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            break;
                        }
                    }
                    event = discovery.next_event() => {
                        match event {
                            Some(DiscoveryEvent::Found(record)) => {
                                tracing::info!(
                                    peer = %record.name,
                                    platform = %record.platform,
                                    addresses = ?record.addresses,
                                    "mDNS discovered a peer"
                                );
                                runtime.upsert_nearby(*record, DiscoverySource::Mdns)
                            }
                            Some(DiscoveryEvent::Lost(device_id)) => {
                                runtime.remove_nearby_from(device_id, DiscoverySource::Mdns)
                            }
                            None => break,
                        }
                    }
                }
            }
        });
    }

    /// Poll the platform resolver and feed the same nearby table mDNS feeds.
    #[cfg(target_os = "android")]
    fn spawn_platform_discovery(self: &Arc<Self>, port: u16, shutdown: watch::Receiver<bool>) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let Some(handle) = runtime.mobile.get().cloned() else {
                return;
            };
            let Some((device_id, name, fingerprint)) = runtime.identity_for_discovery() else {
                return;
            };
            // The local network permission is answered after this task starts, so
            // a refusal is retried instead of being final; the plugin asks for it
            // again on every attempt.
            let mut logged = false;
            loop {
                match handle
                    .discovery_start(port, device_id.clone(), name.clone(), fingerprint.clone())
                    .await
                {
                    Ok(true) => {
                        tracing::info!("NsdManager discovery started");
                        break;
                    }
                    Ok(false) => {
                        if !logged {
                            tracing::info!(
                                "waiting for the local network permission before discovery starts"
                            );
                            logged = true;
                        }
                    }
                    Err(error) => {
                        if !logged {
                            tracing::warn!(%error, "NsdManager discovery failed to start");
                            logged = true;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let mut shutdown = shutdown;
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            break;
                        }
                    }
                    _ = ticker.tick() => {
                        match handle.discovery_peers().await {
                            Ok(peers) => {
                                for peer in peers {
                                    // NsdManager also resolves this device's own
                                    // service, and offering to pair with yourself
                                    // is not a feature.
                                    if peer.device_id == device_id {
                                        continue;
                                    }
                                    if let Some(record) = peer_record(&peer) {
                                        runtime.upsert_nearby(record, DiscoverySource::Nsd);
                                    }
                                }
                            }
                            Err(error) => tracing::warn!(%error, "could not read discovered peers"),
                        }
                    }
                }
            }
            let _ = handle.discovery_stop().await;
        });
    }

    /// Identity fields NsdManager needs for its TXT records.
    #[cfg(target_os = "android")]
    fn identity_for_discovery(&self) -> Option<(String, String, String)> {
        let guard = self.engine.lock().ok()?;
        Some((
            guard.identity().device_id().to_string(),
            guard.config().device_name.clone(),
            guard
                .tls()
                .fingerprint
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        ))
    }

    fn spawn_fallback(
        self: &Arc<Self>,
        mut discovery: FallbackDiscovery,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            break;
                        }
                    }
                    event = discovery.next_event() => {
                        match event {
                            Some(FallbackEvent::Found(record)) => {
                                tracing::info!(
                                    peer = %record.name,
                                    platform = %record.platform,
                                    "UDP fallback discovered a peer"
                                );
                                runtime.upsert_nearby(*record, DiscoverySource::Gateway)
                            }
                            Some(FallbackEvent::Lost(device_id)) => {
                                runtime.remove_nearby_from(device_id, DiscoverySource::Gateway)
                            }
                            None => break,
                        }
                    }
                }
            }
        });
    }

    fn upsert_nearby(&self, record: PeerRecord, source: DiscoverySource) {
        let now = now_ms();
        let peer = NearbyPeer {
            device_id: record.device_id,
            name: record.name,
            platform: record.platform,
            manufacturer: record.manufacturer,
            model: record.model,
            os_version: record.os_version,
            addresses: record.addresses,
            port: record.port,
            cert_fingerprint: record.cert_fingerprint,
            last_seen_ms: now,
            interface: record.interface,
            source,
        };
        let mut keep_mdns = false;
        let mut existed = false;
        let mut summary = None;
        if let Ok(mut peers) = self.nearby.lock() {
            if keeps_mdns(peers.get(&peer.device_id).map(|peer| peer.source), source) {
                // mDNS owns the address, but a cached record may predate the
                // hardware fields and nothing refreshes them. The gateway packet
                // is the only other source, so absorb what it knows instead of
                // discarding the whole update - which is what left a phone
                // showing nothing but "windows" for a Windows 11 laptop.
                if let Some(existing) = peers.get_mut(&peer.device_id)
                    && existing.manufacturer.is_empty()
                    && !peer.manufacturer.is_empty()
                {
                    existing.manufacturer.clone_from(&peer.manufacturer);
                    existing.model.clone_from(&peer.model);
                    existing.os_version.clone_from(&peer.os_version);
                    summary = Some(existing.summary());
                }
                keep_mdns = true;
            } else {
                let mut peer = peer;
                if let Some(previous) = peers.get(&peer.device_id) {
                    // A resolver caches what a previous instance of the peer
                    // advertised; the UDP probe hears the current port every two
                    // seconds, so its value wins when the two disagree.
                    if previous.source == DiscoverySource::Gateway
                        && source != DiscoverySource::Gateway
                    {
                        peer.port = previous.port;
                    }
                    // Union, then sort, so the first address is the same one on
                    // every refresh instead of whichever channel reported last.
                    peer.addresses.extend(previous.addresses.iter().copied());
                    peer.addresses.sort();
                    peer.addresses.dedup();
                    // A resolver can hold an advertisement from before the
                    // hardware fields existed, and an empty value must never
                    // erase one that is already known.
                    if peer.manufacturer.is_empty() {
                        peer.manufacturer.clone_from(&previous.manufacturer);
                        peer.model.clone_from(&previous.model);
                        peer.os_version.clone_from(&previous.os_version);
                    }
                }
                // Discovery is the only thing that knows where the peer lives
                // right now, and a chat session may still hold the address from
                // before the peer restarted and rebound its socket.
                if let Some(first) = peer.addresses.first().copied() {
                    self.chat
                        .update_peer_addr(peer.device_id, SocketAddr::new(first, peer.port));
                }
                summary = Some(peer.summary());
                existed = peers.insert(peer.device_id, peer).is_some();
                tracing::info!(nearby_len = peers.len(), "nearby map after upsert");
            }
        }
        if keep_mdns {
            // mDNS is preferred when both channels see the same device. A
            // fallback advertisement must not replace its addresses or prove
            // liveness after the mDNS record expires.
            return;
        }
        let Some(summary) = summary else {
            return;
        };
        self.hub.emit(AppEvent::Peer {
            data: if existed {
                PeerEvent::Updated { peer: summary }
            } else {
                PeerEvent::Discovered { peer: summary }
            },
        });
    }

    fn remove_nearby_from(&self, device_id: Uuid, source: DiscoverySource) {
        let removed = self
            .nearby
            .lock()
            .map(|mut peers| {
                if peers.get(&device_id).map(|peer| peer.source) != Some(source) {
                    return false;
                }
                peers.remove(&device_id).is_some()
            })
            .unwrap_or(false);
        if removed {
            self.hub.emit(AppEvent::Peer {
                data: PeerEvent::Lost {
                    device_id: device_id.to_string(),
                },
            });
        }
    }

    fn clear_nearby(&self) {
        let removed = self
            .nearby
            .lock()
            .map(|mut peers| std::mem::take(&mut *peers))
            .unwrap_or_default();
        for device_id in removed.keys() {
            self.hub.emit(AppEvent::Peer {
                data: PeerEvent::Lost {
                    device_id: device_id.to_string(),
                },
            });
        }
    }

    async fn handle_incoming(self: Arc<Self>, connection: quinn::Connection) {
        tracing::info!("incoming pairing connection accepted");
        if self
            .pairing_active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            tracing::warn!("incoming connection rejected: another pairing is active");
            connection.close(0u32.into(), b"pairing-busy");
            return;
        }
        let (identity, tls, trust, identity_id) = match self.network_identity() {
            Ok(value) => value,
            Err((code, message)) => {
                self.pairing_active.store(false, Ordering::SeqCst);
                tracing::warn!(%code, %message, "incoming pairing could not read identity");
                connection.close(0u32.into(), b"engine-unavailable");
                return;
            }
        };
        let mut trust = trust;
        let mut limiter = self.limiter.lock().await;
        let challenge_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let receiver_slot: Arc<AsyncMutex<Option<oneshot::Receiver<bool>>>> =
            Arc::new(AsyncMutex::new(None));
        // Show the code as soon as the handshake derives it, not when the other
        // side acknowledges: both screens then display the same six digits and a
        // person can actually compare them.
        let on_code = {
            let runtime = self.clone();
            let challenge_id = challenge_id.clone();
            let receiver_slot = receiver_slot.clone();
            move |prompt: &PairingPrompt| {
                let Some((id, receiver)) = runtime.start_incoming_challenge(prompt) else {
                    return;
                };
                if !runtime.ready(&id, prompt) {
                    return;
                }
                if let Ok(mut slot) = challenge_id.lock() {
                    *slot = Some(id);
                }
                if let Ok(mut slot) = receiver_slot.try_lock() {
                    *slot = Some(receiver);
                }
            }
        };
        let confirm = {
            let receiver_slot = receiver_slot.clone();
            move |_prompt: PairingPrompt| {
                let receiver_slot = receiver_slot.clone();
                async move {
                    let receiver = receiver_slot.lock().await.take();
                    match receiver {
                        Some(receiver) => receiver.await.unwrap_or(false),
                        None => false,
                    }
                }
            }
        };
        let result = server_pair_async(
            &connection,
            &identity,
            &tls,
            &mut trust,
            &mut limiter,
            on_code,
            confirm,
        )
        .await;
        drop(limiter);

        let challenge = challenge_id.lock().ok().and_then(|slot| slot.clone());
        match result {
            Ok(session) if challenge.is_none() => {
                // A trusted resume continues on this connection as a chat
                // session; there is no pairing transcript to close.
                self.pairing_active.store(false, Ordering::SeqCst);
                self.chat.accept(connection, session);
                return;
            }
            Ok(session) => {
                if let Some(challenge) = challenge {
                    self.complete_success(&challenge, &session, identity_id);
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "incoming handshake failed");
                if let Some(challenge) = challenge {
                    self.fail_pairing(&challenge, &handshake_failure(error));
                } else {
                    self.pairing_active.store(false, Ordering::SeqCst);
                }
            }
        }
        // PairReply or PairRejection is the last stream write. Wait briefly so
        // the peer can read it before the QUIC close frame races in.
        tokio::time::sleep(Duration::from_millis(150)).await;
        connection.close(0u32.into(), b"pair-finished");
    }

    fn start_incoming_challenge(
        &self,
        prompt: &PairingPrompt,
    ) -> Option<(String, oneshot::Receiver<bool>)> {
        let challenge_id = Uuid::now_v7().to_string();
        let (sender, receiver) = oneshot::channel();
        let pending = PendingPairing {
            challenge_id: challenge_id.clone(),
            peer_device_id: prompt.peer_device_id.to_string(),
            peer_name: prompt.peer_name.clone(),
            peer_platform: prompt.peer_platform.clone(),
            direction: "responder".into(),
            sas: None,
            expires_at: None,
            sender: Some(sender),
        };
        self.challenges
            .lock()
            .ok()?
            .insert(challenge_id.clone(), pending);
        Some((challenge_id, receiver))
    }

    fn ready(&self, challenge_id: &str, prompt: &PairingPrompt) -> bool {
        let mut challenges = match self.challenges.lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        let Some(pending) = challenges.get_mut(challenge_id) else {
            tracing::warn!(%challenge_id, "pairing challenge disappeared before the code was ready");
            return false;
        };
        let expires = Instant::now() + PAIRING_TTL;
        pending.sas = Some(prompt.sas.clone());
        pending.expires_at = Some(expires);
        let expires_at_ms = now_ms() + PAIRING_TTL.as_millis() as i64;
        tracing::info!(%challenge_id, "pairing code is ready; waiting for the user");
        let event = PairingEvent::SasReady {
            challenge_id: challenge_id.to_string(),
            peer_device_id: pending.peer_device_id.clone(),
            peer_name: pending.peer_name.clone(),
            peer_platform: pending.peer_platform.clone(),
            direction: pending.direction.clone(),
            sas: prompt.sas.clone(),
            expires_at_ms,
        };
        drop(challenges);
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Pairing { data: event });
        true
    }

    pub(crate) fn complete_success(
        &self,
        challenge_id: &str,
        session: &PeerSession,
        identity_id: Uuid,
    ) {
        let pending = self
            .challenges
            .lock()
            .ok()
            .and_then(|mut challenges| challenges.remove(challenge_id));
        let Some(pending) = pending else {
            self.pairing_active.store(false, Ordering::SeqCst);
            return;
        };
        let entry = session.trust_entry();
        let result = self
            .engine
            .lock()
            .map_err(|_| "engine state poisoned".to_string())
            .and_then(|mut engine| {
                if engine.identity().device_id() != identity_id {
                    return Err("identity changed during pairing".to_string());
                }
                engine
                    .insert_trust(entry.clone())
                    .map_err(|error| error.message)
            });
        self.pairing_active.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => {
                self.hub.bump_snapshot();
                let summary = PeerSummary {
                    trusted: true,
                    online: true,
                    ..PeerSummary::from(&entry)
                };
                self.hub.emit(AppEvent::Trust {
                    data: crate::contract::TrustEvent::Added {
                        peer: summary.clone(),
                    },
                });
                self.hub.emit(AppEvent::Peer {
                    data: PeerEvent::Updated { peer: summary },
                });
                self.hub.emit(AppEvent::Pairing {
                    data: PairingEvent::Succeeded {
                        challenge_id: challenge_id.to_string(),
                        device_id: entry.device_id.to_string(),
                        device_name: entry.name,
                    },
                });
            }
            Err(message) => {
                self.hub.bump_snapshot();
                self.hub.emit(AppEvent::Pairing {
                    data: PairingEvent::Failed {
                        challenge_id: challenge_id.to_string(),
                        device_id: pending.peer_device_id,
                        code: "trust_write_failed".into(),
                        message,
                    },
                });
            }
        }
    }

    pub(crate) fn fail_pairing(&self, challenge_id: &str, failure: &(String, String)) {
        let pending = self
            .challenges
            .lock()
            .ok()
            .and_then(|mut challenges| challenges.remove(challenge_id));
        self.pairing_active.store(false, Ordering::SeqCst);
        let Some(pending) = pending else {
            return;
        };
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Pairing {
            data: PairingEvent::Failed {
                challenge_id: challenge_id.to_string(),
                device_id: pending.peer_device_id,
                code: failure.0.clone(),
                message: failure.1.clone(),
            },
        });
    }

    fn set_state(&self, state: NetworkState) {
        let state_for_event = state.clone();
        if let Ok(mut current) = self.state.lock() {
            *current = state;
        }
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Engine {
            data: EngineEvent::NetworkChanged {
                state: state_for_event.state,
                detail: state_for_event.detail,
            },
        });
    }
}

/// What the listener binds: the plan's selection plus the facts the UI needs.
#[derive(Debug, Clone)]
struct Binding {
    selection: Selection,
    mode: SelectionMode,
    local_addresses: Vec<String>,
}

impl Binding {
    fn loopback(&self) -> bool {
        self.selection.interface.is_loopback()
    }

    /// The interface name the UI shows. The settings page sends the platform
    /// identifier from `interface_options` back, never this string.
    fn interface(&self) -> Option<String> {
        Some(self.selection.interface.label.clone())
    }
}

/// Turn a plugin discovery answer into the runtime's own record. A malformed
/// fingerprint means the peer cannot be trusted enough to pair with, so it is
/// dropped rather than shown with a broken pin.
#[cfg(target_os = "android")]
fn peer_record(peer: &tauri_plugin_fluxy_mobile::DiscoveredPeer) -> Option<PeerRecord> {
    let device_id = Uuid::parse_str(&peer.device_id).ok()?;
    let fingerprint = peer.fingerprint.trim();
    if fingerprint.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&fingerprint[index * 2..index * 2 + 2], 16).ok()?;
    }
    let address: IpAddr = peer.address.parse().ok()?;
    let now = Instant::now();
    Some(PeerRecord {
        device_id,
        name: peer.name.clone(),
        platform: peer.platform.clone(),
        // A gateway advertisement carries no hardware hints either.
        manufacturer: String::new(),
        model: String::new(),
        os_version: String::new(),
        protocol_version: fluxy_proto::PROTOCOL_VERSION,
        features: Vec::new(),
        cert_fingerprint: bytes,
        addresses: vec![address],
        port: peer.port,
        interface: None,
        first_seen: now,
        last_seen: now,
        expires_at: now + DEFAULT_PEER_TTL,
    })
}

/// Pick the local address from the persisted preference. A manual selection that
/// no longer exists is an error (the plan's 2003), never a silent downgrade.
///
/// The interface table answers whenever it exists, on every platform. Android
/// used to skip it on the belief that SELinux always hides the interfaces; a
/// car head unit reports wlan0 and eth0, and with the table ignored a manual
/// pick bound the platform's default address instead - the setting applied,
/// the IP never moved.
fn choose_binding(
    config: &AppConfig,
    interfaces: &[InterfaceInfo],
    default_route: Option<&str>,
    mobile: Option<&tauri_plugin_fluxy_mobile::LocalNetworkInfo>,
) -> Result<Binding, SelectionError> {
    if !interfaces.is_empty() {
        let local_addresses = interfaces
            .iter()
            .filter(|interface| !interface.is_loopback())
            .flat_map(|interface| {
                interface.addresses.iter().map(|address| {
                    format!("{} {}", interface.label, address.display(&interface.label))
                })
            })
            .collect::<Vec<_>>();
        let selection = match config.network.mode {
            SelectionMode::Manual => select_manual(
                interfaces,
                config.network.interface_id.as_deref().unwrap_or_default(),
                config.network.address.as_deref().unwrap_or_default(),
                config.dev_allow_loopback,
            )?,
            SelectionMode::Auto => {
                select_auto(interfaces, default_route, config.dev_allow_loopback)?
            }
        };
        return Ok(Binding {
            selection,
            mode: config.network.mode,
            local_addresses,
        });
    }

    // No table: some Android builds hide sysfs and proc under SELinux, and the
    // platform plugin is then the only source. Binding the real address
    // matters - 0.0.0.0 leaves the socket path-ambiguous, measured out as a
    // 32 s delay before an inbound QUIC stream and as lost control frames -
    // and SO_BINDTODEVICE is unavailable there, so the address is the lever.
    if let SelectionMode::Manual = config.network.mode {
        // A manual pick cannot be honoured without a table. Saying so beats
        // binding another address and reporting success.
        return Err(SelectionError::MissingInterface(
            config.network.interface_id.clone().unwrap_or_default(),
        ));
    }
    let parsed = mobile
        .map(|info| {
            info.addresses
                .iter()
                .filter_map(|text| text.parse::<IpAddr>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // A private IPv4 is the LAN path; anything else is a last resort.
    let address = parsed
        .iter()
        .copied()
        .find(|address| matches!(address, IpAddr::V4(v4) if v4.is_private()))
        .or_else(|| {
            parsed
                .iter()
                .copied()
                .find(|address| !address.is_loopback())
        })
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let name = mobile
        .and_then(|info| info.interface.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "any".into());
    tracing::info!(%address, %name, candidates = parsed.len(), "the listener binds the platform address");
    Ok(Binding {
        selection: Selection {
            interface: InterfaceInfo {
                label: name.clone(),
                name,
                description: None,
                index: 0,
                kind: fluxy_net::interface::InterfaceKind::Other,
                up: true,
                addresses: Vec::new(),
            },
            address,
            source: fluxy_net::interface::SelectionSource::FirstAvailable,
        },
        mode: config.network.mode,
        local_addresses: Vec::new(),
    })
}

/// What the two-second interface poll decided.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(target_os = "android", allow(dead_code))]
enum Reconcile {
    /// The current address is still the right one.
    Keep,
    /// A different address is now the right one.
    Rebind(Box<Selection>),
    /// The user's chosen address is gone; report 2003 and wait for it to return.
    Lost(String),
}

/// Compare the live interface table with what is bound.
///
/// Automatic mode follows the network: a better candidate (or the current one
/// disappearing) moves the listener. Manual mode never downgrades silently; a
/// vanished address is reported and the listener stays where the user put it
/// until it comes back.
fn reconcile(
    interfaces: &[InterfaceInfo],
    default_route: Option<&str>,
    mode: SelectionMode,
    current: &Selection,
    allow_loopback: bool,
) -> Reconcile {
    // The Android listener is bound to the wildcard address and never moves;
    // there is no interface table to reconcile against.
    #[cfg(target_os = "android")]
    {
        let _ = (interfaces, default_route, mode, current, allow_loopback);
        return Reconcile::Keep;
    }
    #[cfg(not(target_os = "android"))]
    {
        match mode {
            SelectionMode::Manual => {
                let present = interfaces.iter().find(|interface| {
                    interface.name.eq_ignore_ascii_case(&current.interface.name)
                        && interface.up
                        && interface.address(current.address).is_some()
                });
                match present {
                    Some(interface) if interface.index == current.interface.index => {
                        Reconcile::Keep
                    }
                    Some(interface) => Reconcile::Rebind(Box::new(Selection {
                        interface: interface.clone(),
                        address: current.address,
                        source: current.source,
                    })),
                    None => Reconcile::Lost(format!(
                        "the chosen address {} on {} is gone",
                        current.display_address(),
                        current.interface.label
                    )),
                }
            }
            SelectionMode::Auto => match select_auto(interfaces, default_route, allow_loopback) {
                Err(error) => Reconcile::Lost(error.to_string()),
                Ok(selection)
                    if selection.address == current.address
                        && selection.interface.name == current.interface.name
                        && selection.interface.index == current.interface.index =>
                {
                    Reconcile::Keep
                }
                Ok(selection) => Reconcile::Rebind(Box::new(selection)),
            },
        }
    }
}
/// The interface list the settings page shows, best candidate first.
fn interface_options(
    interfaces: &[InterfaceInfo],
    default_route: Option<&str>,
    recommended: Option<&str>,
) -> Vec<InterfaceOptionView> {
    display_order(interfaces, default_route)
        .into_iter()
        .map(|interface| InterfaceOptionView {
            addresses: interface
                .addresses
                .iter()
                .map(|address| address.display(&interface.label))
                .collect(),
            recommended: recommended.is_some_and(|name| name.eq_ignore_ascii_case(&interface.name)),
            default_route: default_route
                .is_some_and(|name| name.eq_ignore_ascii_case(&interface.name)),
            virtual_adapter: interface.is_virtual(),
            up: interface.up,
            kind: interface.kind.as_str().into(),
            index: interface.index,
            label: interface.label.clone(),
            name: interface.name.clone(),
        })
        .collect()
}

fn handshake_failure(error: HandshakeError) -> (String, String) {
    let code = match &error {
        HandshakeError::Timeout => "timeout",
        HandshakeError::SasMismatch => "sas_mismatch",
        HandshakeError::Untrusted(_) | HandshakeError::Identity(_) => "trust",
        HandshakeError::RateLimited { .. } => "rate_limited",
        HandshakeError::Pairing(message) if message.contains("rejected") => "pairing_rejected",
        HandshakeError::Pairing(_) => "pairing_failed",
        HandshakeError::Transport(_) | HandshakeError::Protocol(_) => "transport",
        HandshakeError::NoPeerCertificate | HandshakeError::Core(_) | HandshakeError::Random(_) => {
            "pairing_failed"
        }
    };
    (code.into(), error.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(
        name: &str,
        index: u32,
        kind: fluxy_net::interface::InterfaceKind,
        addresses: &[&str],
    ) -> InterfaceInfo {
        InterfaceInfo {
            name: name.to_string(),
            label: name.to_string(),
            description: None,
            index,
            kind,
            up: true,
            addresses: addresses
                .iter()
                .map(|text| fluxy_net::interface::AddressInfo::new(text.parse().expect("address")))
                .collect(),
        }
    }

    fn wifi() -> InterfaceInfo {
        interface(
            "Wi-Fi",
            12,
            fluxy_net::interface::InterfaceKind::Wireless,
            &["192.168.1.20"],
        )
    }

    fn ethernet() -> InterfaceInfo {
        interface(
            "Ethernet",
            7,
            fluxy_net::interface::InterfaceKind::Ethernet,
            &["10.0.0.5"],
        )
    }

    fn selection(interface: InterfaceInfo, address: &str) -> Selection {
        Selection {
            interface,
            address: address.parse().expect("address"),
            source: fluxy_net::interface::SelectionSource::DefaultRoute,
        }
    }

    /// A manual pick on a machine with more than one NIC binds that NIC. The
    /// platform facts are ignored, which is what failed on the car head unit:
    /// the Android branch used the plugin address and the chosen one never
    /// reached the listener.
    #[test]
    fn a_manual_pick_beats_the_platform_address() {
        let config = AppConfig {
            network: fluxy_app::NetworkPreference {
                mode: SelectionMode::Manual,
                interface_id: Some("Ethernet".into()),
                address: Some("10.0.0.5".into()),
            },
            ..AppConfig::default()
        };
        let platform = tauri_plugin_fluxy_mobile::LocalNetworkInfo {
            gateway: None,
            interface: Some("wlan0".into()),
            addresses: vec!["192.168.43.4".into()],
            wifi: true,
            hotspot: false,
        };

        let binding = choose_binding(&config, &[wifi(), ethernet()], None, Some(&platform))
            .expect("the table answers");

        assert_eq!(binding.selection.address.to_string(), "10.0.0.5");
        assert_eq!(binding.selection.interface.name, "Ethernet");
        assert_eq!(binding.mode, SelectionMode::Manual);
    }

    /// Without a table the platform address is all there is, so a manual pick
    /// is an error rather than a silent bind of something else.
    #[test]
    fn a_manual_pick_without_a_table_is_refused() {
        let config = AppConfig {
            network: fluxy_app::NetworkPreference {
                mode: SelectionMode::Manual,
                interface_id: Some("eth0".into()),
                address: Some("192.168.195.2".into()),
            },
            ..AppConfig::default()
        };
        let platform = tauri_plugin_fluxy_mobile::LocalNetworkInfo {
            gateway: None,
            interface: Some("wlan0".into()),
            addresses: vec!["192.168.43.4".into()],
            wifi: true,
            hotspot: false,
        };

        let result = choose_binding(&config, &[], None, Some(&platform));
        assert!(
            result.is_err(),
            "a manual pick must not bind another address"
        );
    }

    /// With no table and automatic mode, the platform link is the answer.
    #[test]
    fn automatic_mode_falls_back_to_the_platform_address() {
        let config = AppConfig::default();
        let platform = tauri_plugin_fluxy_mobile::LocalNetworkInfo {
            gateway: None,
            interface: Some("wlan0".into()),
            addresses: vec!["192.168.43.4".into()],
            wifi: true,
            hotspot: false,
        };

        let binding = choose_binding(&config, &[], None, Some(&platform)).expect("platform");
        assert_eq!(binding.selection.address.to_string(), "192.168.43.4");
        assert_eq!(binding.selection.interface.name, "wlan0");
    }
    #[test]
    fn automatic_mode_follows_a_better_interface() {
        let current = selection(wifi(), "192.168.1.20");
        // The same table keeps the same address.
        assert_eq!(
            reconcile(
                &[wifi(), ethernet()],
                Some("Wi-Fi"),
                SelectionMode::Auto,
                &current,
                false
            ),
            Reconcile::Keep
        );
        // The default route moved to Ethernet: the listener follows it.
        match reconcile(
            &[wifi(), ethernet()],
            Some("Ethernet"),
            SelectionMode::Auto,
            &current,
            false,
        ) {
            Reconcile::Rebind(next) => {
                assert_eq!(next.interface.name, "Ethernet");
                assert_eq!(next.address.to_string(), "10.0.0.5");
            }
            other => panic!("expected a rebind, got {other:?}"),
        }
    }

    #[test]
    fn automatic_mode_reports_a_missing_network() {
        let current = selection(wifi(), "192.168.1.20");
        match reconcile(&[], None, SelectionMode::Auto, &current, false) {
            Reconcile::Lost(detail) => assert!(detail.contains("connect Ethernet or Wi-Fi")),
            other => panic!("expected a loss, got {other:?}"),
        }
    }

    #[test]
    fn manual_mode_never_downgrades_silently() {
        let current = selection(wifi(), "192.168.1.20");
        // Still there: keep.
        assert_eq!(
            reconcile(
                &[wifi(), ethernet()],
                Some("Ethernet"),
                SelectionMode::Manual,
                &current,
                false
            ),
            Reconcile::Keep
        );
        // A better interface appearing does not move a manual choice.
        assert_eq!(
            reconcile(
                &[wifi(), ethernet()],
                Some("Ethernet"),
                SelectionMode::Manual,
                &current,
                false
            ),
            Reconcile::Keep
        );
        // Gone: 2003 with a message that names the address.
        match reconcile(&[ethernet()], None, SelectionMode::Manual, &current, false) {
            Reconcile::Lost(detail) => {
                assert!(detail.contains("192.168.1.20"), "{detail}");
                assert!(detail.contains("Wi-Fi"), "{detail}");
            }
            other => panic!("expected a loss, got {other:?}"),
        }
        // A disabled adapter keeps the name but loses the address.
        let mut down = wifi();
        down.up = false;
        assert!(matches!(
            reconcile(
                &[down, ethernet()],
                None,
                SelectionMode::Manual,
                &current,
                false
            ),
            Reconcile::Lost(_)
        ));
    }

    #[test]
    fn manual_mode_rebinds_when_the_index_changes() {
        let current = selection(wifi(), "192.168.1.20");
        let mut renumbered = wifi();
        renumbered.index = 99;
        match reconcile(
            &[renumbered, ethernet()],
            None,
            SelectionMode::Manual,
            &current,
            false,
        ) {
            Reconcile::Rebind(next) => assert_eq!(next.interface.index, 99),
            other => panic!("expected a rebind, got {other:?}"),
        }
    }

    #[test]
    fn the_interface_options_mark_the_recommendation() {
        let interfaces = vec![ethernet(), wifi()];
        let options = interface_options(&interfaces, Some("Wi-Fi"), Some("Ethernet"));
        assert_eq!(options.len(), 2);
        // The default-route interface comes first in the list, while the
        // recommendation can be a different interface (here a manual pick).
        assert_eq!(options[0].name, "Wi-Fi");
        assert!(options[0].default_route);
        assert!(!options[0].recommended);
        assert_eq!(options[1].name, "Ethernet");
        assert!(options[1].recommended);
        assert!(!options[1].default_route);
        assert_eq!(options[1].kind, "ethernet");
        assert_eq!(options[1].addresses, vec!["10.0.0.5".to_string()]);
    }

    #[test]
    fn a_manual_binding_uses_the_chosen_address() {
        let config = AppConfig {
            network: fluxy_app::NetworkPreference::manual("Wi-Fi", "192.168.1.20"),
            ..AppConfig::default()
        };
        let binding = choose_binding(&config, &[wifi(), ethernet()], Some("Ethernet"), None)
            .expect("a manual binding");
        assert_eq!(binding.selection.interface.name, "Wi-Fi");
        assert_eq!(binding.selection.address.to_string(), "192.168.1.20");
        assert_eq!(binding.mode, SelectionMode::Manual);
        assert!(!binding.loopback());
        assert_eq!(binding.interface().as_deref(), Some("Wi-Fi"));
    }

    #[test]
    fn a_manual_binding_for_a_missing_interface_is_2003() {
        let config = AppConfig {
            network: fluxy_app::NetworkPreference::manual("Ethernet 2", "10.0.0.9"),
            ..AppConfig::default()
        };
        let error =
            choose_binding(&config, &[wifi()], None, None).expect_err("a missing interface");
        let error: fluxy_core::Error = error.into();
        assert_eq!(error.code_number, 2003);
        assert!(error.message.contains("Ethernet 2"));
    }

    #[test]
    fn an_automatic_binding_follows_the_default_route() {
        let config = AppConfig::default();
        let binding = choose_binding(&config, &[wifi(), ethernet()], Some("Wi-Fi"), None)
            .expect("an automatic binding");
        assert_eq!(binding.selection.interface.name, "Wi-Fi");
        assert_eq!(binding.mode, SelectionMode::Auto);
        assert!(!binding.local_addresses.is_empty());
    }

    #[test]
    fn nearby_peer_prefers_ipv4_lan_address() {
        let peer = NearbyPeer {
            device_id: Uuid::now_v7(),
            name: "peer".into(),
            platform: "test".into(),
            manufacturer: String::new(),
            model: String::new(),
            os_version: String::new(),
            addresses: vec!["fe80::1".parse().unwrap(), "192.168.1.20".parse().unwrap()],
            port: 4242,
            cert_fingerprint: [7; 32],
            last_seen_ms: 0,
            interface: Some("test".into()),
            source: DiscoverySource::Mdns,
        };
        assert_eq!(
            peer.socket_addr(),
            Some(SocketAddr::new("192.168.1.20".parse().unwrap(), 4242))
        );
    }

    #[test]
    fn nearby_peer_never_prefers_loopback() {
        let peer = NearbyPeer {
            device_id: Uuid::now_v7(),
            name: "peer".into(),
            platform: "test".into(),
            manufacturer: String::new(),
            model: String::new(),
            os_version: String::new(),
            addresses: vec!["127.0.0.1".parse().unwrap(), "fd00::2".parse().unwrap()],
            port: 4242,
            cert_fingerprint: [8; 32],
            last_seen_ms: 0,
            interface: Some("test".into()),
            source: DiscoverySource::Mdns,
        };
        assert_eq!(
            peer.socket_addr(),
            Some(SocketAddr::new("fd00::2".parse().unwrap(), 4242))
        );
    }

    #[test]
    fn mdns_is_preferred_over_the_gateway_fallback() {
        assert!(keeps_mdns(
            Some(DiscoverySource::Mdns),
            DiscoverySource::Gateway
        ));
        assert!(!keeps_mdns(
            Some(DiscoverySource::Gateway),
            DiscoverySource::Mdns
        ));
        assert!(!keeps_mdns(
            Some(DiscoverySource::Gateway),
            DiscoverySource::Gateway
        ));
        assert!(!keeps_mdns(None, DiscoverySource::Gateway));
    }
}
