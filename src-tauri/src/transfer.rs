//! Transfer runtime: the app-layer registry that drives file transfers over an
//! existing control session and publishes throttled UI events.
//!
//! Chunk data and the manifest travel on the transfer data stream; only the
//! offer, the decision, and lifecycle notifications use the control stream. A
//! progress event is coalesced to at most one per PROGRESS_THROTTLE so a fast
//! LAN transfer never floods the WebView.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fluxy_app::Engine;
use fluxy_core::files::{
    DesktopFileAccessManager, FileAccessManager, FileHandleId, FileReference, FileSource,
    scan_folder,
};
use fluxy_core::transfer::{
    BatchEntry, BatchPlan, CheckpointStore, Manifest, ManifestCache, ManifestOptions,
    TransferControl, sanitize_root_name, validate_relative_path,
};
use fluxy_core::{AutoAcceptPolicy, ErrorCode, Identity};
use fluxy_net::session::{
    BatchEntryWire, ControlSender, SessionEvent, TransferMessage, TransferOfferDraft,
};
use fluxy_net::transfer::{
    TransferConfig, TransferOptions, prepare_sink, receive_file_resumable, send_file,
};
use fluxy_net::{CheckpointResume, TransferError};
use quinn::Connection;
use tauri::async_runtime;
use uuid::Uuid;

use crate::chat::ChatRuntime;
use crate::contract::{
    AppEvent, EventHub, IpcError, RegisteredFileView, RegisteredFolderEntry, RegisteredFolderView,
    TransferAction, TransferDirection, TransferEvent, TransferState, TransferSummary,
};
use crate::notify::Notifier;

/// UI progress events are coalesced to this interval (M2-07 acceptance).
pub const PROGRESS_THROTTLE: Duration = Duration::from_millis(100);
/// How long an offer waits for the peer's decision before it times out.
const OFFER_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a sender waits for the control session to come up.
// Ten seconds was tight enough that a chat session which had just dropped its
// idle connection could not be re-established in time, and the send failed with
// a timeout before a single byte moved.
/// How long a queued send waits for the chat session before giving up.
///
/// Measured on a phone-to-desktop send the session took 45 s to come up, past
/// the 30 s this used to allow, so a perfectly healthy transfer failed with
/// "transfer timed out" before it could start. 90 s leaves room for a slow
/// discovery round while still failing a genuinely dead peer.
const SESSION_TIMEOUT: Duration = Duration::from_secs(90);
/// How long a send waits for discovery to report the peer at all.
///
/// A cold-started app can be handed a share before its first probe answers, and
/// failing immediately there is indistinguishable from "the peer is offline".
const PEER_WAIT: Duration = Duration::from_secs(25);
/// Default block size from the protocol limits table.
const BLOCK_SIZE: u32 = 4 * 1024 * 1024;
/// At most this many files can be queued in one send.
pub const MAX_BATCH_FILES: usize = 100;
/// At most this many folders can arrive in one drop.
pub const MAX_DROPPED_FOLDERS: usize = 10;

/// Creation-order stamp for the send queue. Time-ordered so two transfers
/// created in the same millisecond still start in the order they were queued.
static QUEUE_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_queue_seq() -> u64 {
    QUEUE_SEQ.fetch_add(1, Ordering::SeqCst)
}

/// States that own one of the global active-transfer slots.
fn is_active_state(state: TransferState) -> bool {
    matches!(
        state,
        TransferState::Preparing | TransferState::Transferring | TransferState::Paused
    )
}

/// The control-stream handles a transfer task needs. It is cloned out of the
/// chat session and never holds a lock across an await.
#[derive(Clone)]
pub struct TransferSession {
    pub sender: ControlSender,
    pub connection: Connection,
    pub session_id: Uuid,
}

/// Coalesces progress updates to at most one per interval.
#[derive(Debug)]
struct ProgressThrottle {
    interval: Duration,
    last: Option<Instant>,
}

