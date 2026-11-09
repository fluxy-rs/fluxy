//! Tauri-facing IPC contract and application state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fluxy_app::{AppConfig, Engine, EngineStatus, Error};
use fluxy_core::{ClipboardPolicy, TrustEntry};
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::chat::{ChatPeer, ChatRuntime};
use crate::clipboard::{CLIPBOARD_POLL_INTERVAL, ClipboardWatcher};
use crate::network::NetworkRuntime;
use crate::notify::Notifier;
use crate::startup::{StartupBackend, platform_backend};
use crate::transfer::TransferRuntime;

/// Stable error shape returned by every Tauri command.
#[derive(Debug, Clone, Serialize, Type)]
pub struct IpcError {
    pub code: String,
    pub code_number: u16,
    pub message: String,
    pub retryable: bool,
}

impl IpcError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal".into(),
            code_number: 5001,
            message: message.into(),
            retryable: false,
        }
    }
}

impl From<Error> for IpcError {
    fn from(value: Error) -> Self {
        let code = serde_json::to_value(value.code)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "internal".into());
        Self {
            code,
            code_number: value.code_number,
            message: value.message,
            retryable: value.retryable,
        }
    }
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct EngineStatusView {
    pub version: String,
    pub protocol_version: u32,
    pub device_id: String,
    pub device_name: String,
    pub data_dir: String,
    pub listen_port: u16,
    pub trusted_device_count: u32,
    pub dev_loopback: bool,
    /// "android", "windows", "macos", "linux". The WebView picks its layout from
    /// this rather than sniffing the user agent.
    pub platform: String,
    /// This device's certificate fingerprint, lowercase hex. Carried by the
    /// pairing QR code so a scanner can pin the peer without discovery.
    pub identity_fingerprint: String,
}
impl From<EngineStatus> for EngineStatusView {
    fn from(value: EngineStatus) -> Self {
        Self {
            version: value.version,
            protocol_version: value.protocol_version,
            device_id: value.device_id,
            device_name: value.device_name,
            data_dir: value.data_dir,
            listen_port: value.listen_port,
            trusted_device_count: value.trusted_device_count as u32,
            dev_loopback: value.dev_loopback,
            platform: std::env::consts::OS.to_string(),
            identity_fingerprint: value.identity_fingerprint,
        }
    }
}

/// System bar insets in logical pixels. Zero on desktop.
#[derive(Debug, Clone, Copy, Serialize, Type)]
pub struct SystemInsets {
    pub top: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct SettingsSnapshot {
    pub device_name: String,
    pub listen_port: u16,
    pub discovery_port: u16,
    pub clipboard_max_bytes: u64,
    pub auto_clipboard: bool,
    pub max_active_transfers: u32,
    pub notifications: bool,
    /// Whether an incoming transfer from a trusted peer is accepted without
    /// asking. Off by default; the offer path reads it per offer.
    pub auto_accept_transfers: bool,
    /// Whether the app is registered as a login item. The login item itself is
    /// managed by the platform backend; this mirrors the persisted preference.
    pub autostart: bool,
    /// Where the login item lives, for the help page. Empty when the platform
    /// has no file or registry location to show.
    pub autostart_location: String,
    /// Where received files land, as the runtime currently has it. On Android
    /// this is the folder label (e.g. `Downloads/Fluxy`) rather than a path.
    pub receive_dir: String,
    /// Whether a receive destination exists. Desktop always has one; Android
    /// needs the user to pick a folder through the system picker.
    pub receive_dir_ready: bool,
    pub dev_loopback: bool,
}

impl From<&AppConfig> for SettingsSnapshot {
    fn from(value: &AppConfig) -> Self {
        Self {
            device_name: value.device_name.clone(),
            listen_port: value.listen_port,
            discovery_port: value.discovery_port,
            clipboard_max_bytes: value.clipboard_max_bytes,
            auto_clipboard: value.auto_clipboard,
            max_active_transfers: value.max_active_transfers,
            notifications: value.notifications,
            auto_accept_transfers: value.auto_accept_transfers,
            autostart: value.autostart,
            autostart_location: String::new(),
            receive_dir: String::new(),
            receive_dir_ready: cfg!(not(target_os = "android")),
            dev_loopback: value.dev_allow_loopback,
        }
    }
}

/// One entry of the local interface list. It is a settings-page view only: the
/// list never enters the protocol, and no peer ever learns it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct InterfaceOptionView {
    /// Platform interface name, the value the settings page sends back.
    pub name: String,
    /// Friendly name for display.
    pub label: String,
    pub kind: String,
    pub index: u32,
    pub up: bool,
    /// Virtual adapters (VPN, hypervisor, container) are usable but never
    /// preferred.
    pub virtual_adapter: bool,
    pub default_route: bool,
    /// Addresses as the UI shows them, with the scope on a link-local address.
    pub addresses: Vec<String>,
    /// The interface the automatic picker would choose right now.
    pub recommended: bool,
}