impl ProgressThrottle {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// Returns true at most once per interval. The first call always fires so
    /// the UI shows activity immediately.
    fn ready(&mut self, now: Instant) -> bool {
        match self.last {
            Some(last) if now.duration_since(last) < self.interval => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

/// Best-effort MIME type for the public download entry. The Android exporter just
/// needs something plausible; the file name is what the user sees.
#[cfg(target_os = "android")]
fn mime_for(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt") => "text/plain",
        Some("md") => "text/markdown",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("apk") => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

/// Shortest interval a rate sample is trusted over. Progress arrives per chunk,
/// which on a fast LAN can be a few milliseconds apart.
const MIN_SAMPLE_SECONDS: f64 = 0.25;

/// Exponentially smoothed transfer rate in bytes per second.
#[derive(Debug)]
struct SpeedMeter {
    last: Option<(u64, Instant)>,
    rate: f64,
    alpha: f64,
}

impl SpeedMeter {
    fn new() -> Self {
        Self {
            last: None,
            rate: 0.0,
            alpha: 0.3,
        }
    }

    fn sample(&mut self, bytes: u64, now: Instant) -> u64 {
        if let Some((previous, at)) = self.last {
            let seconds = now.duration_since(at).as_secs_f64();
            // Progress arrives once per 4 MiB chunk, so two samples can land a
            // couple of milliseconds apart and turn that chunk into a number in
            // the gigabytes. Wait for a real interval before believing it.
            if seconds < MIN_SAMPLE_SECONDS {
                return self.rate.max(0.0) as u64;
            }
            if seconds > 0.0 {
                let instant = bytes.saturating_sub(previous) as f64 / seconds;
                self.rate = if self.rate <= 0.0 {
                    instant
                } else {
                    self.alpha * instant + (1.0 - self.alpha) * self.rate
                };
            }
        }
        self.last = Some((bytes, now));
        if self.rate <= 0.0 {
            0
        } else {
            self.rate as u64
        }
    }
}

#[derive(Clone)]
struct RegisteredFile {
    /// Opaque cache identity: the path on desktop, the document URI on Android.
    source_key: String,
    /// Registered with the file manager at pick time, so a send never re-opens
    /// a path the user did not choose again.
    handle: FileHandleId,
    name: String,
    size: u64,
}

#[derive(Clone)]
struct SourcePlan {
    handle: FileHandleId,
    source_key: String,
}

/// Batch membership of one sub-transfer.
#[derive(Clone, Debug)]
struct BatchTag {
    batch_id: Uuid,
    root_name: String,
    relative_path: String,
}

#[derive(Clone)]
struct OfferPlan {
    manifest: Manifest,
    file_name: String,
    /// Directory a received file lands in. A batch file belongs under
    /// `<received>/<root_name>/<parent of relative_path>`; None means the
    /// manager's own root, which is the single-file case.
    sink_dir: Option<PathBuf>,
}

/// A folder the user picked, kept only in this process.
struct FolderSelection {
    plan: BatchPlan,
    path: PathBuf,
}

/// Where one batch writes on this device.
struct BatchRoot {
    root: PathBuf,
}

/// The batch fields of one incoming offer.
struct BatchOffer {
    batch_id: Option<Uuid>,
    root_name: Option<String>,
    relative_path: Option<String>,
    entries: Vec<BatchEntryWire>,
}

/// Why a batch offer was refused before any file was created.
#[derive(Debug)]
struct BatchRejection {
    code: &'static str,
    message: String,
}

impl BatchRejection {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Shared state for one outgoing batch: the first offer carries the entry list
/// and later ones only their own relative path.
struct BatchSend {
    plan: BatchPlan,
    entries_taken: std::sync::atomic::AtomicBool,
}

#[derive(Clone)]
struct TransferFailure {
    code: String,
    code_number: u16,
    message: String,
    retryable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PeerDecision {
    Accept,
    Reject,
}

/// One transfer the UI can render and control.
pub struct TransferRecord {
    transfer_id: Uuid,
    device_id: Uuid,
    device_name: String,
    direction: TransferDirection,
    file_name: String,
    bytes_total: u64,
    total_files: u32,
    bytes_done: AtomicU64,
    speed: AtomicU64,
    /// Monotonic creation order, used as the queue tie-breaker.
    queue_seq: u64,
    state: Mutex<TransferState>,
    failure: Mutex<Option<TransferFailure>>,
    control: Mutex<TransferControl>,
    created_at_ms: i64,
    updated_at_ms: AtomicI64,
    throttle: Mutex<ProgressThrottle>,
    wire_throttle: Mutex<ProgressThrottle>,
    speed_meter: Mutex<SpeedMeter>,
    pending_accept: Mutex<Option<tokio::sync::oneshot::Sender<PeerDecision>>>,
    /// Set when the receiver answered with a duplicate frame instead of taking
    /// the bytes. Nothing landed on disk, so nothing may be exported or opened.
    deduplicated: std::sync::atomic::AtomicBool,
    source: Mutex<Option<SourcePlan>>,
    offer: Mutex<Option<OfferPlan>>,
    session: Mutex<Option<TransferSession>>,
    batch: Option<BatchTag>,
}

impl TransferRecord {
    fn new(
        transfer_id: Uuid,
        device_id: Uuid,
        device_name: String,
        direction: TransferDirection,
        file_name: String,
        bytes_total: u64,
        total_files: u32,
    ) -> Self {
        Self {
            transfer_id,
            device_id,
            device_name,
            direction,
            file_name,
            bytes_total,
            total_files,
            bytes_done: AtomicU64::new(0),
            speed: AtomicU64::new(0),
            queue_seq: next_queue_seq(),
            state: Mutex::new(TransferState::Preparing),
            failure: Mutex::new(None),
            control: Mutex::new(TransferControl::new()),
            created_at_ms: now_ms(),
            updated_at_ms: AtomicI64::new(now_ms()),
            throttle: Mutex::new(ProgressThrottle::new(PROGRESS_THROTTLE)),
            wire_throttle: Mutex::new(ProgressThrottle::new(PROGRESS_THROTTLE)),
            speed_meter: Mutex::new(SpeedMeter::new()),
            pending_accept: Mutex::new(None),
            deduplicated: std::sync::atomic::AtomicBool::new(false),
            source: Mutex::new(None),
            offer: Mutex::new(None),
            session: Mutex::new(None),
            batch: None,
        }
    }

    /// A record rebuilt from a checkpoint left by a previous run. Nothing drives
    /// it, so it starts Interrupted; the user retries or drops it.
    fn interrupted(
        transfer_id: Uuid,
        device_id: Uuid,
        device_name: String,
        direction: TransferDirection,
        file_name: String,
        bytes_total: u64,
        bytes_done: u64,
    ) -> Self {
        let record = Self::new(
            transfer_id,
            device_id,
            device_name,
            direction,
            file_name,
            bytes_total,
            1,
        );
        record.bytes_done.store(bytes_done, Ordering::SeqCst);
        record.set_state(TransferState::Interrupted);
        record
    }

    /// Attach this record to one file of a folder batch.
    fn with_batch(mut self, tag: BatchTag) -> Self {
        self.batch = Some(tag);
        self
    }

    fn state(&self) -> TransferState {
        *lock(&self.state)
    }

    fn set_state(&self, state: TransferState) {
        *lock(&self.state) = state;
        self.updated_at_ms.store(now_ms(), Ordering::SeqCst);
    }

    fn control(&self) -> TransferControl {
        lock(&self.control).clone()
    }

    fn summary(&self) -> TransferSummary {
        let failure = lock(&self.failure).clone();
        TransferSummary {
            transfer_id: self.transfer_id.to_string(),
            device_id: self.device_id.to_string(),
            device_name: self.device_name.clone(),
            direction: self.direction,
            state: self.state(),
            file_name: self.file_name.clone(),
            bytes_total: self.bytes_total,
            bytes_done: self.bytes_done.load(Ordering::SeqCst),
            bytes_per_second: self.speed.load(Ordering::SeqCst),
            total_files: self.total_files,
            error_code: failure.as_ref().map(|failure| failure.code.clone()),
            error_code_number: failure.as_ref().map(|failure| failure.code_number),
            error_message: failure.as_ref().map(|failure| failure.message.clone()),
            retryable: failure.as_ref().is_some_and(|failure| failure.retryable),
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms.load(Ordering::SeqCst),
            batch_id: self.batch.as_ref().map(|tag| tag.batch_id.to_string()),
            batch_root: self.batch.as_ref().map(|tag| tag.root_name.clone()),
            relative_path: self.batch.as_ref().map(|tag| tag.relative_path.clone()),
        }
    }
}

/// Process-wide transfer registry. It owns registered files, live records, the
/// manifest cache, and the checkpoint store.
pub struct TransferRuntime {
    engine: Arc<Mutex<Engine>>,
    hub: Arc<EventHub>,
    chat: Mutex<Option<Arc<ChatRuntime>>>,
    files: Mutex<HashMap<Uuid, RegisteredFile>>,
    records: Mutex<HashMap<Uuid, Arc<TransferRecord>>>,
    drag_active: std::sync::atomic::AtomicBool,
    notifier: Mutex<Option<Arc<Notifier>>>,
    /// File access is swappable so Android can install its SAF-backed manager
    /// after the plugin has handed it a descriptor.
    manager: std::sync::RwLock<Arc<dyn FileAccessManager>>,
    manifests: ManifestCache,
    checkpoints: CheckpointStore,
    config: TransferConfig,
    /// Root received files land in. It can change at runtime; transfers already
    /// writing keep the sink they opened.
    receive_root: std::sync::RwLock<PathBuf>,
    /// Set once the Tauri app exists; the Android export needs it to reach the
    /// mobile plugin. Desktop builds never look at it.
    app: Mutex<Option<tauri::AppHandle>>,
    /// Transfers currently moving bytes. The Android foreground service is held
    /// while this is non-zero and released when the last one ends.
    active_transfers: std::sync::atomic::AtomicUsize,
    folders: Mutex<HashMap<Uuid, FolderSelection>>,
    batches: Mutex<HashMap<Uuid, BatchRoot>>,
    batch_sends: Mutex<HashMap<Uuid, Arc<BatchSend>>>,
    /// Root hashes of files this process has finished receiving. A sender that
    /// offers identical content again is answered with a duplicate frame
    /// instead of the whole file. Lifetime-bound on purpose: a file the user
    /// deleted outside the app would otherwise keep claiming to be present.
    received_hashes: Mutex<std::collections::HashSet<[u8; 32]>>,
}

impl TransferRuntime {
    /// Whether an offer from this peer is accepted without asking.
    ///
    /// Two levels: the per-device policy wins when it has been set, and
    /// `Inherit` - the default for a freshly paired device - falls back to the
    /// global switch. Someone who never opens the device page keeps exactly the
    /// behaviour the settings page describes.
    fn auto_accept(&self, device_id: Uuid) -> bool {
        let engine = lock(&self.engine);
        match engine.auto_accept_policy(&device_id) {
            Some(AutoAcceptPolicy::Auto) => true,
            Some(AutoAcceptPolicy::Ask) => false,
            _ => engine.config().auto_accept_transfers,
        }
    }

    pub fn new(engine: Arc<Mutex<Engine>>, hub: Arc<EventHub>) -> Arc<Self> {
        let (received, checkpoints, manifests) = {
            let guard = lock(&engine);
            let root = guard.paths().data_dir.clone();
            (
                guard.receive_dir(),
                root.join("checkpoints"),
                root.join("manifests"),
            )
        };
        for dir in [&received, &checkpoints, &manifests] {
            let _ = std::fs::create_dir_all(dir);
        }
        Arc::new(Self {
            engine,
            hub,
            chat: Mutex::new(None),
            files: Mutex::new(HashMap::new()),
            records: Mutex::new(HashMap::new()),
            drag_active: std::sync::atomic::AtomicBool::new(false),
            notifier: Mutex::new(None),
            manager: std::sync::RwLock::new(Arc::new(DesktopFileAccessManager::new(
                received.clone(),
            ))),
            manifests: ManifestCache::new(manifests),
            checkpoints: CheckpointStore::new(checkpoints),
            config: TransferConfig::default(),
            receive_root: std::sync::RwLock::new(received),
            app: Mutex::new(None),
            active_transfers: std::sync::atomic::AtomicUsize::new(0),
            folders: Mutex::new(HashMap::new()),
            batches: Mutex::new(HashMap::new()),
            batch_sends: Mutex::new(HashMap::new()),
            received_hashes: Mutex::new(std::collections::HashSet::new()),
        })
    }

    /// Replace the file access manager. Android injects one backed by SAF
    /// descriptors; the desktop default is the std::fs implementation.
    pub fn set_manager(&self, manager: Arc<dyn FileAccessManager>) {
        match self.manager.write() {
            Ok(mut current) => *current = manager,
            Err(error) => tracing::warn!(%error, "the file manager lock is poisoned"),
        }
    }

    /// The manager in use right now.
    pub fn manager(&self) -> Arc<dyn FileAccessManager> {
        match self.manager.read() {
            Ok(current) => Arc::clone(&current),
            Err(_) => Arc::new(DesktopFileAccessManager::new(self.receive_root())),
        }
    }

    /// The directory new transfers write into.
    pub fn receive_root(&self) -> PathBuf {
        self.receive_root
            .read()
            .map(|root| root.clone())
            .unwrap_or_else(|_| PathBuf::from("received"))
    }

    /// Move the receive directory for the next transfer. Transfers already
    /// writing keep their open sink, and nothing is moved on disk.
    /// Bracket one active transfer. The first one raises the Android foreground
    /// service, the last one lowers it; the counter makes overlapping transfers
    /// share a single notification.
    fn enter_transfer(self: &Arc<Self>) {
        let count = self.active_transfers.fetch_add(1, Ordering::SeqCst) + 1;
        #[cfg(target_os = "android")]
        {
            if count != 1 {
                return;
            }
            let Some(app) = self.app.lock().ok().and_then(|slot| slot.clone()) else {
                return;
            };
            async_runtime::spawn(async move {
                use tauri::Manager;
                let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
                if let Err(error) = handle.foreground_start().await {
                    tracing::warn!(%error, "could not raise the transfer foreground service");
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        let _ = count;
    }

    fn leave_transfer(self: &Arc<Self>) {
        let count = self
            .active_transfers
            .fetch_sub(1, Ordering::SeqCst)
            .saturating_sub(1);
        #[cfg(target_os = "android")]
        {
            if count != 0 {
                return;
            }
            let Some(app) = self.app.lock().ok().and_then(|slot| slot.clone()) else {
                return;
            };
            async_runtime::spawn(async move {
                use tauri::Manager;
                let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
                if let Err(error) = handle.foreground_stop().await {
                    tracing::warn!(%error, "could not lower the transfer foreground service");
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        let _ = count;
    }

    /// Give the runtime a handle to the app, so a finished receive can be
    /// exported through the platform bridge on Android.
    pub fn set_app_handle(&self, app: tauri::AppHandle) {
        if let Ok(mut slot) = self.app.lock() {
            *slot = Some(app);
        }
    }

    pub fn set_receive_root(&self, root: PathBuf) {
        if let Err(error) = std::fs::create_dir_all(&root) {
            tracing::warn!(path = %root.display(), %error, "unable to create the receive directory");
            return;
        }
        match self.receive_root.write() {
            Ok(mut current) => *current = root,
            Err(error) => tracing::warn!(%error, "the receive directory lock is poisoned"),
        }
    }

    /// Connect the chat runtime after both have been constructed.
    pub fn attach_chat(&self, chat: Arc<ChatRuntime>) {
        *lock(&self.chat) = Some(chat);
    }

    /// Rebuild records for checkpoints a previous run left behind. Runs after
    /// attach_chat because a peer name comes from the address book; without a
    /// known peer the record still appears, named by its id, so nothing is lost.
    pub fn restore_interrupted(self: &Arc<Self>) {
        let ids = match self.checkpoints.list() {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not list transfer checkpoints");
                return;
            }
        };
        let mut restored = 0usize;
        for id in ids {
            let Ok(Some((manifest, checkpoint))) = self.checkpoints.load(id) else {
                continue;
            };
            if matches!(
                checkpoint.state,
                fluxy_core::TransferState::Completed | fluxy_core::TransferState::Cancelled
            ) {
                continue;
            }
            let Some(device_id) = checkpoint.device_id else {
                tracing::debug!(%id, "checkpoint has no device; skipping restore");
                continue;
            };
            tracing::info!(
                id = %id,
                file = %checkpoint.file_name,
                device = ?checkpoint.device_id,
                state = ?checkpoint.state,
                "restoring an interrupted transfer"
            );
            let direction = match checkpoint.role {
                fluxy_core::transfer::TransferRole::Sender => TransferDirection::Send,
                fluxy_core::transfer::TransferRole::Receiver => TransferDirection::Receive,
            };
            let missing: u64 = checkpoint
                .received
                .missing_indices()
                .iter()
                .filter_map(|index| manifest.block_len(*index))
                .map(|length| length as u64)
                .sum();
            let done = manifest.file_size.saturating_sub(missing);
            let device_name = self
                .chat
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .and_then(|chat| chat.known_peer(device_id))
                .map(|peer| peer.name)
                // The fallback has to be the *peer* id, not the transfer id: the
                // row is titled by device, and a transfer-id prefix reads as a
                // device the user has never seen.
                .unwrap_or_else(|| device_id.to_string()[..8].to_string());
            let record = TransferRecord::interrupted(
                id,
                device_id,
                device_name,
                direction,
                checkpoint.file_name.clone(),
                manifest.file_size,
                done,
            );
            if let Ok(mut records) = self.records.lock() {
                records.insert(id, Arc::new(record));
                restored += 1;
            }
        }
        if restored > 0 {
            // The UI pulls a snapshot on startup, so no event is needed here;
            // the restored records are simply present when it asks.
            tracing::info!(restored, "restored interrupted transfers from checkpoints");
        }
    }

    /// Connect the desktop notifier after both have been constructed. Before
    /// this runs (unit tests and early startup) every notification is a no-op.
    pub fn attach_notifier(&self, notifier: Arc<Notifier>) {
        *lock(&self.notifier) = Some(notifier);
    }

    /// Tell the user a transfer finished or failed while the window was not in
    /// front. The notification carries the file name and the peer name only.
    fn notify_outcome(&self, record: &TransferRecord, completed: bool) {
        let notifier = lock(&self.notifier).clone();
        let Some(notifier) = notifier else {
            return;
        };
        let device_id = record.device_id.to_string();
        if completed {
            notifier.notify_transfer_completed(&device_id, &record.device_name, &record.file_name);
        } else {
            notifier.notify_transfer_failed(&device_id, &record.device_name, &record.file_name);
        }
    }

    fn chat(&self) -> Result<Arc<ChatRuntime>, IpcError> {
        lock(&self.chat)
            .clone()
            .ok_or_else(|| IpcError::internal("transfer runtime has no chat session"))
    }

    fn identity(&self) -> Result<Identity, IpcError> {
        Ok(lock(&self.engine).identity().clone())
    }

    pub fn snapshot(&self) -> Vec<TransferSummary> {
        let records: Vec<Arc<TransferRecord>> = lock(&self.records).values().cloned().collect();
        let mut summaries: Vec<TransferSummary> =
            records.iter().map(|record| record.summary()).collect();
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.created_at_ms));
        summaries
    }

    fn record(&self, transfer_id: Uuid) -> Result<Arc<TransferRecord>, IpcError> {
        lock(&self.records)
            .get(&transfer_id)
            .cloned()
            .ok_or_else(|| IpcError {
                code: "protocol".into(),
                code_number: 1001,
                message: "unknown transfer id".into(),
                retryable: false,
            })
    }

    fn insert(&self, record: Arc<TransferRecord>) {
        lock(&self.records).insert(record.transfer_id, record);
    }

    fn get(&self, transfer_id: Uuid) -> Option<Arc<TransferRecord>> {
        lock(&self.records).get(&transfer_id).cloned()
    }

    fn emit_transfer(&self, event: TransferEvent) {
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Transfer { data: event });
    }

    /// Publish an event that is not part of the snapshot (drag state, a drop).
    fn emit_transient(&self, event: TransferEvent) {
        self.hub.emit(AppEvent::Transfer { data: event });
    }

    /// Register a file the platform handed over as an open descriptor. Android
    /// has no path for a picked document, so the descriptor is the source.
    pub fn register_descriptor_source(
        &self,
        uri: String,
        file: std::fs::File,
        name: String,
        mime: Option<String>,
    ) -> Result<RegisteredFileView, IpcError> {
        let (handle, meta) = self
            .manager()
            .register_descriptor(&uri, file, &name, mime)
            .map_err(|error| IpcError {
                code: "file_access".into(),
                code_number: 3003,
                message: error.to_string(),
                retryable: false,
            })?;
        let file_id = Uuid::now_v7();
        lock(&self.files).insert(
            file_id,
            RegisteredFile {
                handle,
                source_key: uri,
                name: meta.name.clone(),
                size: meta.size,
            },
        );
        Ok(RegisteredFileView {
            file_id: file_id.to_string(),
            name: meta.name,
            size: meta.size,
        })
    }

    /// Register a local file the user picked. Only the opaque handle crosses
    /// the IPC boundary; the path stays in this registry.
    pub fn register_file(&self, path: &str) -> Result<RegisteredFileView, IpcError> {
        let path = PathBuf::from(path);
        let metadata = std::fs::metadata(&path).map_err(|error| IpcError {
            code: "file_access".into(),
            code_number: 3003,
            message: format!("cannot read {}: {error}", path.display()),
            retryable: false,
        })?;
        if !metadata.is_file() {
            return Err(IpcError {
                code: "file_access".into(),
                code_number: 3003,
                message: "only regular files can be sent".into(),
                retryable: false,
            });
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| IpcError {
                code: "file_access".into(),
                code_number: 3003,
                message: "the file has no usable name".into(),
                retryable: false,
            })?
            .to_string();
        let handle = self
            .manager()
            .register_source(FileReference::Path(path.clone()))
            .map_err(|error| IpcError {
                code: "file_access".into(),
                code_number: 3003,
                message: format!("cannot register {}: {error}", path.display()),
                retryable: false,
            })?;
        let file_id = Uuid::now_v7();
        lock(&self.files).insert(
            file_id,
            RegisteredFile {
                source_key: path.to_string_lossy().to_string(),
                handle,
                name: name.clone(),
                size: metadata.len(),
            },
        );
        Ok(RegisteredFileView {
            file_id: file_id.to_string(),
            name,
            size: metadata.len(),
        })
    }

    /// Queue one registered file per transfer for a device. Every file becomes
    /// an independent transfer, so one failure never blocks the batch. Sends
    /// start only while a global active slot is free; the rest wait in `queued`
    /// and are promoted by [`Self::pump_queue`].
    pub async fn start_transfers(
        self: &Arc<Self>,
        device_id: &str,
        file_ids: &[String],
    ) -> Result<Vec<TransferSummary>, IpcError> {
        if file_ids.is_empty() {
            return Err(IpcError {
                code: "transfer".into(),
                code_number: 3001,
                message: "select at least one file".into(),
                retryable: false,
            });
        }
        if file_ids.len() > MAX_BATCH_FILES {
            return Err(IpcError {
                code: "transfer".into(),
                code_number: 3001,
                message: format!("at most {MAX_BATCH_FILES} files can be sent at once"),
                retryable: false,
            });
        }
        tracing::info!(device = %device_id, files = file_ids.len(), "start_transfers entered");
        let device = parse_uuid(device_id, "device")?;
        let chat = self.chat()?;
        // A share into a freshly started app arrives before discovery has seen
        // anything, and rejecting there loses the send the user just triggered.
        // Give discovery a few seconds to produce the peer instead.
        let peer = match chat.known_peer(device) {
            Some(peer) => peer,
            None => {
                let deadline = tokio::time::Instant::now() + PEER_WAIT;
                tracing::info!(
                    device = %device,
                    wait_ms = PEER_WAIT.as_millis() as u64,
                    "start_transfers: peer not known yet, waiting for discovery"
                );
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    if let Some(peer) = chat.known_peer(device) {
                        break peer;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        tracing::warn!(device = %device, "start_transfers: discovery never produced the peer");
                        return Err(peer_offline());
                    }
                }
            }
        };
        tracing::info!(peer = %peer.name, "start_transfers: peer known, opening");
        chat.open(peer.clone())?;
        tracing::info!("start_transfers: session requested");

        let mut ids = Vec::with_capacity(file_ids.len());
        for file_id in file_ids {
            let key = parse_uuid(file_id, "file")?;
            tracing::info!(%file_id, "start_transfers: resolving file");
            let file = lock(&self.files)
                .get(&key)
                .cloned()
                .ok_or_else(|| IpcError {
                    code: "file_access".into(),
                    code_number: 3003,
                    message: "the file handle is no longer registered".into(),
                    retryable: false,
                })?;
            let size = file.size;
            // Reuse the id of a cancelled or failed send of the same file to the
            // same peer. A fresh id would leave the old row in the list (two
            // entries for one file) and orphan whatever checkpoint exists, so
            // the retry would silently start from zero.
            tracing::info!(
                file = %file.name,
                size,
                device = %device,
                "start_transfers: looking for an interrupted send to reuse"
            );
            if let Ok(records) = self.records.lock() {
                for record in records.values() {
                    if record.direction == TransferDirection::Send {
                        tracing::info!(
                            candidate = %record.transfer_id,
                            file = %record.file_name,
                            bytes = record.bytes_total,
                            device = %record.device_id,
                            state = ?record.state(),
                            "start_transfers: existing send"
                        );
                    }
                }
            }
            let transfer_id = self
                .records
                .lock()
                .ok()
                .and_then(|records| {
                    records
                        .values()
                        .find(|record| {
                            record.direction == TransferDirection::Send
                                && record.device_id == device
                                && record.file_name == file.name
                                && record.bytes_total == size
                                // Interrupted covers the case the user named: the
                                // process died, restore_interrupted rebuilt the row
                                // from the on-disk checkpoint, and sending the same
                                // file again must land on that same id - otherwise
                                // the checkpoint stays orphaned across restarts.
                                && matches!(
                                    record.state(),
                                    TransferState::Cancelled
                                        | TransferState::Failed
                                        | TransferState::Interrupted
                                )
                        })
                        .map(|record| record.transfer_id)
                })
                .unwrap_or_else(Uuid::now_v7);
            let record = Arc::new(TransferRecord::new(
                transfer_id,
                device,
                peer.name.clone(),
                TransferDirection::Send,
                file.name.clone(),
                size,
                1,
            ));
            record.set_state(TransferState::Queued);
            *lock(&record.source) = Some(SourcePlan {
                handle: file.handle,
                source_key: file.source_key.clone(),
            });
            self.insert(record.clone());
            self.emit_transfer(TransferEvent::Queued {
                transfer_id: transfer_id.to_string(),
                device_id: device.to_string(),
                device_name: record.device_name.clone(),
                file_name: record.file_name.clone(),
                total_files: 1,
                total_bytes: size,
            });
            ids.push(transfer_id);
        }
        tracing::info!(count = ids.len(), "start_transfers: queued, pumping");
        self.pump_queue();
        tracing::info!("start_transfers: pumped");
        ids.iter()
            .map(|id| self.record(*id).map(|record| record.summary()))
            .collect()
    }

    /// Offer one registered file to a device.
    pub async fn start_transfer(
        self: &Arc<Self>,
        device_id: &str,
        file_id: &str,
    ) -> Result<TransferSummary, IpcError> {
        let mut summaries = self
            .start_transfers(device_id, &[file_id.to_string()])
            .await?;
        summaries
            .pop()
            .ok_or_else(|| IpcError::internal("the transfer was not created"))
    }

    /// The configured global cap on simultaneous active transfers.
    fn max_active(&self) -> usize {
        lock(&self.engine).config().max_active_transfers.max(1) as usize
    }

    /// Transfers that currently own a slot.
    fn active_count(&self) -> usize {
        lock(&self.records)
            .values()
            .filter(|record| is_active_state(record.state()))
            .count()
    }

    /// Incoming offers that were admitted and are still waiting for the user.
    fn offered_count(&self) -> usize {
        lock(&self.records)
            .values()
            .filter(|record| record.state() == TransferState::Offered)
            .count()
    }

    /// Slots in use, including offers that were admitted but not answered yet.
    fn occupied(&self) -> usize {
        self.active_count() + self.offered_count()
    }

    /// Promote queued sends while a slot is free. Called after every enqueue
    /// and after every terminal transition, so a freed slot is reused at once.
    pub fn pump_queue(self: &Arc<Self>) {
        let limit = self.max_active();
        while self.active_count() < limit {
            let Some(record) = self.claim_next_queued() else {
                break;
            };
            self.emit_transfer(TransferEvent::Prepared {
                transfer_id: record.transfer_id.to_string(),
                device_id: record.device_id.to_string(),
                device_name: record.device_name.clone(),
                file_name: record.file_name.clone(),
                total_files: record.total_files,
                total_bytes: record.bytes_total,
            });
            let runtime = self.clone();
            async_runtime::spawn(async move { runtime.run_sender(record).await });
        }
    }

    /// Claim the oldest queued send by marking it as starting. Holding the
    /// registry lock makes the claim atomic, so two pumps never start the same
    /// record; ties fall back to the time-ordered transfer id (FIFO).
    fn claim_next_queued(&self) -> Option<Arc<TransferRecord>> {
        let records = lock(&self.records);
        let mut next: Option<&Arc<TransferRecord>> = None;
        for record in records.values() {
            if record.direction != TransferDirection::Send
                || record.state() != TransferState::Queued
            {
                continue;
            }
            if next.is_none_or(|current| {
                (record.created_at_ms, record.queue_seq)
                    < (current.created_at_ms, current.queue_seq)
            }) {
                next = Some(record);
            }
        }
        let record = next.cloned();
        if let Some(record) = &record {
            record.set_state(TransferState::Preparing);
        }
        record
    }

    /// Register a dropped batch. A folder becomes a batch (its tree is walked
    /// and validated here, before anything is offered); a file becomes a single
    /// transfer. Only base names and opaque ids are reported back, so a local
    /// path never crosses the IPC boundary.
    pub fn register_paths(
        &self,
        paths: &[PathBuf],
    ) -> (
        Vec<RegisteredFileView>,
        Vec<RegisteredFolderView>,
        Vec<String>,
        u32,
    ) {
        let mut files = Vec::new();
        let mut folders = Vec::new();
        let mut rejected = Vec::new();
        let mut skipped = 0_u32;
        // The file budget counts every file path that was considered, accepted
        // or not, so one bad path cannot push the rest past the limit.
        let mut file_slots = 0_usize;
        for path in paths {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| "?".into());
            let is_dir = std::fs::metadata(path)
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false);
            if is_dir {
                // Folders are heavier than files, so they have their own budget.
                if folders.len() >= MAX_DROPPED_FOLDERS {
                    skipped += 1;
                    continue;
                }
                match self.register_folder(&path.to_string_lossy()) {
                    Ok(view) => folders.push(view),
                    Err(_) => rejected.push(name),
                }
                continue;
            }
            if file_slots >= MAX_BATCH_FILES {
                skipped += 1;
                continue;
            }
            file_slots += 1;
            match self.register_file(&path.to_string_lossy()) {
                Ok(view) => files.push(view),
                Err(_) => rejected.push(name),
            }
        }
        (files, folders, rejected, skipped)
    }

    /// Handle a window drop: register every file and folder, then publish the
    /// batch so the send confirmation opens without another click.
    pub fn accept_drop(self: &Arc<Self>, paths: Vec<PathBuf>) {
        self.set_drag_hover(false);
        let (files, folders, rejected, skipped) = self.register_paths(&paths);
        if files.is_empty() && folders.is_empty() && rejected.is_empty() {
            return;
        }
        self.emit_transient(TransferEvent::FilesDropped {
            files,
            folders,
            rejected,
            skipped,
        });
    }

    /// Publish a drag-enter/leave transition once, without touching the
    /// snapshot version: hovering must never spam the WebView.
    pub fn set_drag_hover(&self, active: bool) {
        if self.drag_active.swap(active, Ordering::SeqCst) == active {
            return;
        }
        self.emit_transient(TransferEvent::DragHover { active });
    }

    /// Accept or reject an incoming offer.
    pub fn respond(
        self: &Arc<Self>,
        transfer_id: &str,
        accepted: bool,
    ) -> Result<TransferSummary, IpcError> {
        let id = parse_uuid(transfer_id, "transfer")?;
        let record = self.record(id)?;
        if record.direction != TransferDirection::Receive {
            return Err(IpcError::internal(
                "only an incoming transfer can be accepted",
            ));
        }
        let session = lock(&record.session).clone();
        if accepted {
            self.accept(record.clone());
        } else {
            record.set_state(TransferState::Cancelled);
            if let Some(session) = session {
                let message = TransferMessage::Reject {
                    transfer_id: id,
                    code: "rejected".into(),
                    message: "the receiver declined the transfer".into(),
                };
                async_runtime::spawn(async move {
                    let _ = session.sender.send_transfer(message).await;
                });
            }
            self.emit_transfer(TransferEvent::Cancelled {
                transfer_id: id.to_string(),
            });
            self.pump_queue();
        }
        Ok(self.record(id)?.summary())
    }

    fn accept(self: &Arc<Self>, record: Arc<TransferRecord>) {
        record.set_state(TransferState::Preparing);
        let Some(session) = lock(&record.session).clone() else {
            self.fail(
                &record,
                TransferError::Control("the transfer has no control session".into()),
            );
            return;
        };
        let runtime = self.clone();
        let id = record.transfer_id;
        async_runtime::spawn(async move {
            let _ = session
                .sender
                .send_transfer(TransferMessage::Accept { transfer_id: id })
                .await;
            runtime.run_receiver(record).await;
        });
    }

    /// Pause, resume, cancel, or retry one transfer.
    pub fn control(
        self: &Arc<Self>,
        transfer_id: &str,
        action: TransferAction,
    ) -> Result<TransferSummary, IpcError> {
        let id = parse_uuid(transfer_id, "transfer")?;
        let record = self.record(id)?;
        let session = lock(&record.session).clone();
        match action {
            TransferAction::Pause => {
                if record.control().pause() {
                    record.set_state(TransferState::Paused);
                    send_control(session, TransferMessage::Pause { transfer_id: id });
                    self.emit_transfer(TransferEvent::Paused {
                        transfer_id: id.to_string(),
                    });
                }
            }
            TransferAction::Resume => {
                if record.control().resume() {
                    record.set_state(TransferState::Transferring);
                    send_control(session, TransferMessage::Resume { transfer_id: id });
                    self.emit_transfer(TransferEvent::Resumed {
                        transfer_id: id.to_string(),
                    });
                }
            }
            TransferAction::Cancel => {
                if record.control().cancel() {
                    record.set_state(TransferState::Cancelled);
                    send_control(session, TransferMessage::Cancel { transfer_id: id });
                    self.emit_transfer(TransferEvent::Cancelled {
                        transfer_id: id.to_string(),
                    });
                    self.pump_queue();
                }
            }
            TransferAction::Retry => {
                let state = record.state();
                if !matches!(
                    state,
                    TransferState::Failed | TransferState::Cancelled | TransferState::Preparing
                ) {
                    return Err(IpcError {
                        code: "transfer".into(),
                        code_number: 3001,
                        message: "only a failed, cancelled, or pending transfer can be retried"
                            .into(),
                        retryable: false,
                    });
                }
                *lock(&record.control) = TransferControl::new();
                *lock(&record.failure) = None;
                match record.direction {
                    TransferDirection::Send => {
                        // A retry is a new attempt, so it goes back through the
                        // queue instead of jumping the active-transfer cap.
                        record.set_state(TransferState::Queued);
                        self.emit_transfer(TransferEvent::Queued {
                            transfer_id: id.to_string(),
                            device_id: record.device_id.to_string(),
                            device_name: record.device_name.clone(),
                            file_name: record.file_name.clone(),
                            total_files: record.total_files,
                            total_bytes: record.bytes_total,
                        });
                        self.pump_queue();
                    }
                    TransferDirection::Receive => {
                        record.set_state(TransferState::Preparing);
                        self.emit_transfer(TransferEvent::Prepared {
                            transfer_id: id.to_string(),
                            device_id: record.device_id.to_string(),
                            device_name: record.device_name.clone(),
                            file_name: record.file_name.clone(),
                            total_files: record.total_files,
                            total_bytes: record.bytes_total,
                        });
                        let runtime = self.clone();
                        async_runtime::spawn(async move { runtime.run_receiver(record).await });
                    }
                }
            }
        }
        Ok(self.record(id)?.summary())
    }

    /// Pause every transfer that is moving bytes right now.
    ///
    /// Called before the LAN listener is rebound: a network change would
    /// otherwise fail each active transfer on a dead socket. Paused records keep
    /// their checkpoints, and the existing resume path continues from there.
    pub fn pause_active(self: &Arc<Self>) {
        let ids = lock(&self.records)
            .values()
            .filter(|record| record.state() == TransferState::Transferring)
            .map(|record| record.transfer_id.to_string())
            .collect::<Vec<_>>();
        for id in ids {
            if let Err(error) = self.control(&id, TransferAction::Pause) {
                tracing::warn!(transfer_id = %id, message = %error.message, "unable to pause a transfer before rebinding");
            }
        }
    }

    /// Route one transfer control frame received on a chat session.
    pub fn handle_peer_event(
        self: &Arc<Self>,
        device_id: Uuid,
        device_name: &str,
        session: TransferSession,
        event: SessionEvent,
    ) {
        match event {
            SessionEvent::TransferOffer {
                transfer_id,
                file_name,
                file_size,
                block_size,
                root_hash,
                manifest_digest,
                total_files,
                total_bytes,
                manifest,
                batch_id,
                root_name,
                relative_path,
                entries,
            } => self.handle_offer(
                device_id,
                device_name,
                session,
                transfer_id,
                file_name,
                file_size,
                block_size,
                root_hash,
                manifest_digest,
                total_files,
                total_bytes,
                manifest,
                BatchOffer {
                    batch_id,
                    root_name,
                    relative_path,
                    entries,
                },
            ),
            SessionEvent::TransferAccept { transfer_id } => {
                self.resolve(transfer_id, PeerDecision::Accept);
            }
            SessionEvent::TransferReject {
                transfer_id,
                code,
                message,
            } => {
                if let Some(record) = self.get(transfer_id) {
                    self.fail_with(
                        &record,
                        TransferFailure {
                            code: code.clone(),
                            code_number: 3001,
                            message: message.clone(),
                            retryable: true,
                        },
                    );
                }
                self.resolve(transfer_id, PeerDecision::Reject);
            }
            SessionEvent::TransferProgress {
                transfer_id,
                bytes_done,
                bytes_total,
            } => self.on_progress(transfer_id, bytes_done, bytes_total),
            SessionEvent::TransferComplete { transfer_id } => {
                if let Some(record) = self.get(transfer_id)
                    && record.state() == TransferState::Transferring
                {
                    self.complete(&record);
                }
            }
            SessionEvent::TransferError {
                transfer_id,
                code,
                message,
                retryable,
            } => {
                if let Some(record) = self.get(transfer_id) {
                    self.fail_with(
                        &record,
                        TransferFailure {
                            code,
                            code_number: 3001,
                            message,
                            retryable,
                        },
                    );
                }
            }
            SessionEvent::TransferPause { transfer_id } => {
                if let Some(record) = self.get(transfer_id) {
                    record.control().pause();
                    record.set_state(TransferState::Paused);
                    self.emit_transfer(TransferEvent::Paused {
                        transfer_id: transfer_id.to_string(),
                    });
                }
            }
            SessionEvent::TransferCancel { transfer_id } => {
                if let Some(record) = self.get(transfer_id) {
                    record.control().cancel();
                    record.set_state(TransferState::Cancelled);
                    self.emit_transfer(TransferEvent::Cancelled {
                        transfer_id: transfer_id.to_string(),
                    });
                    self.pump_queue();
                }
            }
            SessionEvent::TransferResume { transfer_id } => {
                if let Some(record) = self.get(transfer_id) {
                    record.control().resume();
                    record.set_state(TransferState::Transferring);
                    self.emit_transfer(TransferEvent::Resumed {
                        transfer_id: transfer_id.to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    /// A batch offer names one file inside a folder. Placement validates the
    /// relative path, creates the batch root (once) and the empty directories it
    /// declared, and returns the sink directory plus the safe file name.
    fn place_batch_offer(
        &self,
        batch: &BatchOffer,
        file_size: u64,
    ) -> Result<Option<(BatchTag, PathBuf, String)>, BatchRejection> {
        let (batch_id, root_name, relative_path) =
            match (&batch.batch_id, &batch.root_name, &batch.relative_path) {
                (None, _, _) => {
                    if !batch.entries.is_empty() {
                        return Err(BatchRejection::new(
                            "invalid_batch",
                            "an entry list arrived without a batch id",
                        ));
                    }
                    return Ok(None);
                }
                (Some(id), Some(root), Some(path)) => (*id, root.clone(), path.clone()),
                _ => {
                    return Err(BatchRejection::new(
                        "invalid_batch",
                        "the offer is missing its batch root or relative path",
                    ));
                }
            };
        validate_relative_path(&relative_path)
            .map_err(|error| BatchRejection::new("file_access", error.to_string()))?;

        // The entry list, when present, has to describe this exact file.
        let mut declared = None;
        if !batch.entries.is_empty() {
            let entries = batch
                .entries
                .iter()
                .map(|entry| BatchEntry {
                    relative_path: entry.relative_path.clone(),
                    size: entry.size,
                    mtime_ms: entry.mtime_ms,
                    is_dir: entry.is_dir,
                })
                .collect::<Vec<_>>();
            let plan = BatchPlan::new(batch_id, root_name.clone(), entries)
                .map_err(|error| BatchRejection::new("invalid_batch", error.to_string()))?;
            match plan.entry(&relative_path) {
                Some(entry) if !entry.is_dir && entry.size == file_size => {}
                _ => {
                    return Err(BatchRejection::new(
                        "invalid_batch",
                        "the relative path is not part of the batch entry list",
                    ));
                }
            }
            declared = Some(plan);
        }

        let root = {
            let mut batches = lock(&self.batches);
            match batches.get(&batch_id) {
                Some(existing) => existing.root.clone(),
                None => {
                    let root = unique_batch_root(&self.receive_root(), &root_name);
                    if let Err(error) = std::fs::create_dir_all(&root) {
                        return Err(BatchRejection::new(
                            "file_access",
                            format!("cannot create the batch root: {error}"),
                        ));
                    }
                    if let Some(plan) = &declared {
                        for directory in plan.empty_directories() {
                            match BatchPlan::resolve(&root, directory) {
                                Ok(path) => {
                                    if let Err(error) = std::fs::create_dir_all(&path) {
                                        return Err(BatchRejection::new(
                                            "file_access",
                                            format!("cannot create {directory}: {error}"),
                                        ));
                                    }
                                }
                                Err(error) => {
                                    return Err(BatchRejection::new(
                                        "file_access",
                                        error.to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    batches.insert(batch_id, BatchRoot { root: root.clone() });
                    root
                }
            }
        };

        let (parent, name) = match relative_path.rsplit_once('/') {
            Some((parent, name)) => (parent, name),
            None => ("", relative_path.as_str()),
        };
        let sink_dir = if parent.is_empty() {
            root
        } else {
            BatchPlan::resolve(&root, parent)
                .map_err(|error| BatchRejection::new("file_access", error.to_string()))?
        };
        if let Err(error) = std::fs::create_dir_all(&sink_dir) {
            return Err(BatchRejection::new(
                "file_access",
                format!("cannot create the containing directory: {error}"),
            ));
        }
        let tag = BatchTag {
            batch_id,
            root_name: sanitize_root_name(&root_name),
            relative_path: relative_path.clone(),
        };
        Ok(Some((tag, sink_dir, name.to_string())))
    }

    /// Register a folder the user picked and describe it for the confirmation.
    pub fn register_folder(&self, path: &str) -> Result<RegisteredFolderView, IpcError> {
        let root = PathBuf::from(path);
        let batch_id = Uuid::now_v7();
        let plan = scan_folder(&root, batch_id).map_err(|error| IpcError {
            code: code_name(error.code()).into(),
            code_number: error.code().as_u16(),
            message: error.to_string(),
            retryable: false,
        })?;
        let view = RegisteredFolderView {
            folder_id: batch_id.to_string(),
            root_name: plan.root_name.clone(),
            total_files: plan.file_count() as u32,
            total_bytes: plan.total_bytes(),
            empty_directories: plan.directory_count() as u32,
            sample: plan
                .entries
                .iter()
                .take(8)
                .map(|entry| RegisteredFolderEntry {
                    relative_path: entry.relative_path.clone(),
                    size: entry.size,
                    is_dir: entry.is_dir,
                })
                .collect(),
        };
        lock(&self.folders).insert(batch_id, FolderSelection { plan, path: root });
        Ok(view)
    }

    /// Send a registered folder. Every file becomes an independent transfer that
    /// carries the batch id and its own relative path, so resume, pause, and
    /// cancel keep working per file.
    pub fn start_folder(
        self: &Arc<Self>,
        device_id: &str,
        folder_id: &str,
    ) -> Result<Vec<TransferSummary>, IpcError> {
        let batch_id = parse_uuid(folder_id, "folder")?;
        let (plan, root) = {
            let folders = lock(&self.folders);
            let selection = folders.get(&batch_id).ok_or_else(|| IpcError {
                code: "file_access".into(),
                code_number: 3003,
                message: "the folder handle is no longer registered".into(),
                retryable: false,
            })?;
            (selection.plan.clone(), selection.path.clone())
        };
        let files = plan.files().cloned().collect::<Vec<_>>();
        if files.is_empty() {
            return Err(IpcError {
                code: "transfer".into(),
                code_number: 3001,
                message: "the folder has no files to send; empty folders travel with the files around them"
                    .into(),
                retryable: false,
            });
        }
        let device = parse_uuid(device_id, "device")?;
        let chat = self.chat()?;
        let peer = chat.known_peer(device).ok_or_else(peer_offline)?;
        chat.open(peer.clone())?;

        lock(&self.batch_sends).insert(
            batch_id,
            Arc::new(BatchSend {
                plan: plan.clone(),
                entries_taken: std::sync::atomic::AtomicBool::new(false),
            }),
        );

        let mut ids = Vec::with_capacity(files.len());
        for entry in &files {
            let path =
                BatchPlan::resolve(&root, &entry.relative_path).map_err(|error| IpcError {
                    code: "file_access".into(),
                    code_number: 3003,
                    message: error.to_string(),
                    retryable: false,
                })?;
            let handle = self
                .manager()
                .register_source(FileReference::Path(path.clone()))
                .map_err(|error| IpcError {
                    code: "file_access".into(),
                    code_number: 3003,
                    message: error.to_string(),
                    retryable: false,
                })?;
            let transfer_id = Uuid::now_v7();
            let record = Arc::new(
                TransferRecord::new(
                    transfer_id,
                    device,
                    peer.name.clone(),
                    TransferDirection::Send,
                    entry.relative_path.clone(),
                    entry.size,
                    1,
                )
                .with_batch(BatchTag {
                    batch_id,
                    root_name: plan.root_name.clone(),
                    relative_path: entry.relative_path.clone(),
                }),
            );
            record.set_state(TransferState::Queued);
            *lock(&record.source) = Some(SourcePlan {
                handle,
                source_key: path.to_string_lossy().to_string(),
            });
            self.insert(record.clone());
            self.emit_transfer(TransferEvent::Queued {
                transfer_id: transfer_id.to_string(),
                device_id: device.to_string(),
                device_name: record.device_name.clone(),
                file_name: record.file_name.clone(),
                total_files: 1,
                total_bytes: entry.size,
            });
            ids.push(transfer_id);
        }
        self.pump_queue();
        ids.iter()
            .map(|id| self.record(*id).map(|record| record.summary()))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_offer(
        self: &Arc<Self>,
        device_id: Uuid,
        device_name: &str,
        session: TransferSession,
        transfer_id: Uuid,
        file_name: String,
        file_size: u64,
        block_size: u32,
        root_hash: [u8; 32],
        manifest_digest: [u8; 32],
        total_files: u32,
        total_bytes: u64,
        manifest_bytes: Vec<u8>,
        batch: BatchOffer,
    ) {
        tracing::info!(
            %device_id,
            device_name = %device_name,
            file = %file_name,
            "offer received; name as advertised by the sender"
        );
        // A batch sub-transfer is placed under its own root, validated against
        // the entry list, and refused outright when anything does not line up.
        // Refuse an offer that would not fit. Doing this before anything is
        // accepted keeps the sender's retry cheap and avoids a half-written
        // file that fails at the last block.
        if let Ok(available) = fs2::available_space(self.receive_root())
            && total_bytes > available
        {
            tracing::warn!(
                needed = total_bytes,
                available,
                "rejecting a transfer that does not fit"
            );
            let message = TransferMessage::Reject {
                transfer_id,
                // "disk_full" is the protocol's stable code for this case and has
                // copy in both locales, so the sender shows a real reason.
                code: code_name(ErrorCode::DiskFull).into(),
                message: format!(
                    "not enough free space: {total_bytes} bytes needed, {available} available"
                ),
            };
            async_runtime::spawn(async move {
                let _ = session.sender.send_transfer(message).await;
            });
            return;
        }
        let placement = match self.place_batch_offer(&batch, file_size) {
            Ok(placement) => placement,
            Err(rejection) => {
                let message = TransferMessage::Reject {
                    transfer_id,
                    code: rejection.code.into(),
                    message: rejection.message,
                };
                async_runtime::spawn(async move {
                    let _ = session.sender.send_transfer(message).await;
                });
                return;
            }
        };
        let (batch_tag, sink_dir, file_name) = match placement {
            Some((tag, dir, name)) => (Some(tag), Some(dir), name),
            None => (None, None, file_name),
        };
        let manifest = match Manifest::from_bytes(&manifest_bytes) {
            Ok(manifest)
                if manifest.file_size == file_size
                    && manifest.block_size == block_size
                    && manifest.root_hash == root_hash
                    && manifest.manifest_digest == manifest_digest =>
            {
                manifest
            }
            _ => {
                let message = TransferMessage::Reject {
                    transfer_id,
                    code: "invalid_manifest".into(),
                    message: "the offered manifest does not match its summary".into(),
                };
                async_runtime::spawn(async move {
                    let _ = session.sender.send_transfer(message).await;
                });
                return;
            }
        };
        if let Err(error) = fluxy_core::files::validate_file_name(&file_name) {
            let message = TransferMessage::Reject {
                transfer_id,
                code: "file_access".into(),
                message: error.to_string(),
            };
            async_runtime::spawn(async move {
                let _ = session.sender.send_transfer(message).await;
            });
            return;
        }

        let offer = OfferPlan {
            manifest,
            file_name: file_name.clone(),
            sink_dir,
        };
        if let Some(existing) = self.get(transfer_id) {
            *lock(&existing.session) = Some(session);
            *lock(&existing.offer) = Some(offer);
            if matches!(
                existing.state(),
                TransferState::Failed | TransferState::Preparing | TransferState::Interrupted
            ) {
                // A retried sender re-offers the same id; resume without asking
                // the user again. Interrupted belongs here for the same reason
                // and one more: the row was rebuilt from an on-disk checkpoint
                // after a process death, and that state deliberately has no
                // retry affordance in the UI - routing it through emit_offer
                // left the offer waiting for a button that does not exist.
                self.accept(existing);
            } else {
                self.emit_offer(&existing);
            }
            return;
        }

        if self.occupied() >= self.max_active() {
            let message = TransferMessage::Reject {
                transfer_id,
                code: code_name(ErrorCode::PeerBusy).into(),
                message: "this device is already running the maximum number of transfers".into(),
            };
            async_runtime::spawn(async move {
                let _ = session.sender.send_transfer(message).await;
            });
            return;
        }

        let mut record = TransferRecord::new(
            transfer_id,
            device_id,
            device_name.to_string(),
            TransferDirection::Receive,
            file_name,
            file_size.max(total_bytes),
            total_files.max(1),
        );
        if let Some(tag) = batch_tag {
            record = record.with_batch(tag);
        }
        let record = Arc::new(record);
        *lock(&record.session) = Some(session);
        *lock(&record.offer) = Some(offer);
        record.set_state(TransferState::Offered);
        self.insert(record.clone());
        // A sender that offers is already a trusted peer, so a user who asked
        // for unattended receiving should not have to click accept every time.
        if self.auto_accept(device_id) {
            tracing::info!(file = %record.file_name, "auto-accepting the offered transfer");
            self.emit_offer(&record);
            self.accept(record);
            return;
        }
        self.emit_offer(&record);

        // A sender that gives up never tells us, so an unanswered offer must
        // release its slot on its own.
        let runtime = self.clone();
        let pending = record.clone();
        async_runtime::spawn(async move {
            tokio::time::sleep(OFFER_TIMEOUT).await;
            runtime.expire_offer(&pending);
        });
    }

    /// Fail an offer the user never answered so its slot becomes reusable.
    fn expire_offer(self: &Arc<Self>, record: &Arc<TransferRecord>) {
        if record.state() != TransferState::Offered {
            return;
        }
        self.fail_with(
            record,
            TransferFailure {
                code: code_name(ErrorCode::Timeout).into(),
                code_number: ErrorCode::Timeout.as_u16(),
                message: "the offer expired before it was answered".into(),
                retryable: false,
            },
        );
    }

    fn emit_offer(&self, record: &Arc<TransferRecord>) {
        self.emit_transfer(TransferEvent::Offered {
            transfer_id: record.transfer_id.to_string(),
            device_id: record.device_id.to_string(),
            device_name: record.device_name.clone(),
            file_name: record.file_name.clone(),
            file_size: record.bytes_total,
            total_files: record.total_files,
        });
    }

    fn resolve(&self, transfer_id: Uuid, decision: PeerDecision) {
        if let Some(record) = self.get(transfer_id)
            && let Some(sender) = lock(&record.pending_accept).take()
        {
            let _ = sender.send(decision);
        }
    }

    /// Update one transfer's progress and emit a throttled event.
    pub fn on_progress(self: &Arc<Self>, transfer_id: Uuid, bytes_done: u64, bytes_total: u64) {
        let Some(record) = self.get(transfer_id) else {
            return;
        };
        record.bytes_done.store(bytes_done, Ordering::SeqCst);
        let now = Instant::now();
        let speed = lock(&record.speed_meter).sample(bytes_done, now);
        record.speed.store(speed, Ordering::SeqCst);
        record.updated_at_ms.store(now_ms(), Ordering::SeqCst);
        if lock(&record.throttle).ready(now) {
            self.emit_transfer(TransferEvent::Progress {
                transfer_id: transfer_id.to_string(),
                bytes_done,
                bytes_total,
                bytes_per_second: speed,
            });
        }
        let send_wire = record.direction == TransferDirection::Receive
            && lock(&record.wire_throttle).ready(now);
        if send_wire && let Some(session) = lock(&record.session).clone() {
            let message = TransferMessage::Progress {
                transfer_id,
                bytes_done,
                bytes_total,
            };
            async_runtime::spawn(async move {
                let _ = session.sender.send_transfer(message).await;
            });
        }
    }

    async fn run_sender(self: Arc<Self>, record: Arc<TransferRecord>) {
        self.enter_transfer();
        let outcome = self.send_inner(&record).await;
        self.leave_transfer();
        match outcome {
            Ok(()) => self.complete(&record),
            Err(SendFailure::Canceled) => self.cancelled(&record),
            Err(SendFailure::Rejected { code, message }) => self.fail_with(
                &record,
                TransferFailure {
                    code,
                    code_number: 3001,
                    message,
                    retryable: true,
                },
            ),
            Err(SendFailure::Transfer(error)) => self.fail(&record, error),
        }
    }

    async fn send_inner(&self, record: &Arc<TransferRecord>) -> Result<(), SendFailure> {
        let session = self.wait_for_session(record).await?;
        *lock(&record.session) = Some(session.clone());
        let manifest_started = Instant::now();
        let (manifest, source) = self.build_manifest(record).await?;
        tracing::info!(
            file = %record.file_name,
            bytes = manifest.file_size,
            elapsed_ms = manifest_started.elapsed().as_millis() as u64,
            "manifest ready; offering the transfer"
        );
        let mut offer = TransferOfferDraft::file(
            record.transfer_id,
            record.file_name.clone(),
            manifest.file_size,
            manifest.block_size,
            manifest.root_hash,
            manifest.manifest_digest,
            manifest.to_bytes(),
        );
        if let Some(tag) = &record.batch {
            offer.batch_id = Some(tag.batch_id);
            offer.root_name = Some(tag.root_name.clone());
            offer.relative_path = Some(tag.relative_path.clone());
            let send = lock(&self.batch_sends).get(&tag.batch_id).cloned();
            if let Some(send) = send
                && !send.entries_taken.swap(true, Ordering::SeqCst)
            {
                // The first sub-transfer of the batch describes the whole
                // folder; later ones only name their own relative path.
                offer.entries = send
                    .plan
                    .entries
                    .iter()
                    .map(|entry| BatchEntryWire {
                        relative_path: entry.relative_path.clone(),
                        size: entry.size,
                        mtime_ms: entry.mtime_ms,
                        is_dir: entry.is_dir,
                    })
                    .collect();
                offer.total_files = send.plan.file_count() as u32;
                offer.total_bytes = send.plan.total_bytes();
            }
        }
        session
            .sender
            .send_transfer(TransferMessage::Offer(Box::new(offer)))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "sending the transfer offer failed");
                SendFailure::Transfer(TransferError::Control(error.to_string()))
            })?;
        tracing::info!(file = %record.file_name, "offer sent; waiting for the peer decision");
        self.wait_for_decision(record).await?;
        record.set_state(TransferState::Transferring);
        let identity = self
            .identity()
            .map_err(|error| SendFailure::Transfer(TransferError::Control(error.message)))?;
        let control = record.control();
        send_file(
            &session.connection,
            &identity,
            session.session_id,
            record.transfer_id,
            manifest,
            source,
            self.config,
            Some(&control),
        )
        .await
        .map_err(SendFailure::Transfer)?;
        let _ = session
            .sender
            .send_transfer(TransferMessage::Complete {
                transfer_id: record.transfer_id,
            })
            .await;
        Ok(())
    }

    async fn wait_for_session(
        &self,
        record: &Arc<TransferRecord>,
    ) -> Result<TransferSession, SendFailure> {
        let deadline = Instant::now() + SESSION_TIMEOUT;
        tracing::info!(device = %record.device_id, "waiting for a chat session to send on");
        loop {
            if record.control().is_cancelled() {
                return Err(SendFailure::Canceled);
            }
            let chat = lock(&self.chat).clone();
            if let Some(chat) = chat
                && let Some(session) = chat.transfer_session(record.device_id)
            {
                // Compare the connection's health, not its remote address: an
                // accepted connection reports the peer's ephemeral source port,
                // so comparing it with the port the peer listens on rejected
                // perfectly good sessions on every retry.
                if session.connection.close_reason().is_some() {
                    tracing::info!(
                        remote = %session.connection.remote_address(),
                        "chat session is already closed; waiting for a new one"
                    );
                } else {
                    tracing::info!("chat session acquired; hashing the file");
                    return Ok(session);
                }
            }
            if Instant::now() >= deadline {
                tracing::warn!(
                    device = %record.device_id,
                    file = %record.file_name,
                    "no chat session became ready before the deadline"
                );
                return Err(SendFailure::Transfer(TransferError::Timeout));
            }
            tokio::time::sleep(PROGRESS_THROTTLE).await;
        }
    }

    async fn build_manifest(
        &self,
        record: &Arc<TransferRecord>,
    ) -> Result<(Arc<Manifest>, Arc<dyn FileSource>), SendFailure> {
        let plan = lock(&record.source)
            .clone()
            .ok_or(SendFailure::Transfer(TransferError::Timeout))?;
        let manager = self.manager();
        let source = manager
            .open_source(plan.handle)
            .map_err(|error| SendFailure::Transfer(error.into()))?;
        let key = ManifestCache::key_for_source(&plan.source_key, BLOCK_SIZE);
        let options = ManifestOptions {
            block_size: BLOCK_SIZE,
            ..ManifestOptions::default()
        };
        let cached = self
            .manifests
            .get_or_compute(&key, source.as_ref(), options, &mut |_progress| {})
            .map_err(|error| {
                SendFailure::Transfer(TransferError::InvalidManifest(error.to_string()))
            })?;
        Ok((Arc::new(cached.manifest), Arc::from(source)))
    }

    async fn wait_for_decision(&self, record: &Arc<TransferRecord>) -> Result<(), SendFailure> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        *lock(&record.pending_accept) = Some(sender);
        match tokio::time::timeout(OFFER_TIMEOUT, receiver).await {
            Ok(Ok(PeerDecision::Accept)) => Ok(()),
            Ok(Ok(PeerDecision::Reject)) => Err(SendFailure::Rejected {
                code: "rejected".into(),
                message: "the receiver declined the transfer".into(),
            }),
            Ok(Err(_)) => Err(SendFailure::Canceled),
            Err(_) => Err(SendFailure::Transfer(TransferError::Timeout)),
        }
    }

    async fn run_receiver(self: Arc<Self>, record: Arc<TransferRecord>) {
        self.enter_transfer();
        let outcome = self.receive_inner(&record).await;
        self.leave_transfer();
        match outcome {
            Ok(()) => self.complete(&record),
            Err(TransferError::Canceled) => self.cancelled(&record),
            Err(error) => self.fail(&record, error),
        }
    }

    async fn receive_inner(
        self: &Arc<Self>,
        record: &Arc<TransferRecord>,
    ) -> Result<(), TransferError> {
        let session = lock(&record.session)
            .clone()
            .ok_or_else(|| TransferError::Control("the transfer has no control session".into()))?;
        let offer = lock(&record.offer)
            .clone()
            .ok_or_else(|| TransferError::Control("the transfer has no offer".into()))?;
        let manifest = Arc::new(offer.manifest.clone());
        // Identical content that this process already saved is not sent again:
        // tell the sender, close the transfer as delivered, and never open a
        // sink. The index is memory-only, so a file deleted outside the app
        // between two sends simply transfers as usual.
        if let Ok(hash) = <[u8; 32]>::try_from(manifest.root_hash.as_slice())
            && lock(&self.received_hashes).contains(&hash)
        {
            record.deduplicated.store(true, Ordering::SeqCst);
            tracing::info!(file = %record.file_name, "content already present; telling the sender");
            if let Some(session) = lock(&record.session).clone() {
                let message = TransferMessage::Duplicate {
                    transfer_id: record.transfer_id,
                };
                async_runtime::spawn(async move {
                    let _ = session.sender.send_transfer(message).await;
                });
            }
            return Ok(());
        }
        // The sender mints a new transfer id for every send, and after a restart
        // it cannot reuse the old one, so look the checkpoint up by content
        // first and fall back to the id for the same-process case.
        let plan = match self
            .checkpoints
            .resume_plan(record.transfer_id, &manifest)?
        {
            Some(plan) => Some(plan),
            None => self
                .checkpoints
                .resume_plan_for(&offer.file_name, &manifest)?
                .map(|(_id, plan)| plan),
        };
        // A batch file lands under its root; a single file lands in the current
        // receive directory, which can change between transfers.
        let receive_manager;
        let manager: &dyn FileAccessManager = match &offer.sink_dir {
            Some(dir) => {
                receive_manager = DesktopFileAccessManager::new(dir.clone());
                &receive_manager
            }
            None => {
                receive_manager = DesktopFileAccessManager::new(self.receive_root());
                &receive_manager
            }
        };
        tracing::info!(
            transfer_id = %record.transfer_id,
            plan = plan.is_some(),
            "receiver: preparing the sink"
        );
        let mut sink = prepare_sink(
            manager,
            record.transfer_id,
            &offer.file_name,
            &manifest,
            plan.as_ref(),
        )?;
        tracing::info!(transfer_id = %record.transfer_id, "receiver: sink ready");
        let identity = self
            .identity()
            .map_err(|error| TransferError::Control(error.message))?;
        tracing::info!(
            transfer_id = %record.transfer_id,
            device_id = %record.device_id,
            "receiver resume for device"
        );
        // Hand the plan to the resume object, not just to the sink. Without this
        // the stored bitmap reaches the file but never the checkpoint, so the
        // transfer still starts from zero.
        let resume = match plan {
            Some(plan) => {
                CheckpointResume::from_plan(&self.checkpoints, offer.file_name.clone(), Some(plan))
            }
            None => CheckpointResume::new(&self.checkpoints, offer.file_name.clone()),
        }
        .for_device(record.device_id);
        let progress = |done: u64, total: u64| self.on_progress(record.transfer_id, done, total);
        let control = record.control();
        let options = TransferOptions {
            resume: Some(&resume),
            control: Some(&control),
            keep_checkpoint_on_cancel: false,
            progress: Some(&progress),
        };
        let result = receive_file_resumable(
            &session.connection,
            &identity,
            session.session_id,
            record.transfer_id,
            manifest,
            sink.as_mut(),
            self.config,
            options,
        )
        .await;
        match result {
            Ok(_) => {
                sink.finalize()?;
                let _ = session
                    .sender
                    .send_transfer(TransferMessage::Complete {
                        transfer_id: record.transfer_id,
                    })
                    .await;
                Ok(())
            }
            Err(error) => {
                let code = code_name(error.code());
                let _ = session
                    .sender
                    .send_transfer(TransferMessage::Error {
                        transfer_id: record.transfer_id,
                        code: code.into(),
                        message: error.to_string(),
                        retryable: error.is_retryable(),
                    })
                    .await;
                Err(error)
            }
        }
    }

    /// Path a finished receive ended up at: the batch sub-directory when the
    /// offer carried one, otherwise the receive root. Used by the Android
    /// export step and by "open with the default app" on both platforms.
    fn received_path(&self, record: &Arc<TransferRecord>) -> Option<PathBuf> {
        let offer = lock(&record.offer).clone()?;
        let root = match offer.sink_dir {
            Some(dir) => dir,
            None => self.receive_root(),
        };
        Some(root.join(&offer.file_name))
    }

    /// Where a completed receive ended up, for "open with the default app".
    /// Open a completed receive with whatever the desktop associates with it.
    ///
    /// Android is not wired yet: the file there is reached through a
    /// MediaStore/SAF URI rather than a path, so the Kotlin side has to resolve
    /// it. Until then this reports the limitation instead of failing silently.
    pub(crate) async fn open_received(
        &self,
        transfer_id: &str,
    ) -> Result<(), crate::contract::IpcError> {
        let id = Uuid::parse_str(transfer_id)
            .map_err(|_| crate::contract::IpcError::internal("the transfer id is not a uuid"))?;

        // Android hands the file to another app, which has to go through a
        // content:// Uri and therefore the platform bridge; the other three
        // spawn the system opener.
        #[cfg(target_os = "android")]
        {
            use tauri::Manager;
            let app = self
                .app
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .ok_or_else(|| {
                    crate::contract::IpcError::internal("the app handle is not attached yet")
                })?;
            let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
            // The export is checked first on purpose. A receive that left the
            // sandbox for MediaStore has had its original deleted, so
            // received_file_path would fail before we ever got here - the
            // content URI is the only address that still resolves. The path
            // is the fallback for a receive that was never exported.
            if let Some(uri) = Self::exported_uri(&app, id) {
                return handle
                    .open_file(uri)
                    .await
                    .map_err(crate::contract::IpcError::internal);
            }
            let path = self
                .received_file_path(id)
                .map_err(|failure| crate::contract::IpcError::internal(failure.message))?;
            return handle
                .open_file(path.to_string_lossy().into_owned())
                .await
                .map_err(crate::contract::IpcError::internal);
        }

        #[cfg(not(target_os = "android"))]
        let path = self
            .received_file_path(id)
            .map_err(|failure| crate::contract::IpcError::internal(failure.message))?;

        #[cfg(not(target_os = "android"))]
        {
            #[cfg(target_os = "windows")]
            let spawned = std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(&path)
                .spawn();

            #[cfg(target_os = "macos")]
            let spawned = std::process::Command::new("open").arg(&path).spawn();

            #[cfg(all(unix, not(target_os = "macos")))]
            let spawned = std::process::Command::new("xdg-open").arg(&path).spawn();

            spawned
                .map(|_| ())
                .map_err(|error| crate::contract::IpcError::internal(error.to_string()))
        }
    }

    fn received_file_path(&self, transfer_id: Uuid) -> Result<PathBuf, TransferFailure> {
        let record = lock(&self.records)
            .get(&transfer_id)
            .cloned()
            .ok_or_else(|| TransferFailure {
                code: "invalid_file_id".to_string(),
                code_number: 3003,
                message: "unknown transfer".to_string(),
                retryable: false,
            })?;
        if record.direction != TransferDirection::Receive
            || *lock(&record.state) != TransferState::Completed
        {
            return Err(TransferFailure {
                code: "invalid_state".to_string(),
                code_number: 3004,
                message: "only a completed receive can be opened".to_string(),
                retryable: false,
            });
        }
        let path = self.received_path(&record).ok_or_else(|| TransferFailure {
            code: "invalid_file_id".to_string(),
            code_number: 3003,
            message: "no received file to open".to_string(),
            retryable: false,
        })?;
        if !path.exists() {
            return Err(TransferFailure {
                code: "file_access".to_string(),
                code_number: 3001,
                message: "the received file is gone".to_string(),
                retryable: false,
            });
        }
        Ok(path)
    }
    /// On Android a finished receive is copied into the public downloads so the
    /// user can find it, then the sandbox copy is removed and the export is
    /// recorded next to the app data. Both steps are best effort: a failed export
    /// must not turn a completed transfer into a failed one.
    #[cfg(target_os = "android")]
    fn export_received(self: &Arc<Self>, record: &Arc<TransferRecord>) {
        if record.direction != TransferDirection::Receive {
            return;
        }
        // With all-files access the receive root already is the public folder, so
        // the file is visible where it landed and copying it into MediaStore
        // would only duplicate it.
        if let Some(path) = self.received_path(record)
            && path.starts_with("/storage/emulated")
            && !path.starts_with("/storage/emulated/0/Android/")
        {
            tracing::info!(path = %path.display(), "received file landed in the public folder");
            return;
        }
        let Some(app) = self.app.lock().ok().and_then(|slot| slot.clone()) else {
            tracing::warn!("no app handle; skipping the export of the received file");
            return;
        };
        let Some(path) = self.received_path(record) else {
            return;
        };
        let name = record.file_name.clone();
        let transfer_id = record.transfer_id;
        let runtime = self.clone();
        async_runtime::spawn(async move {
            use tauri::Manager;
            let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
            let mime = mime_for(&name);
            match handle
                .export(
                    path.to_string_lossy().into_owned(),
                    name.clone(),
                    mime.to_string(),
                )
                .await
            {
                Ok(uri) => {
                    tracing::info!(%name, %uri, "received file exported to the public downloads");
                    runtime.record_export(transfer_id, &name, &uri);
                    if let Err(error) = std::fs::remove_file(&path) {
                        tracing::warn!(%error, %name, "the sandbox copy could not be removed");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, %name, "exporting the received file failed");
                }
            }
        });
    }

    /// The content URI an exported receive was given, when it was exported.
    #[cfg(target_os = "android")]
    fn exported_uri(app: &tauri::AppHandle<tauri::Wry>, transfer_id: Uuid) -> Option<String> {
        use tauri::Manager;
        let dir = app.path().app_data_dir().ok()?.join("exports");
        let bytes = std::fs::read(dir.join(format!("{transfer_id}.json"))).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        value.get("uri")?.as_str().map(str::to_string)
    }

    /// Write the export trail the mobile plan asks for: one small JSON per
    /// transfer, so a restart can tell what was already exported where.
    #[cfg(target_os = "android")]
    fn record_export(&self, transfer_id: Uuid, name: &str, uri: &str) {
        let Some(app) = self.app.lock().ok().and_then(|slot| slot.clone()) else {
            return;
        };
        use tauri::Manager;
        let Ok(dir) = app.path().app_data_dir() else {
            return;
        };
        let dir = dir.join("exports");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let entry = serde_json::json!({
            "transfer_id": transfer_id.to_string(),
            "name": name,
            "uri": uri,
            "exported_at_ms": now_ms(),
        });
        let path = dir.join(format!("{transfer_id}.json"));
        if let Err(error) =
            std::fs::write(&path, serde_json::to_vec_pretty(&entry).unwrap_or_default())
        {
            tracing::warn!(%error, "recording the export failed");
        }
    }

    fn complete(self: &Arc<Self>, record: &Arc<TransferRecord>) {
        #[cfg(target_os = "android")]
        if !record.deduplicated.load(Ordering::SeqCst) {
            self.export_received(record);
        }
        // Record what actually happened, so a slow run can be judged from the log
        // instead of from a bar that moves.
        let elapsed = (now_ms() - record.created_at_ms) as f64 / 1000.0;
        if elapsed > 0.0 {
            tracing::info!(
                file = %record.file_name,
                bytes = record.bytes_total,
                elapsed_ms = (elapsed * 1000.0) as u64,
                mib_per_s = (record.bytes_total as f64 / 1_048_576.0 / elapsed),
                "transfer finished"
            );
        }
        record.set_state(TransferState::Completed);
        record
            .bytes_done
            .store(record.bytes_total, Ordering::SeqCst);
        if record.direction == TransferDirection::Receive
            && let Some(offer) = lock(&record.offer).clone()
            && let Ok(hash) = <[u8; 32]>::try_from(offer.manifest.root_hash.as_slice())
        {
            lock(&self.received_hashes).insert(hash);
        }
        self.emit_transfer(TransferEvent::Completed {
            transfer_id: record.transfer_id.to_string(),
        });
        self.notify_outcome(record, true);
        self.pump_queue();
    }

    fn cancelled(self: &Arc<Self>, record: &Arc<TransferRecord>) {
        record.set_state(TransferState::Cancelled);
        self.emit_transfer(TransferEvent::Cancelled {
            transfer_id: record.transfer_id.to_string(),
        });
        self.pump_queue();
    }

    fn fail(self: &Arc<Self>, record: &Arc<TransferRecord>, error: TransferError) {
        let core: fluxy_core::Error = error.into();
        self.fail_with(
            record,
            TransferFailure {
                code: code_name(core.code).into(),
                code_number: core.code_number,
                message: core.message.clone(),
                retryable: core.retryable,
            },
        );
    }

    /// Record a failure, publish it, and hand the freed slot to the queue.
    fn fail_with(self: &Arc<Self>, record: &Arc<TransferRecord>, failure: TransferFailure) {
        record.set_state(TransferState::Failed);
        *lock(&record.failure) = Some(failure.clone());
        self.emit_transfer(TransferEvent::Failed {
            transfer_id: record.transfer_id.to_string(),
            code: failure.code,
            message: failure.message,
            retryable: failure.retryable,
        });
        self.notify_outcome(record, false);
        self.pump_queue();
    }
}

enum SendFailure {
    Canceled,
    Rejected { code: String, message: String },
    Transfer(TransferError),
}

fn send_control(session: Option<TransferSession>, message: TransferMessage) {
    let Some(session) = session else {
        return;
    };
    async_runtime::spawn(async move {
        let _ = session.sender.send_transfer(message).await;
    });
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Pick a batch root that does not collide with what is already received.
fn unique_batch_root(received: &std::path::Path, root_name: &str) -> PathBuf {
    let base = sanitize_root_name(root_name);
    let mut candidate = received.join(&base);
    let mut index = 1;
    while candidate.exists() {
        candidate = received.join(format!("{base} ({index})"));
        index += 1;
    }
    candidate
}

fn parse_uuid(value: &str, what: &str) -> Result<Uuid, IpcError> {
    Uuid::parse_str(value).map_err(|_| IpcError {
        code: "protocol".into(),
        code_number: 1001,
        message: format!("invalid {what} id"),
        retryable: false,
    })
}

fn peer_offline() -> IpcError {
    IpcError {
        code: "peer_offline".into(),
        code_number: 4201,
        message: "device is not currently reachable on the LAN".into(),
        retryable: true,
    }
}

fn code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Protocol => "protocol",
        ErrorCode::UnsupportedVersion => "unsupported_version",
        ErrorCode::Identity => "identity",
        ErrorCode::Trust => "trust",
        ErrorCode::Timeout => "timeout",
        ErrorCode::NetworkUnavailable => "network_unavailable",
        ErrorCode::Transfer => "transfer",
        ErrorCode::SourceChanged => "source_changed",
        ErrorCode::FileAccess => "file_access",
        ErrorCode::DiskFull => "disk_full",
        ErrorCode::HashMismatch => "hash_mismatch",
        ErrorCode::MissingChunks => "missing_chunks",
        ErrorCode::Canceled => "canceled",
        ErrorCode::ClipboardTooLarge => "clipboard_too_large",
        ErrorCode::PeerBusy => "peer_busy",
        ErrorCode::Internal => "internal",
    }
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
    use fluxy_app::AppPaths;

    fn runtime() -> (Arc<TransferRuntime>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let engine = Engine::load(paths).unwrap();
        let hub = Arc::new(EventHub::new());
        (TransferRuntime::new(Arc::new(Mutex::new(engine)), hub), dir)
    }

    fn record(direction: TransferDirection, name: &str, size: u64) -> Arc<TransferRecord> {
        Arc::new(TransferRecord::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            "Peer".into(),
            direction,
            name.into(),
            size,
            1,
        ))
    }

    #[test]
    fn progress_throttle_coalesces_to_the_interval() {
        let start = Instant::now();
        let mut throttle = ProgressThrottle::new(Duration::from_millis(100));
        assert!(throttle.ready(start));
        assert!(!throttle.ready(start + Duration::from_millis(50)));
        assert!(!throttle.ready(start + Duration::from_millis(99)));
        assert!(throttle.ready(start + Duration::from_millis(100)));
        assert!(!throttle.ready(start + Duration::from_millis(150)));
        assert!(throttle.ready(start + Duration::from_millis(200)));
    }

    #[test]
    fn a_thousand_progress_updates_emit_at_most_one_per_interval() {
        let start = Instant::now();
        let mut throttle = ProgressThrottle::new(PROGRESS_THROTTLE);
        let mut emitted = 0;
        for step in 0..1000 {
            let now = start + Duration::from_millis(step);
            if throttle.ready(now) {
                emitted += 1;
            }
        }
        // One second of 1 ms updates must coalesce to about ten events.
        assert!((10..=11).contains(&emitted), "emitted {emitted}");
    }

    #[test]
    fn speed_meter_smooths_a_burst() {
        let start = Instant::now();
        let mut meter = SpeedMeter::new();
        assert_eq!(meter.sample(0, start), 0);
        // 1 MiB in 100 ms is 10 MiB/s.
        // 3 MiB over 300 ms is 10 MiB/s, and 300 ms clears the
        // MIN_SAMPLE_SECONDS floor that the 100 ms sample used to sit below.
        let first = meter.sample(3 * 1024 * 1024, start + Duration::from_millis(300));
        assert_eq!(first, 10 * 1024 * 1024);
        // A stalled window pulls the smoothed rate down without jumping to 0.
        let second = meter.sample(3 * 1024 * 1024, start + Duration::from_millis(900));
        assert!(second < first);
        assert!(second > 0);
    }

    #[test]
    fn completion_and_failure_notify_but_a_cancel_does_not() {
        let (wired, _dir) = runtime();
        let notifier = Arc::new(Notifier::default());
        wired.attach_notifier(notifier.clone());

        let done = record(TransferDirection::Receive, "report.bin", 10);
        wired.complete(&done);
        assert_eq!(done.state(), TransferState::Completed);
        assert_eq!(notifier.attempts(), vec!["transfer_completed"]);

        let failed = record(TransferDirection::Send, "notes.txt", 10);
        wired.fail_with(
            &failed,
            TransferFailure {
                code: "disk_full".into(),
                code_number: 3004,
                message: "no space left on device".into(),
                retryable: true,
            },
        );
        assert_eq!(
            notifier.attempts(),
            vec!["transfer_completed", "transfer_failed"]
        );

        // A cancel is a user decision, not an outcome worth interrupting for.
        let cancelled = record(TransferDirection::Send, "movie.mkv", 10);
        wired.cancelled(&cancelled);
        assert_eq!(
            notifier.attempts(),
            vec!["transfer_completed", "transfer_failed"]
        );

        // Without a notifier the same transitions stay silent.
        let (bare, _dir) = runtime();
        let quiet = record(TransferDirection::Receive, "quiet.bin", 10);
        bare.complete(&quiet);
        assert_eq!(notifier.attempts().len(), 2);
    }

    #[test]
    fn register_file_returns_an_opaque_handle() {
        let (runtime, dir) = runtime();
        let path = dir.path().join("report.bin");
        std::fs::write(&path, b"hello").unwrap();
        let view = runtime.register_file(path.to_str().unwrap()).unwrap();
        assert_eq!(view.name, "report.bin");
        assert_eq!(view.size, 5);
        assert!(Uuid::parse_str(&view.file_id).is_ok());
        // The registry keeps the path; the IPC view never exposes it.
        assert!(!format!("{view:?}").contains(dir.path().to_str().unwrap()));
        assert_eq!(lock(&runtime.files).len(), 1);
    }

    #[test]
    fn register_file_rejects_a_directory_and_a_missing_path() {
        let (runtime, dir) = runtime();
        assert!(runtime.register_file(dir.path().to_str().unwrap()).is_err());
        assert!(
            runtime
                .register_file(dir.path().join("nope.bin").to_str().unwrap())
                .is_err()
        );
    }

    #[test]
    fn snapshot_reports_failure_details_and_retryability() {
        let (runtime, _dir) = runtime();
        let record = record(TransferDirection::Send, "big.bin", 1024);
        record.set_state(TransferState::Failed);
        *lock(&record.failure) = Some(TransferFailure {
            code: "source_changed".into(),
            code_number: 3002,
            message: "source changed while reading".into(),
            retryable: false,
        });
        runtime.insert(record);
        let summary = runtime.snapshot().pop().unwrap();
        assert_eq!(summary.state, TransferState::Failed);
        assert_eq!(summary.error_code.as_deref(), Some("source_changed"));
        assert_eq!(summary.device_name, "Peer");
        assert_eq!(summary.file_name, "big.bin");
        assert!(!summary.retryable);
    }

    #[test]
    fn control_rejects_an_unknown_transfer_and_a_completed_retry() {
        let (runtime, _dir) = runtime();
        let unknown = Uuid::now_v7().to_string();
        assert!(runtime.control(&unknown, TransferAction::Pause).is_err());

        let record = record(TransferDirection::Send, "done.bin", 16);
        record.set_state(TransferState::Completed);
        let id = record.transfer_id;
        runtime.insert(record);
        assert!(
            runtime
                .control(&id.to_string(), TransferAction::Retry)
                .is_err()
        );
    }

    #[test]
    fn pause_and_resume_flip_the_local_control_state() {
        let (runtime, _dir) = runtime();
        let record = record(TransferDirection::Receive, "movie.bin", 4096);
        record.set_state(TransferState::Transferring);
        let id = record.transfer_id;
        runtime.insert(record);

        let paused = runtime
            .control(&id.to_string(), TransferAction::Pause)
            .unwrap();
        assert_eq!(paused.state, TransferState::Paused);
        let resumed = runtime
            .control(&id.to_string(), TransferAction::Resume)
            .unwrap();
        assert_eq!(resumed.state, TransferState::Transferring);
        let cancelled = runtime
            .control(&id.to_string(), TransferAction::Cancel)
            .unwrap();
        assert_eq!(cancelled.state, TransferState::Cancelled);
    }

    #[test]
    fn a_received_root_hash_is_remembered_for_the_process_lifetime() {
        let (runtime, _dir) = runtime();
        let hash = [7u8; 32];
        assert!(!lock(&runtime.received_hashes).contains(&hash));
        lock(&runtime.received_hashes).insert(hash);
        assert!(lock(&runtime.received_hashes).contains(&hash));
    }

    #[test]
    fn pause_active_stops_every_moving_transfer() {
        let (runtime, _dir) = runtime();
        let moving = record(TransferDirection::Receive, "movie.bin", 4096);
        moving.set_state(TransferState::Transferring);
        let moving_id = moving.transfer_id;
        runtime.insert(moving);

        let finished = record(TransferDirection::Send, "done.bin", 16);
        finished.set_state(TransferState::Completed);
        let finished_id = finished.transfer_id;
        runtime.insert(finished);

        let waiting = record(TransferDirection::Send, "queued.bin", 16);
        waiting.set_state(TransferState::Queued);
        let waiting_id = waiting.transfer_id;
        runtime.insert(waiting);

        // A network change pauses what is moving and leaves the rest alone.
        runtime.pause_active();
        let states = runtime
            .snapshot()
            .into_iter()
            .map(|summary| (summary.transfer_id, summary.state))
            .collect::<HashMap<_, _>>();
        assert_eq!(states[&moving_id.to_string()], TransferState::Paused);
        assert_eq!(states[&finished_id.to_string()], TransferState::Completed);
        assert_eq!(states[&waiting_id.to_string()], TransferState::Queued);

        // Idempotent: a second rebind finds nothing to pause.
        runtime.pause_active();
        assert_eq!(
            runtime
                .snapshot()
                .into_iter()
                .filter(|summary| summary.state == TransferState::Transferring)
                .count(),
            0
        );
    }

    fn batch_offer(relative_path: &str, size: u64) -> BatchOffer {
        BatchOffer {
            batch_id: Some(Uuid::now_v7()),
            root_name: Some("holiday".into()),
            relative_path: Some(relative_path.into()),
            entries: vec![
                BatchEntryWire {
                    relative_path: relative_path.into(),
                    size,
                    mtime_ms: 0,
                    is_dir: false,
                },
                BatchEntryWire {
                    relative_path: "empty".into(),
                    size: 0,
                    mtime_ms: 0,
                    is_dir: true,
                },
            ],
        }
    }

    #[test]
    fn registering_a_folder_describes_its_files_and_empty_directories() {
        let (runtime, dir) = runtime();
        let root = dir.path().join("holiday");
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("photos/one.jpg"), b"12345").unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();

        let view = runtime.register_folder(&root.to_string_lossy()).unwrap();

        assert_eq!(view.root_name, "holiday");
        assert_eq!(view.total_files, 1);
        assert_eq!(view.total_bytes, 5);
        assert_eq!(view.empty_directories, 1);
        assert!(
            view.sample
                .iter()
                .any(|entry| entry.relative_path == "photos/one.jpg")
        );
    }

    #[test]
    fn registering_a_file_as_a_folder_is_3003() {
        let (runtime, dir) = runtime();
        let file = dir.path().join("plain.txt");
        std::fs::write(&file, b"x").unwrap();

        let error = runtime
            .register_folder(&file.to_string_lossy())
            .expect_err("a file is not a folder");

        assert_eq!(error.code_number, 3003);
    }

    #[test]
    fn starting_an_unknown_folder_is_3003() {
        let (runtime, _dir) = runtime();
        let error = runtime
            .start_folder(&Uuid::now_v7().to_string(), &Uuid::now_v7().to_string())
            .expect_err("unregistered folder");
        assert_eq!(error.code_number, 3003);
    }

    #[test]
    fn the_receive_root_switches_and_is_created() {
        let (runtime, dir) = runtime();
        let default_root = runtime.receive_root();
        assert!(default_root.ends_with("received"));

        let target = dir.path().join("Downloads").join("Fluxy");
        runtime.set_receive_root(target.clone());

        assert_eq!(runtime.receive_root(), target);
        assert!(target.is_dir(), "the new root is created on demand");
        // A refused directory leaves the previous root in place.
        let file = dir.path().join("plain.txt");
        std::fs::write(&file, b"x").unwrap();
        runtime.set_receive_root(file);
        assert_eq!(runtime.receive_root(), target);
    }

    #[test]
    fn a_batch_offer_is_placed_under_a_fresh_root_with_its_empty_directories() {
        let (runtime, _dir) = runtime();

        let (tag, sink_dir, name) = runtime
            .place_batch_offer(&batch_offer("photos/one.jpg", 5), 5)
            .unwrap()
            .expect("a batch placement");

        assert_eq!(name, "one.jpg");
        assert_eq!(tag.relative_path, "photos/one.jpg");
        assert!(sink_dir.ends_with("photos"));
        assert!(sink_dir.starts_with(runtime.receive_root().as_path()));
        assert!(sink_dir.exists());
        // The empty directory travelled with the first offer and was created.
        assert!(
            runtime
                .receive_root()
                .join("holiday")
                .join("empty")
                .is_dir()
        );
    }

    #[test]
    fn a_second_batch_with_the_same_root_gets_a_numbered_directory() {
        let (runtime, _dir) = runtime();
        std::fs::create_dir_all(runtime.receive_root().join("holiday")).unwrap();

        let (_tag, sink_dir, _name) = runtime
            .place_batch_offer(&batch_offer("one.jpg", 1), 1)
            .unwrap()
            .expect("a batch placement");

        assert!(sink_dir.ends_with("holiday (1)"));
    }

    #[test]
    fn a_batch_offer_with_an_unsafe_path_is_refused() {
        let (runtime, _dir) = runtime();
        for path in ["../escape.jpg", "/etc/passwd", "a/../../b", "CON"] {
            let error = runtime
                .place_batch_offer(&batch_offer(path, 1), 1)
                .expect_err("an unsafe relative path");
            assert_eq!(error.code, "file_access", "path {path}");
        }
    }

    #[test]
    fn a_batch_offer_outside_its_entry_list_is_refused() {
        let (runtime, _dir) = runtime();
        let mut offer = batch_offer("one.jpg", 5);
        offer.relative_path = Some("other.jpg".into());

        let error = runtime
            .place_batch_offer(&offer, 5)
            .expect_err("not in the entry list");
        assert_eq!(error.code, "invalid_batch");
    }

    #[test]
    fn a_batch_offer_with_a_mismatched_size_is_refused() {
        let (runtime, _dir) = runtime();
        let error = runtime
            .place_batch_offer(&batch_offer("one.jpg", 5), 6)
            .expect_err("a size that disagrees with the entry list");
        assert_eq!(error.code, "invalid_batch");
    }

    #[test]
    fn on_progress_updates_the_summary() {
        let (runtime, _dir) = runtime();
        let record = record(TransferDirection::Receive, "movie.bin", 4096);
        let id = record.transfer_id;
        runtime.insert(record);
        runtime.on_progress(id, 2048, 4096);
        let summary = runtime.snapshot().pop().unwrap();
        assert_eq!(summary.bytes_done, 2048);
        assert_eq!(summary.bytes_total, 4096);
    }

    #[test]
    fn progress_events_are_coalesced_through_the_hub() {
        let (runtime, _dir) = runtime();
        let mut receiver = runtime.hub.subscribe();
        let record = record(TransferDirection::Receive, "huge.bin", 1000);
        let id = record.transfer_id;
        runtime.insert(record);
        for step in 0..1000 {
            runtime.on_progress(id, step, 1000);
        }
        let mut progress = 0;
        while let Ok(envelope) = receiver.try_recv() {
            if let AppEvent::Transfer {
                data: TransferEvent::Progress { .. },
            } = envelope.event
            {
                progress += 1;
            }
        }
        assert!((1..=2).contains(&progress), "progress events: {progress}");
    }

    fn runtime_with_cap(cap: u32) -> (Arc<TransferRuntime>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let mut engine = Engine::load(paths).unwrap();
        engine.set_max_active_transfers(cap).unwrap();
        let hub = Arc::new(EventHub::new());
        (TransferRuntime::new(Arc::new(Mutex::new(engine)), hub), dir)
    }

    fn queued(direction: TransferDirection, name: &str) -> Arc<TransferRecord> {
        let record = record(direction, name, 10);
        record.set_state(TransferState::Queued);
        record
    }

    #[test]
    fn the_active_cap_comes_from_the_config() {
        let (runtime, _dir) = runtime_with_cap(3);
        assert_eq!(runtime.max_active(), 3);
        let (clamped, _dir) = runtime_with_cap(0);
        assert_eq!(clamped.max_active(), 1);
    }

    #[test]
    fn occupied_counts_active_and_offered_records() {
        let (runtime, _dir) = runtime();
        let active = record(TransferDirection::Send, "a.bin", 10);
        active.set_state(TransferState::Transferring);
        let offered = record(TransferDirection::Receive, "b.bin", 10);
        offered.set_state(TransferState::Offered);
        let waiting = queued(TransferDirection::Send, "c.bin");
        for record in [active, offered, waiting] {
            runtime.insert(record);
        }
        assert_eq!(runtime.active_count(), 1);
        assert_eq!(runtime.offered_count(), 1);
        assert_eq!(runtime.occupied(), 2);
    }

    #[test]
    fn the_queue_claims_the_oldest_send_first() {
        let (runtime, _dir) = runtime();
        let first = queued(TransferDirection::Send, "first.bin");
        let second = queued(TransferDirection::Send, "second.bin");
        runtime.insert(first.clone());
        runtime.insert(second.clone());

        let claimed = runtime.claim_next_queued().unwrap();
        assert_eq!(claimed.transfer_id, first.transfer_id);
        assert_eq!(claimed.state(), TransferState::Preparing);
        assert_eq!(runtime.active_count(), 1);

        let next = runtime.claim_next_queued().unwrap();
        assert_eq!(next.transfer_id, second.transfer_id);
        assert!(runtime.claim_next_queued().is_none());
    }

    #[test]
    fn the_queue_ignores_receiving_records() {
        let (runtime, _dir) = runtime();
        runtime.insert(queued(TransferDirection::Receive, "in.bin"));
        assert!(runtime.claim_next_queued().is_none());
    }

    #[tokio::test]
    async fn a_batch_over_the_file_cap_is_rejected() {
        let (runtime, _dir) = runtime();
        let too_many = vec!["file".to_string(); MAX_BATCH_FILES + 1];
        let error = runtime
            .start_transfers("device", &too_many)
            .await
            .unwrap_err();
        assert_eq!(error.code_number, 3001);
        let empty = runtime.start_transfers("device", &[]).await.unwrap_err();
        assert_eq!(empty.code_number, 3001);
    }

    #[test]
    fn a_dropped_folder_becomes_a_batch_and_a_file_stays_a_transfer() {
        let (runtime, _dir) = runtime();
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("one.bin");
        let two = dir.path().join("two.bin");
        std::fs::write(&one, b"one").unwrap();
        std::fs::write(&two, b"two").unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(nested.join("photos")).unwrap();
        std::fs::write(nested.join("photos/a.jpg"), b"aaaa").unwrap();

        let (files, folders, rejected, skipped) =
            runtime.register_paths(&[one.clone(), nested, two.clone()]);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "one.bin");
        assert_eq!(files[1].size, 3);
        // The folder is described, not refused.
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].root_name, "nested");
        assert_eq!(folders[0].total_files, 1);
        assert_eq!(folders[0].total_bytes, 4);
        assert!(rejected.is_empty());
        assert_eq!(skipped, 0);

        // A directory the scanner refuses is still reported, not silently lost.
        let runaway = dir.path().join("runaway");
        std::fs::create_dir(&runaway).unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&one, runaway.join("link.bin")).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&one, runaway.join("link.bin")).is_ok();
        #[cfg(not(any(unix, windows)))]
        let linked = false;
        if linked {
            let (_, folders, rejected, _) = runtime.register_paths(&[runaway]);
            assert!(folders.is_empty());
            assert_eq!(rejected, vec!["runaway".to_string()]);
        }

        let many: Vec<PathBuf> = (0..MAX_BATCH_FILES + 2)
            .map(|index| dir.path().join(format!("missing-{index}.bin")))
            .collect();
        let (files, _folders, rejected, skipped) = runtime.register_paths(&many);
        assert!(files.is_empty());
        assert_eq!(rejected.len(), MAX_BATCH_FILES);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn drag_hover_is_published_once_per_transition() {
        let (runtime, _dir) = runtime();
        let mut receiver = runtime.hub.subscribe();
        runtime.set_drag_hover(true);
        runtime.set_drag_hover(true);
        runtime.set_drag_hover(false);
        let mut hover = 0;
        while let Ok(envelope) = receiver.try_recv() {
            if let AppEvent::Transfer {
                data: TransferEvent::DragHover { .. },
            } = envelope.event
            {
                hover += 1;
            }
        }
        assert_eq!(hover, 2);
    }

    #[test]
    fn a_ten_file_drop_registers_every_handle() {
        let (runtime, _dir) = runtime();
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..10)
            .map(|index| {
                let path = dir.path().join(format!("drop-{index}.bin"));
                std::fs::write(&path, b"payload").unwrap();
                path
            })
            .collect();
        let mut receiver = runtime.hub.subscribe();
        runtime.accept_drop(paths);
        let mut dropped = 0;
        while let Ok(envelope) = receiver.try_recv() {
            if let AppEvent::Transfer {
                data: TransferEvent::FilesDropped { files, .. },
            } = envelope.event
            {
                dropped += files.len();
            }
        }
        assert_eq!(dropped, 10);
    }

    #[test]
    fn a_full_queue_starts_two_and_holds_the_rest() {
        let (runtime, _dir) = runtime_with_cap(2);
        let records: Vec<Arc<TransferRecord>> = (0..10)
            .map(|index| queued(TransferDirection::Send, &format!("q{index}.bin")))
            .collect();
        for record in &records {
            runtime.insert(record.clone());
        }
        runtime.pump_queue();
        assert_eq!(runtime.active_count(), 2);
        let waiting = records
            .iter()
            .filter(|record| record.state() == TransferState::Queued)
            .count();
        assert_eq!(waiting, 8);
    }

    #[test]
    fn an_unanswered_offer_expires_and_frees_its_slot() {
        let (runtime, _dir) = runtime_with_cap(1);
        let offered = record(TransferDirection::Receive, "in.bin", 10);
        offered.set_state(TransferState::Offered);
        let waiting = queued(TransferDirection::Send, "out.bin");
        runtime.insert(offered.clone());
        runtime.insert(waiting.clone());
        assert_eq!(runtime.occupied(), 1);

        runtime.expire_offer(&offered);
        assert_eq!(offered.state(), TransferState::Failed);
        let summary = offered.summary();
        assert_eq!(summary.error_code.as_deref(), Some("timeout"));
        assert_eq!(summary.error_code_number, Some(2001));
        // The freed slot is handed straight to the queued send.
        assert_eq!(waiting.state(), TransferState::Preparing);

        // Expiring an answered or already failed offer changes nothing.
        runtime.expire_offer(&offered);
        assert_eq!(offered.state(), TransferState::Failed);
    }

    #[test]
    fn code_names_match_the_protocol_table() {
        assert_eq!(code_name(ErrorCode::SourceChanged), "source_changed");
        assert_eq!(code_name(ErrorCode::DiskFull), "disk_full");
        assert_eq!(code_name(ErrorCode::HashMismatch), "hash_mismatch");
        assert_eq!(code_name(ErrorCode::MissingChunks), "missing_chunks");
        assert_eq!(code_name(ErrorCode::Canceled), "canceled");
        assert_eq!(code_name(ErrorCode::PeerBusy), "peer_busy");
        assert_eq!(ErrorCode::PeerBusy.as_u16(), 2002);
    }
}