/// User-visible state of the LAN discovery and listener runtime. It contains
/// no keys and no peer-controlled HTML; strings are rendered as text only.
#[derive(Debug, Clone, Serialize, Type)]
pub struct NetworkStatusView {
    pub state: String,
    pub detail: String,
    pub interface: Option<String>,
    pub local_address: Option<String>,
    pub listen_addr: Option<String>,
    pub discovery_port: u16,
    pub mdns_service: String,
    pub advertised: bool,
    pub browsing: bool,
    pub nearby_count: u32,
    pub local_addresses: Vec<String>,
    pub dev_loopback: bool,
    /// "auto" or "manual".
    pub selection_mode: String,
    /// Why the current address was chosen: default_route, private_address,
    /// first_available, loopback, or manual.
    pub selection_source: String,
    pub interface_index: Option<u32>,
    /// False when the platform has no interface-pinning option (or refused it),
    /// so routing may still send traffic elsewhere.
    pub interface_pinned: bool,
    /// How many times a network change moved the listener.
    pub rebind_count: u32,
    pub last_rebind_ms: Option<i64>,
    /// The interface list, best candidate first.
    pub interface_options: Vec<InterfaceOptionView>,
    /// UDP gateway/limited-broadcast fallback status. It is a diagnostic only:
    /// peers found this way still need the TLS pin and SAS confirmation.
    pub fallback_active: bool,
    pub fallback_gateway: Option<String>,
    pub fallback_targets: Vec<String>,
    pub fallback_probes_sent: u64,
    pub fallback_responses: u64,
    pub fallback_peers: u32,
    pub fallback_last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct PairingChallengeView {
    pub challenge_id: String,
    pub peer_device_id: String,
    pub peer_name: String,
    pub direction: String,
}

/// A device discovery has seen but that is not paired yet.
#[derive(Debug, Clone, Serialize, Type)]
pub struct NearbyPeerView {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    /// Same hardware hints as PeerSummary; empty from an older peer.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
    pub address: Option<String>,
    pub port: u16,
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct PeerSummary {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    /// Hardware hints for the device list. Empty when the peer does not report
    /// them - older builds, or a hand-typed address.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
    pub trusted: bool,
    pub online: bool,
    pub clipboard_policy: ClipboardReceivePolicy,
    /// Per-device override for automatic acceptance. `Inherit` means the
    /// global setting in `SettingsSnapshot` decides.
    pub auto_accept: DeviceAutoAccept,
    pub paired_at_ms: i64,
    pub last_seen_ms: Option<i64>,
    pub addresses: Vec<String>,
}

impl From<&TrustEntry> for PeerSummary {
    fn from(value: &TrustEntry) -> Self {
        Self {
            device_id: value.device_id.to_string(),
            name: value.name.clone(),
            platform: value.platform.clone(),
            // The trust store predates these fields and does not persist them;
            // a live advertisement fills them in while the peer is around.
            manufacturer: String::new(),
            model: String::new(),
            os_version: String::new(),
            trusted: true,
            online: false,
            clipboard_policy: value.clipboard_policy.into(),
            auto_accept: value.auto_accept.into(),
            paired_at_ms: value.paired_at_ms,
            last_seen_ms: None,
            addresses: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    /// An incoming offer that is waiting for the user's decision.
    Offered,
    /// Waiting for a free active-transfer slot: a queued send, or an accepted
    /// receive whose pull has not started yet.
    Queued,
    /// Left over from a previous run: the checkpoint is on disk but nothing is
    /// driving it, so the user can decide to retry or drop it.
    Interrupted,
    Preparing,
    Transferring,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// One user action from the transfer center. Retry is a new attempt that
/// reuses the same transfer id so a receiver can resume from its checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferAction {
    Pause,
    Resume,
    Cancel,
    Retry,
}

/// A local file the user picked. The path never crosses the IPC boundary.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RegisteredFileView {
    pub file_id: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferSummary {
    pub transfer_id: String,
    pub device_id: String,
    pub device_name: String,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub file_name: String,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_per_second: u64,
    pub total_files: u32,
    pub error_code: Option<String>,
    pub error_code_number: Option<u16>,
    pub error_message: Option<String>,
    pub retryable: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Set when this transfer is one file of a folder batch. The UI groups by
    /// it and never needs the entries themselves.
    pub batch_id: Option<String>,
    /// Display name of the batch root directory.
    pub batch_root: Option<String>,
    /// This file's path inside the batch root.
    pub relative_path: Option<String>,
}

/// A folder the user picked for a batch send. Only the opaque id and a summary
/// cross the IPC boundary; the paths stay in the runtime.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RegisteredFolderView {
    pub folder_id: String,
    pub root_name: String,
    pub total_files: u32,
    pub total_bytes: u64,
    pub empty_directories: u32,
    /// The first few entries, so the confirmation can show what will travel.
    pub sample: Vec<RegisteredFolderEntry>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RegisteredFolderEntry {
    pub relative_path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Started {
        version: String,
        device_id: String,
        device_name: String,
        started_at_ms: i64,
    },
    Stopped {
        uptime_ms: u64,
    },
    Heartbeat {
        uptime_ms: u64,
    },
    NetworkChanged {
        state: String,
        detail: String,
    },
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PeerEvent {
    Discovered { peer: PeerSummary },
    Updated { peer: PeerSummary },
    Lost { device_id: String },
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PairingEvent {
    SasReady {
        challenge_id: String,
        peer_device_id: String,
        peer_name: String,
        peer_platform: String,
        direction: String,
        sas: String,
        expires_at_ms: i64,
    },
    Succeeded {
        challenge_id: String,
        device_id: String,
        device_name: String,
    },
    Failed {
        challenge_id: String,
        device_id: String,
        code: String,
        message: String,
    },
    Cancelled {
        challenge_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChatDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChatMessageState {
    Sending,
    Delivered,
    Failed,
    Received,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ChatMessageView {
    pub message_id: String,
    pub device_id: String,
    pub direction: ChatDirection,
    pub text: String,
    pub state: ChatMessageState,
    pub created_at_ms: i64,
    pub sequence: u64,
    pub copy_hint: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ChatConversationView {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub online: bool,
    pub session_state: String,
    pub last_message: Option<String>,
    pub last_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ChatSnapshot {
    pub conversations: Vec<ChatConversationView>,
    pub messages: Vec<ChatMessageView>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Connecting {
        device_id: String,
    },
    Connected {
        device_id: String,
        session_id: String,
        connected_at_ms: i64,
    },
    Disconnected {
        device_id: String,
        reason: String,
        at_ms: i64,
    },
    Received {
        message_id: String,
        device_id: String,
        text: String,
        copy_hint: bool,
        received_at_ms: i64,
    },
    Delivered {
        message_id: String,
        device_id: String,
        delivered_at_ms: i64,
    },
    Failed {
        message_id: String,
        device_id: String,
        code: String,
        message: String,
        retryable: bool,
    },
}

/// Per-device receiver policy. `Ask` prompts for every offer; `Auto` is only
/// selected explicitly by the user; `Reject` refuses without prompting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardReceivePolicy {
    Ask,
    Auto,
    Reject,
}

/// Per-device automatic acceptance of incoming files.
///
/// Three states rather than a boolean, because "not set" is a real answer: it
/// means the global setting applies. A boolean would force every device to
/// carry a copy of the global value and silently stop following it when that
/// setting changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DeviceAutoAccept {
    Inherit,
    Auto,
    Ask,
}

impl From<fluxy_core::AutoAcceptPolicy> for DeviceAutoAccept {
    fn from(value: fluxy_core::AutoAcceptPolicy) -> Self {
        match value {
            fluxy_core::AutoAcceptPolicy::Inherit => Self::Inherit,
            fluxy_core::AutoAcceptPolicy::Auto => Self::Auto,
            fluxy_core::AutoAcceptPolicy::Ask => Self::Ask,
        }
    }
}

impl From<DeviceAutoAccept> for fluxy_core::AutoAcceptPolicy {
    fn from(value: DeviceAutoAccept) -> Self {
        match value {
            DeviceAutoAccept::Inherit => Self::Inherit,
            DeviceAutoAccept::Auto => Self::Auto,
            DeviceAutoAccept::Ask => Self::Ask,
        }
    }
}

impl From<ClipboardPolicy> for ClipboardReceivePolicy {
    fn from(value: ClipboardPolicy) -> Self {
        match value {
            ClipboardPolicy::Ask => Self::Ask,
            ClipboardPolicy::Auto => Self::Auto,
            ClipboardPolicy::Reject => Self::Reject,
        }
    }
}

impl From<ClipboardReceivePolicy> for ClipboardPolicy {
    fn from(value: ClipboardReceivePolicy) -> Self {
        match value {
            ClipboardReceivePolicy::Ask => Self::Ask,
            ClipboardReceivePolicy::Auto => Self::Auto,
            ClipboardReceivePolicy::Reject => Self::Reject,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardShareState {
    Offered,
    Accepted,
    Sending,
    Applied,
    Failed,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ClipboardShareView {
    pub offer_id: String,
    pub device_id: String,
    pub direction: ClipboardDirection,
    pub state: ClipboardShareState,
    pub preview: String,
    pub size_bytes: u64,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ClipboardSnapshot {
    pub shares: Vec<ClipboardShareView>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipboardEvent {
    Offered {
        offer_id: String,
        device_id: String,
        preview: String,
        size_bytes: u64,
        expires_at_ms: i64,
    },
    OfferReceived {
        offer_id: String,
        device_id: String,
        text_preview: String,
        size_bytes: u64,
        expires_at_ms: i64,
    },
    Accepted {
        offer_id: String,
        device_id: String,
    },
    Applied {
        offer_id: String,
        device_id: String,
    },
    ApplyFailed {
        offer_id: String,
        device_id: String,
        code: String,
        message: String,
    },
    Rejected {
        offer_id: String,
        device_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransferEvent {
    /// An incoming offer that needs the user's approval before it can start.
    Offered {
        transfer_id: String,
        device_id: String,
        device_name: String,
        file_name: String,
        file_size: u64,
        total_files: u32,
    },
    /// A transfer waiting for a free active-transfer slot: a queued send, or an
    /// accepted receive whose pull has not started yet.
    Queued {
        transfer_id: String,
        direction: TransferDirection,
        device_id: String,
        device_name: String,
        file_name: String,
        total_files: u32,
        total_bytes: u64,
    },
    /// A transfer that just started: it owns a slot now.
    Prepared {
        transfer_id: String,
        direction: TransferDirection,
        device_id: String,
        device_name: String,
        file_name: String,
        total_files: u32,
        total_bytes: u64,
    },
    /// Throttled to at most one event per 100 ms by the runtime.
    Progress {
        transfer_id: String,
        bytes_done: u64,
        bytes_total: u64,
        bytes_per_second: u64,
    },
    Paused {
        transfer_id: String,
    },
    Resumed {
        transfer_id: String,
    },
    Completed {
        transfer_id: String,
    },
    Cancelled {
        transfer_id: String,
    },
    Failed {
        transfer_id: String,
        code: String,
        message: String,
        retryable: bool,
    },
    FileAccessRequired {
        transfer_id: String,
        file_id: String,
    },
    /// The window drag state changed; drives the drop overlay.
    DragHover {
        active: bool,
    },
    /// Files were dropped on the window. Only opaque handles and base names
    /// cross this boundary; the local paths stay in the Rust registry.
    FilesDropped {
        files: Vec<RegisteredFileView>,
        /// Dropped folders, already scanned and described. They become batches.
        folders: Vec<RegisteredFolderView>,
        rejected: Vec<String>,
        /// Dropped paths beyond the per-batch limits.
        skipped: u32,
    },
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrustEvent {
    Added { peer: PeerSummary },
    Updated { peer: PeerSummary },
    Removed { device_id: String },
}

/// Shell-level requests raised by native chrome rather than by the engine. The
/// tray is the only producer today: it cannot navigate the WebView itself, so
/// it asks the frontend to show a route.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellEvent {
    /// Show the firewall and help page.
    HelpRequested,
}

/// One tagged event. The category is separate from the variant tag so the
/// frontend can switch on a stable top-level discriminator.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum AppEvent {
    Engine { data: EngineEvent },
    Shell { data: ShellEvent },
    Peer { data: PeerEvent },
    Pairing { data: PairingEvent },
    Chat { data: ChatEvent },
    Clipboard { data: ClipboardEvent },
    Transfer { data: TransferEvent },
    Trust { data: TrustEvent },
    Error { error: IpcError },
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct AppEventEnvelope {
    pub seq: u64,
    pub snapshot_version: u64,
    pub emitted_at_ms: i64,
    pub event: AppEvent,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct SubscriptionInfo {
    pub seq: u64,
    pub snapshot_version: u64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct AppSnapshot {
    pub snapshot_version: u64,
    pub seq: u64,
    pub status: EngineStatusView,
    pub peers: Vec<PeerSummary>,
    pub transfers: Vec<TransferSummary>,
    pub settings: SettingsSnapshot,
    pub network: NetworkStatusView,
    pub chat: ChatSnapshot,
    pub clipboard: ClipboardSnapshot,
}

/// Process-wide event sequence and snapshot version.
pub struct EventHub {
    sender: broadcast::Sender<AppEventEnvelope>,
    seq: AtomicU64,
    snapshot_version: AtomicU64,
    started_at: Instant,
    started_at_ms: i64,
}

impl EventHub {
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(1024);
        Self {
            sender,
            seq: AtomicU64::new(0),
            snapshot_version: AtomicU64::new(1),
            started_at: Instant::now(),
            started_at_ms: now_ms(),
        }
    }

    pub fn emit(&self, event: AppEvent) -> AppEventEnvelope {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let envelope = AppEventEnvelope {
            seq,
            snapshot_version: self.snapshot_version.load(Ordering::SeqCst),
            emitted_at_ms: now_ms(),
            event,
        };
        let _ = self.sender.send(envelope.clone());
        envelope
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEventEnvelope> {
        self.sender.subscribe()
    }

    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    pub fn snapshot_version(&self) -> u64 {
        self.snapshot_version.load(Ordering::SeqCst)
    }

    pub fn bump_snapshot(&self) -> u64 {
        self.snapshot_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    pub fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct AppState {
    engine: Arc<Mutex<Engine>>,
    pub hub: Arc<EventHub>,
    pub network: Arc<NetworkRuntime>,
    pub chat: Arc<ChatRuntime>,
    pub transfer: Arc<TransferRuntime>,
    pub notifier: Arc<Notifier>,
    /// Login-item backend. Injected so tests never touch the real Run key or
    /// autostart directory.
    startup: Arc<dyn StartupBackend>,
    /// Set before a deliberate quit so the window-close handler stops hiding.
    quitting: Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    pub fn new(engine: Engine) -> Self {
        Self::with_startup(engine, platform_backend())
    }

    /// Build the state over an explicit login-item backend so tests never touch
    /// the real Run key or autostart directory.
    pub fn with_startup(engine: Engine, startup: Arc<dyn StartupBackend>) -> Self {
        let engine = Arc::new(Mutex::new(engine));
        let hub = Arc::new(EventHub::new());
        let notifier = Arc::new(Notifier::default());
        // Apply the persisted preference before the runtime can emit anything.
        if let Ok(guard) = engine.lock() {
            notifier.set_enabled(guard.config().notifications);
        }
        let chat = ChatRuntime::with_notifier(engine.clone(), hub.clone(), notifier.clone());
        let transfer = TransferRuntime::new(engine.clone(), hub.clone());
        transfer.attach_chat(chat.clone());
        transfer.attach_notifier(notifier.clone());
        chat.attach_transfer(transfer.clone());
        let network = NetworkRuntime::new(engine.clone(), hub.clone(), chat.clone());
        // A network change pauses active transfers before the listener goes
        // down, so each one keeps its checkpoint instead of failing.
        let transfer_for_rebind = transfer.clone();
        network.attach_rebind_hook(Arc::new(move || transfer_for_rebind.pause_active()));
        Self {
            engine,
            hub,
            network,
            chat,
            transfer,
            notifier,
            startup,
            quitting: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Validate, persist, and apply a new interface selection. A selection that
    /// cannot be parsed or that violates the loopback rule is rejected here with
    /// 2003; an interface that is merely missing is reported by the runtime once
    /// it tries to bind, so the message can name the address.
    pub fn set_network_selection(
        &self,
        mode: &str,
        interface_id: Option<String>,
        address: Option<String>,
    ) -> Result<NetworkStatusView, IpcError> {
        let mode = match mode {
            "auto" => fluxy_net::interface::SelectionMode::Auto,
            "manual" => fluxy_net::interface::SelectionMode::Manual,
            other => {
                return Err(IpcError {
                    code: "network_unavailable".into(),
                    code_number: 2003,
                    message: format!("{other} is not a selection mode"),
                    retryable: false,
                });
            }
        };
        let interface_id = interface_id.filter(|value| !value.trim().is_empty());
        let address = address.filter(|value| !value.trim().is_empty());
        let (port, dev_loopback) = {
            let engine = self.lock_engine()?;
            (
                engine.config().listen_port,
                engine.config().dev_allow_loopback,
            )
        };
        let parsed = match address.as_deref() {
            Some(text) => {
                Some(
                    fluxy_net::interface::parse_address(text).map_err(|error| IpcError {
                        code: "network_unavailable".into(),
                        code_number: 2003,
                        message: error.to_string(),
                        retryable: true,
                    })?,
                )
            }
            None => None,
        };
        let selection = fluxy_net::NetworkSelection {
            mode,
            interface_id,
            address: parsed.map(|(address, _scope)| address),
            port,
            dev_loopback,
        };
        self.network.apply_selection(selection)?;
        self.network.restart();
        Ok(self.network.status())
    }

    pub fn start_network(&self) {
        self.network.start();
        // The listener binds asynchronously; publish a snapshot once it has so
        // the UI shows the port the OS actually chose rather than the 0 the
        // config means "any".
        let hub = self.hub.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            hub.bump_snapshot();
        });
    }

    pub fn stop_network(&self) {
        self.network.stop();
    }

    fn lock_engine(&self) -> Result<MutexGuard<'_, Engine>, IpcError> {
        self.engine
            .lock()
            .map_err(|_| IpcError::internal("engine state poisoned"))
    }

    pub fn status(&self) -> Result<EngineStatusView, IpcError> {
        let engine = self.lock_engine()?;
        Ok(engine.status().into())
    }

    /// The settings view the UI should display. The login-item location lives
    /// outside the config, so every snapshot that reaches the frontend carries
    /// the backend's answer rather than an empty string.
    /// The data directory, for the handful of files the shell puts beside the
    /// engine's own state (the mobile markers).
    pub fn engine_data_dir(&self) -> Option<PathBuf> {
        self.lock_engine()
            .ok()
            .map(|engine| engine.paths().data_dir.clone())
    }

    fn settings_view(&self, config: &AppConfig) -> SettingsSnapshot {
        let mut settings = SettingsSnapshot::from(config);
        settings.autostart_location = self.startup.location();
        settings.receive_dir = self.transfer.receive_root().display().to_string();
        #[cfg(target_os = "android")]
        {
            settings.receive_dir = self.transfer.receive_dir_label();
            settings.receive_dir_ready = self.transfer.receive_dir_ready();
        }
        settings
    }

    /// Mirror the Android folder picker's answer onto the runtime.
    #[cfg(target_os = "android")]
    pub fn set_receive_dir_state(&self, ready: bool, label: &str) {
        self.transfer.set_receive_dir_state(ready, label);
        self.hub.bump_snapshot();
    }

    /// Persist where received files land and switch the runtime to it. A path
    /// that cannot be created or is a file is refused with 3003.
    pub fn set_receive_dir(&self, directory: Option<String>) -> Result<SettingsSnapshot, IpcError> {
        // Android's destination is a SAF grant the picker already stored, not a
        // path the config could hold. Answer with the current view so the
        // caller refreshes instead of writing the label into the config.
        #[cfg(target_os = "android")]
        {
            let _ = directory;
            let engine = self.lock_engine()?;
            Ok(self.settings_view(engine.config()))
        }
        #[cfg(not(target_os = "android"))]
        {
            let directory = directory
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from);
            let config = {
                let mut engine = self.lock_engine()?;
                engine.set_receive_dir(directory)?
            };
            self.transfer.set_receive_root(
                config
                    .receive_dir
                    .clone()
                    .unwrap_or_else(|| self.transfer.receive_root()),
            );
            self.hub.bump_snapshot();
            Ok(self.settings_view(&config))
        }
    }

    /// Persist a new active-transfer cap, start whatever the new room allows,
    /// and return the settings the UI should display.
    pub fn set_transfer_concurrency(&self, limit: u32) -> Result<SettingsSnapshot, IpcError> {
        let config = {
            let mut engine = self.lock_engine()?;
            engine.set_max_active_transfers(limit)?
        };
        self.transfer.pump_queue();
        self.hub.bump_snapshot();
        Ok(self.settings_view(&config))
    }

    /// Persist a new device name. The name is advertised over mDNS and sent in
    /// the handshake, so a real change restarts the LAN runtime the same way an
    /// identity rotation does; the UI warns about that before saving.
    pub fn device_name(&self) -> Option<String> {
        self.lock_engine()
            .ok()
            .map(|engine| engine.config().device_name.clone())
    }

    pub fn set_device_name(&self, name: String) -> Result<SettingsSnapshot, IpcError> {
        if self.network.pairing_in_progress() {
            return Err(IpcError {
                code: "pairing_busy".into(),
                code_number: 4105,
                message: "finish or cancel the current pairing before renaming".into(),
                retryable: true,
            });
        }
        let (config, changed) = {
            let mut engine = self.lock_engine()?;
            let previous = engine.config().device_name.clone();
            let config = engine.set_device_name(&name)?;
            let changed = config.device_name != previous;
            (config, changed)
        };
        // The handshake announces this name; keep it in step with the config.
        fluxy_net::set_local_device_name(config.device_name.clone());
        if changed {
            self.network.restart();
        }
        self.hub.bump_snapshot();
        Ok(self.settings_view(&config))
    }

    /// Persist whether incoming transfers are accepted without asking. Read
    /// when an offer arrives, so changing it mid-session takes effect at once.
    pub fn set_auto_accept(&self, enabled: bool) -> Result<SettingsSnapshot, IpcError> {
        let config = {
            let mut engine = self.lock_engine()?;
            engine.set_auto_accept_transfers(enabled)?
        };
        self.hub.bump_snapshot();
        Ok(self.settings_view(&config))
    }

    /// Persist the desktop-notification preference and apply it immediately.
    pub fn set_notifications(&self, enabled: bool) -> Result<SettingsSnapshot, IpcError> {
        let config = {
            let mut engine = self.lock_engine()?;
            engine.set_notifications(enabled)?
        };
        self.notifier.set_enabled(enabled);
        self.hub.bump_snapshot();
        Ok(self.settings_view(&config))
    }

    /// Persist the automatic clipboard-send switch. The watcher reads the
    /// preference on every tick, so flipping it takes effect immediately.
    pub fn set_auto_clipboard(&self, enabled: bool) -> Result<SettingsSnapshot, IpcError> {
        let config = {
            let mut engine = self.lock_engine()?;
            engine.set_auto_clipboard(enabled)?
        };
        // The tray check item mirrors the same preference, so a change from the
        // settings page keeps the menu honest.
        self.notifier.set_auto_clipboard_checked(enabled);
        self.hub.bump_snapshot();
        Ok(self.settings_view(&config))
    }

    /// Watch the local clipboard while the automatic-send switch is on.
    ///
    /// The plan asks for a 300 ms debounced read, so the watcher only offers a
    /// value that has looked the same for a full debounce window. It never
    /// offers content that arrived from a peer and never offers the same
    /// content twice, which is what keeps two watching devices from looping.
    /// Every offer goes to each online trusted device; the receiver still
    /// applies its own per-device receive policy.
    pub fn start_clipboard_watch(&self) {
        let engine = self.engine.clone();
        let network = self.network.clone();
        let chat = self.chat.clone();
        tauri::async_runtime::spawn(async move {
            let mut watcher = ClipboardWatcher::new();
            let mut watching = false;
            loop {
                tokio::time::sleep(CLIPBOARD_POLL_INTERVAL).await;
                if !auto_clipboard_enabled(&engine) {
                    // Off drops the pending candidate, so a later enable starts
                    // from a fresh observation instead of sending stale content.
                    if watching {
                        watcher.reset();
                        watching = false;
                    }
                    continue;
                }
                if !watching {
                    // Enabling only syncs what is copied from now on; whatever is
                    // already on the clipboard is recorded, not sent.
                    chat.prime_clipboard_watch(&mut watcher);
                    watching = true;
                }
                let now = Instant::now();
                let Some(text) = chat.clipboard_watch_tick(&mut watcher, now) else {
                    continue;
                };
                // Each online trusted device decides for itself; offline devices
                // are skipped so the watcher cannot build a backlog.
                for peer in online_trusted_peers(&engine, &network, &chat) {
                    if let Err(error) = chat.share_clipboard_text(peer, text.clone()).await {
                        tracing::debug!(code = %error.code, "automatic clipboard share skipped");
                    }
                }
            }
        });
    }

    /// Persist the autostart preference and write the platform login item.
    ///
    /// The login item is written first: if the platform refuses, the preference
    /// stays untouched so the UI never claims something that did not happen.
    pub fn set_autostart(&self, enabled: bool) -> Result<SettingsSnapshot, IpcError> {
        let exe = std::env::current_exe().map_err(|error| {
            IpcError::internal(format!("the executable path is unavailable: {error}"))
        })?;
        self.startup
            .set_enabled(enabled, &exe)
            .map_err(|message| IpcError {
                code: "autostart_failed".into(),
                code_number: 4601,
                message,
                retryable: false,
            })?;
        let config = {
            let mut engine = self.lock_engine()?;
            engine.set_autostart(enabled)?
        };
        self.notifier.set_autostart_checked(enabled);
        self.hub.bump_snapshot();
        Ok(self.settings_view(&config))
    }

    /// Repair a login item that a cleaner removed while the preference is on.
    /// Called once at startup, after the tray exists.
    pub fn reconcile_autostart(&self) {
        let enabled = self
            .lock_engine()
            .map(|engine| engine.config().autostart)
            .unwrap_or(false);
        if !enabled {
            return;
        }
        match self.startup.is_enabled() {
            Ok(true) => {}
            Ok(false) => match std::env::current_exe() {
                Ok(exe) => match self.startup.set_enabled(true, &exe) {
                    Ok(()) => tracing::info!("re-created the missing login item"),
                    Err(message) => tracing::warn!(%message, "could not re-create the login item"),
                },
                Err(error) => tracing::warn!(%error, "the executable path is unavailable"),
            },
            Err(message) => tracing::warn!(%message, "could not read the login item"),
        }
    }

    /// Ask the frontend to show the firewall and help page. The tray owns the
    /// entry point, so the shell focuses the window first.
    pub fn request_help(&self) {
        self.hub.emit(AppEvent::Shell {
            data: ShellEvent::HelpRequested,
        });
    }

    /// Mark a deliberate quit so the window-close handler stops hiding. The
    /// tray calls this before it exits.
    pub fn begin_quit(&self) {
        self.quitting
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Closing the window keeps the app running in the tray, which is the
    /// M3-04 criterion: a closed window still receives. A deliberate quit is
    /// the only close that really ends the process.
    pub fn should_hide_on_close(&self) -> bool {
        !self.is_quitting()
    }

    pub fn snapshot(&self) -> Result<AppSnapshot, IpcError> {
        // Capture the cursor before reading mutable state. Any event emitted
        // after this point produces a visible seq/version gap, so the frontend
        // refetches instead of assuming this snapshot included that event.
        let snapshot_version = self.hub.snapshot_version();
        let seq = self.hub.seq();
        let (status, trusted, config) = {
            let engine = self.lock_engine()?;
            (
                EngineStatusView::from(engine.status()),
                engine.trusted_devices(),
                engine.config().clone(),
            )
        };
        let settings = self.settings_view(&config);
        let peers = self.network.peer_summaries(&trusted);
        // Discovery knows what a peer calls itself now; the trust file only has
        // what it called itself at pairing time. Adopt the live name so an
        // offline peer still shows the right label on the next start.
        {
            let observed: Vec<(uuid::Uuid, String)> = peers
                .iter()
                .filter(|peer| peer.trusted && !peer.name.trim().is_empty())
                .filter_map(|peer| {
                    peer.device_id
                        .parse::<uuid::Uuid>()
                        .ok()
                        .map(|id| (id, peer.name.clone()))
                })
                .collect();
            if let Ok(mut engine) = self.lock_engine() {
                let names = observed.iter().map(|(id, name)| (id, name.as_str()));
                let _ = engine.sync_peer_names(names);
            }
        }
        let network = self.network.status();
        let chat = self.chat.snapshot(&trusted, &peers);
        let clipboard = self.chat.clipboard_snapshot();

        Ok(AppSnapshot {
            snapshot_version,
            seq,
            status,
            peers,
            transfers: self.transfer.snapshot(),
            settings,
            network,
            chat,
            clipboard,
        })
    }

    fn chat_peer(&self, device_id: &str) -> Result<ChatPeer, IpcError> {
        resolve_chat_peer(&self.engine, &self.network, &self.chat, device_id)
    }

    pub fn chat_open(&self, device_id: &str) -> Result<(), IpcError> {
        let peer = self.chat_peer(device_id)?;
        self.chat.open(peer)
    }

    pub async fn chat_send(
        &self,
        device_id: &str,
        text: String,
    ) -> Result<crate::contract::ChatMessageView, IpcError> {
        let peer = self.chat_peer(device_id)?;
        self.chat.send(peer, text).await
    }

    pub async fn chat_retry(
        &self,
        device_id: &str,
        message_id: &str,
    ) -> Result<crate::contract::ChatMessageView, IpcError> {
        let peer = self.chat_peer(device_id)?;
        let message_id = Uuid::parse_str(message_id)
            .map_err(|_| IpcError::internal("invalid chat message id"))?;
        self.chat.retry(peer, message_id).await
    }

    pub async fn clipboard_share(&self, device_id: &str) -> Result<ClipboardShareView, IpcError> {
        let peer = self.chat_peer(device_id)?;
        self.chat.share_clipboard(peer).await
    }

    pub async fn clipboard_accept(
        &self,
        device_id: &str,
        offer_id: &str,
    ) -> Result<ClipboardShareView, IpcError> {
        let peer = self.chat_peer(device_id)?;
        let offer_id = Uuid::parse_str(offer_id)
            .map_err(|_| IpcError::internal("invalid clipboard offer id"))?;
        self.chat.accept_clipboard(peer, offer_id).await
    }

    pub async fn clipboard_reject(
        &self,
        device_id: &str,
        offer_id: &str,
    ) -> Result<ClipboardShareView, IpcError> {
        let peer = self.chat_peer(device_id)?;
        let offer_id = Uuid::parse_str(offer_id)
            .map_err(|_| IpcError::internal("invalid clipboard offer id"))?;
        self.chat.reject_clipboard(peer, offer_id).await
    }

    /// Devices discovery has seen, with the trusted flag so the UI can offer a
    /// pair button only where it makes sense.
    pub fn nearby_peers(&self) -> Vec<NearbyPeerView> {
        let trusted: Vec<String> = self
            .engine
            .lock()
            .map(|engine| {
                engine
                    .trusted_devices()
                    .iter()
                    .map(|entry| entry.device_id.to_string())
                    .collect()
            })
            .unwrap_or_default();
        self.network
            .nearby_snapshot()
            .into_iter()
            .map(|peer| {
                let device_id = peer.device_id.to_string();
                NearbyPeerView {
                    trusted: trusted.contains(&device_id),
                    device_id,
                    name: peer.name,
                    platform: peer.platform,
                    manufacturer: peer.manufacturer,
                    model: peer.model,
                    os_version: peer.os_version,
                    // Sorted above, so this is stable across refreshes.
                    address: peer
                        .addresses
                        .iter()
                        .find(|address| address.is_ipv4() && !address.is_loopback())
                        .or_else(|| peer.addresses.iter().find(|address| !address.is_loopback()))
                        .or_else(|| peer.addresses.first())
                        .map(ToString::to_string),
                    port: peer.port,
                }
            })
            .collect()
    }

    pub fn pair_start(&self, device_id: &str) -> Result<PairingChallengeView, IpcError> {
        let peer = self.network.nearby_peer(device_id)?;
        let expected_identity = self.lock_engine()?.identity().device_id();
        let (challenge_id, confirmation) = self.network.begin_outbound(&peer)?;
        let view = PairingChallengeView {
            challenge_id: challenge_id.clone(),
            peer_device_id: peer.device_id.to_string(),
            peer_name: peer.name.clone(),
            direction: "initiator".into(),
        };
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = state
                .network
                .clone()
                .pair_outbound(peer, challenge_id.clone(), confirmation)
                .await;
            match result {
                Ok(session) => {
                    state
                        .network
                        .complete_success(&challenge_id, &session, expected_identity);
                }
                Err(error) => state.network.fail_pairing(&challenge_id, &error),
            }
        });
        Ok(view)
    }

    /// Pair with an address the user typed instead of a discovered peer. This is
    /// the path a NATed emulator or a multicast-filtered network needs.
    pub fn pair_manual(
        &self,
        address: String,
        fingerprint: String,
    ) -> Result<PairingChallengeView, IpcError> {
        // The address arrives as "host:port" (that is what the UI shows and
        // what a pairing code carries), but parse_address() only accepts a
        // bare IP. Splitting the port off first is what makes both forms -
        // "host:port" and a bare "host" that then fails for want of a port -
        // follow one path instead of contradicting each other.
        let (host, port_text) = match address.rsplit_once(':') {
            Some((host, port)) => {
                // Bracketed IPv6: [::1]:8080.
                let host = host.trim_start_matches('[').trim_end_matches(']');
                (host.to_string(), Some(port.to_string()))
            }
            None => (address.clone(), None),
        };
        let (ip, scope) = fluxy_net::interface::parse_address(&host).map_err(|error| IpcError {
            code: "network_unavailable".into(),
            code_number: 2003,
            message: format!("the address is not usable: {error}"),
            retryable: true,
        })?;
        let Some(port) = port_text.and_then(|port| port.parse::<u16>().ok()) else {
            return Err(IpcError {
                code: "network_unavailable".into(),
                code_number: 2003,
                message: "the address needs a port".into(),
                retryable: true,
            });
        };
        let _ = scope;
        let address = std::net::SocketAddr::new(ip, port);
        let trimmed = fingerprint.trim();
        if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(IpcError {
                code: "protocol".into(),
                code_number: 1001,
                message: "the fingerprint must be 64 hex characters".into(),
                retryable: true,
            });
        }
        let mut cert_fingerprint = [0u8; 32];
        for (index, byte) in cert_fingerprint.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16)
                .map_err(|_| IpcError::internal("invalid fingerprint"))?;
        }
        let fingerprint = cert_fingerprint;
        let peer = crate::network::NearbyPeer::manual(address, fingerprint);
        let expected_identity = self.lock_engine()?.identity().device_id();
        let (challenge_id, confirmation) = self.network.begin_outbound(&peer)?;
        let view = PairingChallengeView {
            challenge_id: challenge_id.clone(),
            peer_device_id: peer.device_id.to_string(),
            peer_name: peer.name.clone(),
            direction: "initiator".into(),
        };
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = state
                .network
                .clone()
                .pair_outbound(peer, challenge_id.clone(), confirmation)
                .await;
            match result {
                Ok(session) => {
                    state
                        .network
                        .complete_success(&challenge_id, &session, expected_identity)
                }
                Err(error) => state.network.fail_pairing(&challenge_id, &error),
            }
        });
        Ok(view)
    }

    pub fn pair_confirm(&self, challenge_id: &str, accepted: bool) -> Result<(), IpcError> {
        self.network.confirm(challenge_id, accepted)
    }

    pub fn pair_cancel(&self, challenge_id: &str) -> Result<(), IpcError> {
        self.network.cancel(challenge_id)
    }

    pub fn trust_remove(&self, device_id: &str) -> Result<bool, IpcError> {
        let id = Uuid::parse_str(device_id).map_err(|_| IpcError::internal("invalid device id"))?;
        let removed = self.lock_engine()?.remove_trust(&id)?;
        if removed {
            self.chat.revoke(id);
            self.hub.bump_snapshot();
            self.hub.emit(AppEvent::Trust {
                data: TrustEvent::Removed {
                    device_id: id.to_string(),
                },
            });
        }
        Ok(removed)
    }

    pub fn clipboard_set_policy(
        &self,
        device_id: &str,
        policy: ClipboardReceivePolicy,
    ) -> Result<(), IpcError> {
        let id = Uuid::parse_str(device_id).map_err(|_| IpcError::internal("invalid device id"))?;
        let entry = {
            let mut engine = self.lock_engine()?;
            let previous = engine.set_clipboard_policy(&id, policy.into())?;
            if previous.is_none() {
                return Err(IpcError {
                    code: "not_trusted".into(),
                    code_number: 4202,
                    message: "device is not trusted".into(),
                    retryable: false,
                });
            }
            engine
                .trusted_devices()
                .into_iter()
                .find(|entry| entry.device_id == id)
                .ok_or_else(|| IpcError::internal("trust entry disappeared"))?
        };
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Trust {
            data: TrustEvent::Updated {
                peer: PeerSummary::from(&entry),
            },
        });
        Ok(())
    }

    /// Set or clear the per-device automatic acceptance of incoming files.
    ///
    /// `Inherit` is stored rather than resolved here, so a later change to the
    /// global switch reaches every device that never overrode it.
    pub fn device_set_auto_accept(
        &self,
        device_id: &str,
        policy: DeviceAutoAccept,
    ) -> Result<(), IpcError> {
        let id = Uuid::parse_str(device_id).map_err(|_| IpcError::internal("invalid device id"))?;
        let entry = {
            let mut engine = self.lock_engine()?;
            let previous = engine.set_auto_accept_policy(&id, policy.into())?;
            if previous.is_none() {
                return Err(IpcError {
                    code: "not_trusted".into(),
                    code_number: 4202,
                    message: "device is not trusted".into(),
                    retryable: false,
                });
            }
            engine
                .trusted_devices()
                .into_iter()
                .find(|entry| entry.device_id == id)
                .ok_or_else(|| IpcError::internal("trust entry disappeared"))?
        };
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Trust {
            data: TrustEvent::Updated {
                peer: PeerSummary::from(&entry),
            },
        });
        Ok(())
    }

    pub fn rotate_identity(&self) -> Result<EngineStatusView, IpcError> {
        if self.network.pairing_in_progress() {
            return Err(IpcError {
                code: "pairing_busy".into(),
                code_number: 4105,
                message: "finish or cancel the current pairing before rotating identity".into(),
                retryable: true,
            });
        }
        let (status, removed) = {
            let mut engine = self.lock_engine()?;
            let removed: Vec<String> = engine
                .trusted_devices()
                .iter()
                .map(|entry| entry.device_id.to_string())
                .collect();
            engine.rotate_identity()?;
            (EngineStatusView::from(engine.status()), removed)
        };

        self.hub.bump_snapshot();
        for device_id in removed {
            self.hub.emit(AppEvent::Trust {
                data: TrustEvent::Removed { device_id },
            });
        }
        self.network.restart();
        Ok(status)
    }

    pub fn emit_started(&self) {
        if let Ok(snapshot) = self.snapshot() {
            let status = snapshot.status;
            self.hub.emit(AppEvent::Engine {
                data: EngineEvent::Started {
                    version: status.version,
                    device_id: status.device_id,
                    device_name: status.device_name,
                    started_at_ms: self.hub.started_at_ms(),
                },
            });
        }
    }

    pub fn emit_stopped(&self) {
        self.hub.emit(AppEvent::Engine {
            data: EngineEvent::Stopped {
                uptime_ms: self.hub.uptime_ms(),
            },
        });
    }

    pub fn emit_heartbeat(&self) {
        self.hub.emit(AppEvent::Engine {
            data: EngineEvent::Heartbeat {
                uptime_ms: self.hub.uptime_ms(),
            },
        });
    }
}

/// Resolve a trusted device id into a reachable peer: a live chat session
/// first, then the discovery table. A device that is neither is offline.
fn resolve_chat_peer(
    engine: &Arc<Mutex<Engine>>,
    network: &NetworkRuntime,
    chat: &ChatRuntime,
    device_id: &str,
) -> Result<ChatPeer, IpcError> {
    let id =
        Uuid::parse_str(device_id).map_err(|_| IpcError::internal("invalid chat device id"))?;
    let entry = engine
        .lock()
        .map_err(|_| IpcError::internal("engine state poisoned"))?
        .trusted_devices()
        .into_iter()
        .find(|entry| entry.device_id == id)
        .ok_or_else(|| IpcError {
            code: "not_trusted".into(),
            code_number: 4202,
            message: "device is not trusted for chat".into(),
            retryable: false,
        })?;
    if let Some(peer) = chat.known_peer(id) {
        return Ok(peer);
    }
    let nearby = network.nearby_peer(device_id).map_err(|_| IpcError {
        code: "peer_offline".into(),
        code_number: 4201,
        message: "device is not currently reachable on the LAN".into(),
        retryable: true,
    })?;
    let addr = nearby.socket_addr().ok_or_else(|| IpcError {
        code: "peer_offline".into(),
        code_number: 4201,
        message: "device has no usable LAN address".into(),
        retryable: true,
    })?;
    Ok(ChatPeer {
        device_id: id,
        name: entry.name,
        platform: entry.platform,
        addr,
    })
}

/// Whether the user asked for automatic clipboard sending. Read on every
/// watcher tick so the switch takes effect without a restart.
fn auto_clipboard_enabled(engine: &Arc<Mutex<Engine>>) -> bool {
    engine
        .lock()
        .map(|engine| engine.config().auto_clipboard)
        .unwrap_or(false)
}

/// Every trusted device that is reachable right now.
fn online_trusted_peers(
    engine: &Arc<Mutex<Engine>>,
    network: &NetworkRuntime,
    chat: &ChatRuntime,
) -> Vec<ChatPeer> {
    let ids: Vec<String> = engine
        .lock()
        .map(|engine| {
            engine
                .trusted_devices()
                .iter()
                .map(|entry| entry.device_id.to_string())
                .collect()
        })
        .unwrap_or_default();
    ids.iter()
        .filter_map(|id| resolve_chat_peer(engine, network, chat, id).ok())
        .collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxy_app::AppPaths;
    use fluxy_core::TrustStore;
    use uuid::Uuid;

    fn trust_entry() -> TrustEntry {
        TrustEntry {
            device_id: Uuid::now_v7(),
            name: "peer".into(),
            platform: "test".into(),
            public_key: [1; 32],
            cert_fingerprint: [2; 32],
            paired_at_ms: 1_700_000_000_000,
            clipboard_policy: fluxy_core::ClipboardPolicy::Ask,
            auto_accept: fluxy_core::AutoAcceptPolicy::Inherit,
        }
    }

    /// Login-item backend that records what the shell asked for and can be told
    /// to fail, so the switch is tested without touching the real Run key.
    #[derive(Default)]
    struct FakeStartup {
        enabled: Mutex<bool>,
        fail: bool,
        calls: Mutex<Vec<bool>>,
    }

    impl FakeStartup {
        fn failing() -> Arc<Self> {
            Arc::new(Self {
                enabled: Mutex::new(false),
                fail: true,
                calls: Mutex::new(Vec::new()),
            })
        }

        fn enabled(&self) -> bool {
            *self.enabled.lock().unwrap()
        }
    }

    impl StartupBackend for FakeStartup {
        fn is_enabled(&self) -> Result<bool, String> {
            Ok(self.enabled())
        }

        fn set_enabled(&self, enabled: bool, _exe: &std::path::Path) -> Result<(), String> {
            self.calls.lock().unwrap().push(enabled);
            if self.fail {
                return Err("the login item could not be written".into());
            }
            *self.enabled.lock().unwrap() = enabled;
            Ok(())
        }

        fn location(&self) -> String {
            "test login item".into()
        }
    }

    fn state_with_startup(startup: Arc<FakeStartup>) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let engine = Engine::load(paths).unwrap();
        (dir, AppState::with_startup(engine, startup))
    }

    fn state_with_trust() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let mut trust = TrustStore::default();
        trust.insert(trust_entry());
        trust.save(&paths.trust_file).unwrap();
        let engine = Engine::load(paths).unwrap();
        (dir, AppState::new(engine))
    }

    #[test]
    fn the_login_item_follows_the_switch_and_is_persisted() {
        let startup = Arc::new(FakeStartup::default());
        let (dir, state) = state_with_startup(startup.clone());
        // Off by default, and the location is always reported for diagnostics.
        assert!(!state.snapshot().unwrap().settings.autostart);
        assert_eq!(
            state.snapshot().unwrap().settings.autostart_location,
            "test login item"
        );

        let enabled = state.set_autostart(true).unwrap();
        assert!(enabled.autostart);
        assert!(startup.enabled());
        assert!(state.snapshot().unwrap().settings.autostart);
        let config = AppConfig::load_or_default(&dir.path().join("config.json")).unwrap();
        assert!(config.autostart);

        let disabled = state.set_autostart(false).unwrap();
        assert!(!disabled.autostart);
        assert!(!startup.enabled());
        assert_eq!(startup.calls.lock().unwrap().clone(), vec![true, false]);
    }

    #[test]
    fn a_rejected_login_item_leaves_the_preference_alone() {
        let startup = FakeStartup::failing();
        let (dir, state) = state_with_startup(startup);
        let error = state.set_autostart(true).unwrap_err();
        assert_eq!(error.code, "autostart_failed");
        assert_eq!(error.code_number, 4601);
        assert!(!state.snapshot().unwrap().settings.autostart);
        let config = AppConfig::load_or_default(&dir.path().join("config.json")).unwrap();
        assert!(!config.autostart);
    }

    #[test]
    fn startup_repairs_a_login_item_that_went_missing() {
        let startup = Arc::new(FakeStartup::default());
        let (_dir, state) = state_with_startup(startup.clone());
        state.set_autostart(true).unwrap();
        // A cleaner removed the item while the preference stayed on.
        *startup.enabled.lock().unwrap() = false;
        state.reconcile_autostart();
        assert!(startup.enabled());

        // Nothing is written while the preference is off.
        state.set_autostart(false).unwrap();
        state.reconcile_autostart();
        assert!(!startup.enabled());
    }

    #[test]
    fn every_settings_change_keeps_the_login_item_location() {
        let startup = Arc::new(FakeStartup::default());
        let (_dir, state) = state_with_startup(startup);
        let name = state.snapshot().unwrap().settings.device_name;
        // Re-saving the same name skips the network restart this test does not
        // need; the point is that the location survives every settings reply.
        assert_eq!(
            state.set_auto_clipboard(true).unwrap().autostart_location,
            "test login item"
        );
        assert_eq!(
            state.set_notifications(false).unwrap().autostart_location,
            "test login item"
        );
        assert_eq!(
            state
                .set_transfer_concurrency(3)
                .unwrap()
                .autostart_location,
            "test login item"
        );
        assert_eq!(
            state.set_device_name(name).unwrap().autostart_location,
            "test login item"
        );
        assert_eq!(
            state.snapshot().unwrap().settings.autostart_location,
            "test login item"
        );
    }

    #[test]
    fn closing_the_window_hides_until_the_tray_quits() {
        let startup = Arc::new(FakeStartup::default());
        let (_dir, state) = state_with_startup(startup);
        // M3-04: a closed window keeps receiving in the tray.
        assert!(state.should_hide_on_close());
        assert!(!state.is_quitting());
        state.begin_quit();
        assert!(!state.should_hide_on_close());
        assert!(state.is_quitting());
    }

    #[test]
    fn the_tray_help_entry_asks_the_frontend_for_the_help_page() {
        let startup = Arc::new(FakeStartup::default());
        let (_dir, state) = state_with_startup(startup);
        let mut receiver = state.hub.subscribe();
        state.request_help();
        match receiver.try_recv().unwrap().event {
            AppEvent::Shell {
                data: ShellEvent::HelpRequested,
            } => {}
            other => panic!("expected a help request, got {other:?}"),
        }
    }

    /// The settings page relies on these failures being 2003 with a message it
    /// can act on. A valid selection would start a real listener, so only the
    /// rejection paths are exercised here.
    #[test]
    fn a_bad_network_selection_is_rejected_with_2003() {
        let (_dir, state) = state_with_trust();

        let bad_mode = state
            .set_network_selection("sometimes", None, None)
            .expect_err("an unknown mode");
        assert_eq!(bad_mode.code, "network_unavailable");
        assert_eq!(bad_mode.code_number, 2003);
        assert!(bad_mode.message.contains("sometimes"));

        let bad_address = state
            .set_network_selection(
                "manual",
                Some("Wi-Fi".into()),
                Some("not-an-address".into()),
            )
            .expect_err("an unparsable address");
        assert_eq!(bad_address.code_number, 2003);

        let missing_interface = state
            .set_network_selection("manual", None, Some("192.168.1.20".into()))
            .expect_err("a manual pick without an interface");
        assert_eq!(missing_interface.code_number, 2003);
        assert!(missing_interface.message.contains("interface"));

        let missing_address = state
            .set_network_selection("manual", Some("Wi-Fi".into()), None)
            .expect_err("a manual pick without an address");
        assert_eq!(missing_address.code_number, 2003);
        assert!(missing_address.message.contains("address"));

        // An empty string is the same as no value at all, which is what an
        // untouched form control sends.
        let blank = state
            .set_network_selection("manual", Some("  ".into()), Some("".into()))
            .expect_err("blank fields");
        assert_eq!(blank.code_number, 2003);
    }

    #[test]
    fn event_sequence_is_monotonic_and_shared() {
        let hub = EventHub::new();
        let mut receiver = hub.subscribe();
        let first = hub.emit(AppEvent::Engine {
            data: EngineEvent::Heartbeat { uptime_ms: 1 },
        });
        let second = hub.emit(AppEvent::Engine {
            data: EngineEvent::Heartbeat { uptime_ms: 2 },
        });

        assert_eq!(first.seq + 1, second.seq);
        assert_eq!(first.snapshot_version, hub.snapshot_version());
        assert_eq!(receiver.try_recv().unwrap().seq, first.seq);
        assert_eq!(receiver.try_recv().unwrap().seq, second.seq);
    }

    #[test]
    fn lifecycle_events_are_observed() {
        let (_dir, state) = state_with_trust();
        let expected_name = state.status().unwrap().device_name;
        let mut receiver = state.hub.subscribe();

        state.emit_started();
        match receiver.try_recv().unwrap().event {
            AppEvent::Engine {
                data: EngineEvent::Started { device_name, .. },
            } => assert_eq!(device_name, expected_name),
            other => panic!("expected engine start, got {other:?}"),
        }

        state.emit_stopped();
        match receiver.try_recv().unwrap().event {
            AppEvent::Engine {
                data: EngineEvent::Stopped { .. },
            } => {}
            other => panic!("expected engine stop, got {other:?}"),
        }
    }

    #[test]
    fn event_envelope_has_stable_category_and_type_tags() {
        let hub = EventHub::new();
        let envelope = hub.emit(AppEvent::Engine {
            data: EngineEvent::Heartbeat { uptime_ms: 42 },
        });
        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(value["event"]["category"], "engine");
        assert_eq!(value["event"]["data"]["type"], "heartbeat");
        assert_eq!(value["event"]["data"]["uptime_ms"], 42);
    }

    #[test]
    fn snapshot_lists_trusted_devices_and_settings() {
        let (_dir, state) = state_with_trust();
        let snapshot = state.snapshot().unwrap();

        assert_eq!(snapshot.snapshot_version, 1);
        assert_eq!(snapshot.peers.len(), 1);
        assert_eq!(snapshot.peers[0].name, "peer");
        assert!(snapshot.peers[0].trusted);
        assert_eq!(
            snapshot.settings.device_name,
            state.status().unwrap().device_name
        );
        assert!(snapshot.transfers.is_empty());
    }

    #[test]
    fn renaming_the_device_updates_the_snapshot_and_persists() {
        let (dir, state) = state_with_trust();
        let renamed = state.set_device_name("  Bench\tTop  ".into()).unwrap();
        assert_eq!(renamed.device_name, "BenchTop");
        assert_eq!(state.status().unwrap().device_name, "BenchTop");
        assert_eq!(state.snapshot().unwrap().settings.device_name, "BenchTop");

        let config = AppConfig::load_or_default(&dir.path().join("config.json")).unwrap();
        assert_eq!(config.device_name, "BenchTop");
    }

    #[test]
    fn the_notification_preference_reaches_the_notifier_and_the_snapshot() {
        let (_dir, state) = state_with_trust();
        assert!(state.notifier.enabled());

        let muted = state.set_notifications(false).unwrap();
        assert!(!muted.notifications);
        assert!(!state.notifier.enabled());
        assert!(!state.snapshot().unwrap().settings.notifications);

        let restored = state.set_notifications(true).unwrap();
        assert!(restored.notifications);
        assert!(state.notifier.enabled());
    }

    #[test]
    fn automatic_clipboard_sending_is_off_by_default_and_persists() {
        let (dir, state) = state_with_trust();
        // The plan requires the watcher to be off until the user asks for it.
        assert!(!state.snapshot().unwrap().settings.auto_clipboard);
        assert!(!auto_clipboard_enabled(&state.engine));

        let enabled = state.set_auto_clipboard(true).unwrap();
        assert!(enabled.auto_clipboard);
        assert!(state.snapshot().unwrap().settings.auto_clipboard);
        assert!(auto_clipboard_enabled(&state.engine));

        let config = AppConfig::load_or_default(&dir.path().join("config.json")).unwrap();
        assert!(config.auto_clipboard);

        let disabled = state.set_auto_clipboard(false).unwrap();
        assert!(!disabled.auto_clipboard);
        assert!(!auto_clipboard_enabled(&state.engine));
    }

    #[test]
    fn clipboard_policy_is_per_device_and_persisted() {
        let (dir, state) = state_with_trust();
        let peer_id = state.snapshot().unwrap().peers[0].device_id.clone();
        let mut receiver = state.hub.subscribe();

        state
            .clipboard_set_policy(&peer_id, ClipboardReceivePolicy::Auto)
            .unwrap();
        let snapshot = state.snapshot().unwrap();
        assert_eq!(
            snapshot.peers[0].clipboard_policy,
            ClipboardReceivePolicy::Auto
        );
        assert!(state.hub.snapshot_version() >= 2);
        assert!(matches!(
            receiver.try_recv().unwrap().event,
            AppEvent::Trust {
                data: TrustEvent::Updated { .. }
            }
        ));

        let paths = AppPaths::from_root(dir.path().to_path_buf());
        let trust = TrustStore::load_or_default(&paths.trust_file).unwrap();
        assert_eq!(
            trust.clipboard_policy(&Uuid::parse_str(&peer_id).unwrap()),
            Some(fluxy_core::ClipboardPolicy::Auto)
        );

        let error = state
            .clipboard_set_policy(&Uuid::now_v7().to_string(), ClipboardReceivePolicy::Reject)
            .unwrap_err();
        assert_eq!(error.code, "not_trusted");
    }

    #[test]
    fn rotation_bumps_snapshot_and_emits_trust_removal() {
        let (_dir, state) = state_with_trust();
        let old_id = state.status().unwrap().device_id;
        let peer_id = state.snapshot().unwrap().peers[0].device_id.clone();
        let mut receiver = state.hub.subscribe();

        let status = state.rotate_identity().unwrap();
        assert!(state.hub.snapshot_version() >= 2);
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.snapshot_version, state.hub.snapshot_version());
        assert!(snapshot.peers.is_empty());
        assert!(snapshot.chat.messages.is_empty());
        assert_ne!(status.device_id, old_id);

        match receiver.try_recv().unwrap().event {
            AppEvent::Trust {
                data: TrustEvent::Removed { device_id },
            } => assert_eq!(device_id, peer_id),
            other => panic!("expected trust removal, got {other:?}"),
        }
    }
}
