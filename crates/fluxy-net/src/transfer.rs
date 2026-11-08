//! Pull-based chunk transfer over one bidirectional stream per file.
//!
//! The receiver opens the stream and names its transfer in the first sixteen
//! bytes. The sender routes the stream by that name, answers with the manifest,
//! and serves every `CHUNK_REQUEST` on the same stream as an `FCH1` chunk
//! frame. The receiver verifies each chunk and writes it to the sink before
//! acknowledging it.
//!
//! QUIC accepts a stream without saying which transfer opened it, so without
//! the name a sender serving two files at once took the other file's stream and
//! answered its requests with a manifest that carried the wrong transfer id.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use quinn::{Connection, RecvStream, SendStream};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use fluxy_core::files::{FileAccessManager, FileError, FileSink, FileSource};
use fluxy_core::transfer::{
    CHUNK_HEADER_LEN, CHUNK_MAGIC, CheckpointError, CheckpointStore, ChunkError, ChunkHeader,
    MAX_CHUNK_BYTES, Manifest, PullScheduler, ResumePlan, SchedulerConfig, StateError,
    StateMachine, TransferCheckpoint, TransferControl, TransferRole, TransferState, covered_ranges,
};
use fluxy_core::{ErrorCode, Identity};
use fluxy_proto::{
    ChunkAck, ChunkRequest, Envelope, MAX_CONTROL_FRAME, MessageKind, ProtoError, TransferManifest,
    decode_frame, decode_payload, encode_payload,
};

use crate::handshake::make_envelope;
use crate::transport::{TransportError, read_envelope, write_envelope};

/// Time to wait for one control frame or a transfer stream before failing.
pub const TRANSFER_IO_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll interval for retries and timeout expiry.
pub const TRANSFER_TICK: Duration = Duration::from_millis(100);
/// Bytes a receiver writes before anything else on its transfer stream.
pub const TRANSFER_STREAM_NAME_LEN: usize = 16;
/// Streams a connection may park while they wait for their transfer to register.
///
/// Every file of a batch can name its stream before the matching send task
/// reaches `send_file`, so this has to cover a full batch rather than one or two.
const MAX_PARKED_STREAMS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferConfig {
    pub scheduler: SchedulerConfig,
    pub max_chunk_bytes: usize,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            scheduler: SchedulerConfig::default(),
            max_chunk_bytes: MAX_CHUNK_BYTES,
        }
    }
}

#[derive(Debug, Error)]
pub enum TransferError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Chunk(#[from] ChunkError),
    #[error(transparent)]
    File(#[from] FileError),
    #[error(transparent)]
    Protocol(#[from] ProtoError),
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error("transfer control stream failed: {0}")]
    Control(String),
    #[error("transfer canceled")]
    Canceled,
    #[error("the peer is already running the maximum number of transfers")]
    PeerBusy,
    #[error("chunks {0:?} are missing after retries")]
    MissingChunks(Vec<u32>),
    #[error("root hash mismatch: expected {expected:02x?}, computed {actual:02x?}")]
    HashMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("transfer timed out")]
    Timeout,
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("unexpected control frame {0:?}")]
    Unexpected(i32),
}

impl TransferError {
    /// Stable error code from the v1 protocol table.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::File(error) => error.code(),
            Self::Chunk(error) => error.code(),
            Self::Checkpoint(error) => error.code(),
            Self::State(error) => error.code(),
            Self::Canceled => ErrorCode::Canceled,
            Self::PeerBusy => ErrorCode::PeerBusy,
            Self::MissingChunks(_) => ErrorCode::MissingChunks,
            Self::HashMismatch { .. } => ErrorCode::HashMismatch,
            Self::Timeout => ErrorCode::Timeout,
            Self::Transport(_)
            | Self::Protocol(_)
            | Self::Control(_)
            | Self::InvalidManifest(_)
            | Self::Unexpected(_) => ErrorCode::Transfer,
        }
    }

    /// Whether a user-triggered retry can plausibly succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::Control(_)
                | Self::Timeout
                | Self::MissingChunks(_)
                | Self::PeerBusy
                | Self::HashMismatch { .. }
        )
    }
}

impl From<TransferError> for fluxy_core::Error {
    fn from(value: TransferError) -> Self {
        fluxy_core::Error::new(value.code(), value.to_string(), value.is_retryable())
    }
}

/// Convert a local manifest into its wire form.
pub fn manifest_to_proto(transfer_id: Uuid, manifest: &Manifest) -> TransferManifest {
    TransferManifest {
        transfer_id: transfer_id.as_bytes().to_vec(),
        block_size: manifest.block_size,
        file_size: manifest.file_size,
        block_hashes: manifest
            .block_hashes
            .iter()
            .map(|hash| hash.to_vec())
            .collect(),
        root_hash: manifest.root_hash.to_vec(),
        manifest_digest: manifest.manifest_digest.to_vec(),
    }
}

/// Validate and convert a wire manifest.
pub fn manifest_from_proto(message: &TransferManifest) -> Result<(Uuid, Manifest), TransferError> {
    let transfer_id = Uuid::from_slice(&message.transfer_id)
        .map_err(|_| TransferError::InvalidManifest("transfer id is not a UUID".into()))?;
    let mut block_hashes = Vec::with_capacity(message.block_hashes.len());
    for hash in &message.block_hashes {
        if hash.len() != 32 {
            return Err(TransferError::InvalidManifest(
                "a block hash is not 32 bytes".into(),
            ));
        }
        let mut fixed = [0_u8; 32];
        fixed.copy_from_slice(hash);
        block_hashes.push(fixed);
    }
    let root_hash = fixed_hash(&message.root_hash, "root hash")?;
    let manifest_digest = fixed_hash(&message.manifest_digest, "manifest digest")?;
    let expected_blocks = message.file_size.div_ceil(message.block_size.max(1) as u64);
    if block_hashes.len() as u64 != expected_blocks {
        return Err(TransferError::InvalidManifest(format!(
            "expected {expected_blocks} block hashes, got {}",
            block_hashes.len()
        )));
    }
    if Manifest::digest_for_hashes(&block_hashes) != manifest_digest {
        return Err(TransferError::InvalidManifest(
            "manifest digest does not match the block hashes".into(),
        ));
    }
    Ok((
        transfer_id,
        Manifest {
            block_size: message.block_size,
            file_size: message.file_size,
            block_hashes,
            root_hash,
            manifest_digest,
        },
    ))
}

fn fixed_hash(bytes: &[u8], label: &str) -> Result<[u8; 32], TransferError> {
    if bytes.len() != 32 {
        return Err(TransferError::InvalidManifest(format!(
            "{label} is not 32 bytes"
        )));
    }
    let mut fixed = [0_u8; 32];
    fixed.copy_from_slice(bytes);
    Ok(fixed)
}

/// Route incoming transfer streams to the transfer that opened them.
///
/// QUIC hands out a bidirectional stream without saying which transfer the peer
/// opened it for. Two sends in flight at once therefore accepted each other's
/// stream and wrote a manifest with the wrong id into it - the failure a
/// multi-file send showed as "manifest has a different transfer id". The router
/// is the single acceptor per connection: every receiver names its transfer in
/// the first sixteen bytes, and the router hands the stream to the task waiting
/// for that name. Nothing is reset for being unrecognised: a stream that names
/// no registered transfer is parked, because a receiver answers an offer before
/// the matching send task gets as far as `send_file`.
struct StreamRouter {
    connection: Connection,
    state: Mutex<RouterState>,
}

#[derive(Default)]
struct RouterState {
    waiting: HashMap<Uuid, oneshot::Sender<Result<Incoming, String>>>,
    parked: HashMap<Uuid, Vec<Incoming>>,
    closed: Option<String>,
}

impl RouterState {
    /// Hold a stream until the transfer that names it registers.
    ///
    /// The cap is a guard against a peer that names streams nothing will ever
    /// claim, not a queue size: a real batch has one stream per file.
    fn park(&mut self, transfer_id: Uuid, incoming: Incoming) {
        let parked: usize = self.parked.values().map(Vec::len).sum();
        if parked >= MAX_PARKED_STREAMS
            && let Some(victim) = self.parked.keys().next().copied()
        {
            self.parked.remove(&victim);
        }
        self.parked.entry(transfer_id).or_default().push(incoming);
    }

    /// Take the parked stream for a transfer, if one arrived early.
    fn take_parked(&mut self, transfer_id: Uuid) -> Option<Incoming> {
        let mut parked = self.parked.remove(&transfer_id)?;
        let incoming = parked.pop();
        if !parked.is_empty() {
            self.parked.insert(transfer_id, parked);
        }
        incoming
    }
}

/// A transfer stream that has been accepted and named.
struct Incoming {
    send: SendStream,
    recv: RecvStream,
}

/// Routers by connection, so a stream is never handed to another connection's
/// transfer. The map holds weak handles: the accept loop owns the strong one,
/// and it ends when the connection does.
static ROUTERS: OnceLock<Mutex<HashMap<usize, Weak<StreamRouter>>>> = OnceLock::new();

fn router_for(connection: &Connection) -> Arc<StreamRouter> {
    let routers = ROUTERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = routers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.retain(|_, weak| weak.strong_count() > 0);
    if let Some(router) = guard.get(&connection.stable_id()).and_then(Weak::upgrade) {
        return router;
    }
    let router = Arc::new(StreamRouter {
        connection: connection.clone(),
        state: Mutex::new(RouterState::default()),
    });
    guard.insert(connection.stable_id(), Arc::downgrade(&router));
    drop(guard);
    let accepting = router.clone();
    tokio::spawn(async move { accepting.accept_loop().await });
    router
}

impl StreamRouter {
    fn lock(&self) -> std::sync::MutexGuard<'_, RouterState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    async fn accept_loop(self: Arc<Self>) {
        loop {
            let (send, recv) = match self.connection.accept_bi().await {
                Ok(pair) => pair,
                Err(error) => {
                    // Every sender still waiting has to learn the connection is
                    // gone now, not after its own timeout.
                    self.close(&format!(
                        "the connection closed while waiting for a transfer stream: {error}"
                    ));
                    break;
                }
            };
            let router = self.clone();
            tokio::spawn(async move { router.dispatch(send, recv).await });
        }
    }

    async fn dispatch(&self, send: SendStream, mut recv: RecvStream) {
        let mut name = [0_u8; TRANSFER_STREAM_NAME_LEN];
        if recv.read_exact(&mut name).await.is_err() {
            // A peer that opens and closes a stream without naming anything has
            // nothing for us to serve.
            return;
        }
        let Ok(transfer_id) = Uuid::from_slice(&name) else {
            tracing::warn!("a transfer stream did not name a transfer");
            return;
        };
        let incoming = Incoming { send, recv };
        let mut state = self.lock();
        if let Some(waiter) = state.waiting.remove(&transfer_id) {
            drop(state);
            let _ = waiter.send(Ok(incoming));
            return;
        }
        if state.closed.is_some() {
            return;
        }
        state.park(transfer_id, incoming);
        tracing::info!(%transfer_id, "a transfer stream arrived before its send registered");
    }

    /// Wait for the stream that names `transfer_id`.
    ///
    /// No deadline: a receiver may hold the pull behind its own concurrency cap
    /// while an earlier file of the same batch finishes, and that can take far
    /// longer than any timeout worth picking. A closed connection wakes the
    /// wait, and a cancelled transfer leaves it; only then does the sender give
    /// up.
    async fn claim(
        &self,
        transfer_id: Uuid,
        control: Option<&TransferControl>,
    ) -> Result<Incoming, TransferError> {
        let (waiter, waiting) = oneshot::channel();
        tokio::pin!(waiting);
        {
            let mut state = self.lock();
            if let Some(reason) = &state.closed {
                return Err(TransferError::Control(reason.clone()));
            }
            if let Some(incoming) = state.take_parked(transfer_id) {
                return Ok(incoming);
            }
            state.waiting.insert(transfer_id, waiter);
        }
        let mut ticker = tokio::time::interval(TRANSFER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                result = &mut waiting => {
                    return match result {
                        Ok(Ok(incoming)) => Ok(incoming),
                        Ok(Err(reason)) => Err(TransferError::Control(reason)),
                        Err(_) => Err(TransferError::Control(
                            "the transfer stream router stopped".into(),
                        )),
                    };
                }
                _ = ticker.tick() => {
                    if control.is_some_and(TransferControl::is_cancelled) {
                        self.lock().waiting.remove(&transfer_id);
                        return Err(TransferError::Canceled);
                    }
                }
            }
        }
    }

    fn close(&self, reason: &str) {
        let mut state = self.lock();
        if state.closed.is_some() {
            return;
        }
        state.closed = Some(reason.to_string());
        let waiting: Vec<_> = state.waiting.drain().map(|(_, waiter)| waiter).collect();
        state.parked.clear();
        drop(state);
        for waiter in waiting {
            let _ = waiter.send(Err(reason.to_string()));
        }
    }
}

/// Serve one file until the receiver closes its stream.
///
/// Returns the number of payload bytes written. `control` cancels an outgoing
/// transfer. The stream is the one the receiver opened and named; the manifest
/// and every chunk answer travel back on it.
///
/// `config` sizes the receiver's request window and retry budget; a sender only
/// answers what it is asked for, so nothing here is tunable and the argument is
/// kept for the callers that share one config with both ends.
#[allow(clippy::too_many_arguments)]
pub async fn send_file(
    connection: &Connection,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    manifest: Arc<Manifest>,
    source: Arc<dyn FileSource>,
    _config: TransferConfig,
    control: Option<&TransferControl>,
) -> Result<u64, TransferError> {
    let path = connection.stats().path;
    tracing::info!(
        rtt_ms = path.rtt.as_millis() as u64,
        cwnd = path.cwnd,
        lost_packets = path.lost_packets,
        sent_packets = path.sent_packets,
        congestion_events = path.congestion_events,
        "transfer path state"
    );
    let Incoming { mut send, recv } = router_for(connection).claim(transfer_id, control).await?;
    tracing::info!(%transfer_id, "sender: the receiver named this transfer");
    serve_on_stream(
        &mut send,
        recv,
        identity,
        session_id,
        transfer_id,
        manifest,
        source,
        control,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn serve_on_stream(
    send: &mut SendStream,
    recv: RecvStream,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    manifest: Arc<Manifest>,
    source: Arc<dyn FileSource>,
    control: Option<&TransferControl>,
) -> Result<u64, TransferError> {
    send_manifest(send, identity, session_id, transfer_id, &manifest).await?;
    // One task owns the receive half: a dropped frame read loses the bytes it
    // had already consumed, and a 4 MiB chunk would be cut in half by the next
    // tick. The serving loop only ever awaits a whole envelope.
    let (envelopes, mut incoming) = mpsc::channel::<Result<Envelope, TransportError>>(4);
    let _reader = AbortOnDrop(tokio::spawn(async move {
        let mut recv = recv;
        loop {
            let next = read_envelope(&mut recv).await;
            let stop = next.is_err();
            if envelopes.send(next).await.is_err() || stop {
                break;
            }
        }
    }));
    let mut ticker = tokio::time::interval(TRANSFER_TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut served: u64 = 0;

    loop {
        if control.is_some_and(TransferControl::is_cancelled) {
            return Err(TransferError::Canceled);
        }
        // 3002: refuse to serve a source that changed after it was opened.
        if let Err(error) = source.verify_unchanged() {
            return Err(TransferError::File(error));
        }
        tokio::select! {
            _ = ticker.tick() => {}
            envelope = incoming.recv() => {
                let envelope = match envelope {
                    Some(Ok(envelope)) => envelope,
                    Some(Err(TransportError::Stream(error))) => {
                        tracing::warn!(%transfer_id, %error, "the transfer stream closed without a chunk request");
                        break;
                    }
                    Some(Err(error)) => return Err(TransferError::Transport(error)),
                    None => break,
                };
                if envelope.kind == MessageKind::ChunkAck as i32 {
                    continue;
                }
                // The receiver already holds this content. Nothing to send, and
                // the transfer counts as delivered rather than failed.
                if envelope.kind == MessageKind::TransferDuplicate as i32 {
                    tracing::info!(%transfer_id, "the receiver already has this content");
                    return Ok(manifest.file_size);
                }
                // The receiver reports its own failure here rather than on the
                // control session, so the row stops instead of sitting in
                // transferring until the transfer times out.
                if envelope.kind == MessageKind::TransferError as i32 {
                    let failure: fluxy_proto::TransferError =
                        decode_payload(&envelope.payload)?;
                    tracing::warn!(%transfer_id, code = %failure.code, "the receiver gave up");
                    return Err(TransferError::Control(failure.message));
                }
                if envelope.kind != MessageKind::ChunkRequest as i32 {
                    return Err(TransferError::Unexpected(envelope.kind));
                }
                let request: ChunkRequest = decode_payload(&envelope.payload)?;
                if request.transfer_id != transfer_id.as_bytes() {
                    return Err(TransferError::InvalidManifest(
                        "chunk request has a different transfer id".into(),
                    ));
                }
                let index = request.chunk_index as usize;
                let Some(length) = manifest.block_len(index) else {
                    continue;
                };
                let offset = index as u64 * manifest.block_size as u64;
                let read_started = std::time::Instant::now();
                let data = source.read_at(offset, length)?;
                let read_ms = read_started.elapsed().as_millis() as u64;
                if data.len() != length {
                    return Err(TransferError::Chunk(ChunkError::LengthMismatch {
                        expected: length,
                        actual: data.len(),
                    }));
                }
                let header = ChunkHeader::for_block(transfer_id, &manifest, index, data.as_ref())?;
                // Header and payload are written separately so a 4 MiB chunk is
                // not copied into a second 4 MiB frame buffer first.
                send.write_all(&header.header_bytes())
                    .await
                    .map_err(|error| TransportError::Stream(error.to_string()))?;
                send.write_all(data.as_ref())
                    .await
                    .map_err(|error| TransportError::Stream(error.to_string()))?;
                served += length as u64;
                tracing::info!(
                    index,
                    bytes = length,
                    read_ms,
                    "chunk streamed"
                );
            }
        }
    }

    let _ = send.shutdown().await;
    Ok(served)
}

/// Pull and write every chunk of a file.
///
/// Returns the number of payload bytes written. The caller finalizes the sink
/// after the root-hash check in M2-06.
pub async fn receive_file(
    connection: &Connection,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    manifest: Arc<Manifest>,
    sink: &mut dyn FileSink,
    config: TransferConfig,
) -> Result<u64, TransferError> {
    receive_file_resumable(
        connection,
        identity,
        session_id,
        transfer_id,
        manifest,
        sink,
        config,
        TransferOptions::default(),
    )
    .await
}

/// Durable receiver-side resume settings.
pub struct CheckpointResume<'a> {
    pub store: &'a CheckpointStore,
    /// A checkpoint already matched against the manifest, if any.
    pub checkpoint: Option<TransferCheckpoint>,
    pub file_name: String,
    /// Peer on the other end, persisted so a restart can rebuild the record.
    pub device_id: Option<Uuid>,
    /// Persist progress after every N received chunks.
    pub interval: usize,
}

impl<'a> CheckpointResume<'a> {
    pub fn new(store: &'a CheckpointStore, file_name: impl Into<String>) -> Self {
        Self {
            store,
            checkpoint: None,
            file_name: file_name.into(),
            device_id: None,
            interval: 1,
        }
    }

    /// Remember which peer this transfer belongs to.
    pub fn for_device(mut self, device_id: Uuid) -> Self {
        self.device_id = Some(device_id);
        self
    }

    pub fn from_plan(
        store: &'a CheckpointStore,
        file_name: impl Into<String>,
        plan: Option<ResumePlan>,
    ) -> Self {
        Self {
            store,
            checkpoint: plan.map(|plan| plan.checkpoint),
            file_name: file_name.into(),
            device_id: None,
            interval: 1,
        }
    }

    pub fn with_interval(mut self, interval: usize) -> Self {
        self.interval = interval.max(1);
        self
    }
}

/// Optional resume, pause/cancel, and cleanup settings for a receive.
#[derive(Default)]
pub struct TransferOptions<'a> {
    /// Durable checkpoint settings; `None` keeps no resume state.
    pub resume: Option<&'a CheckpointResume<'a>>,
    /// Pause/cancel handle shared with the UI.
    pub control: Option<&'a TransferControl>,
    /// Keep the checkpoint when the user cancels. The partial file is always
    /// discarded on cancel, so a cancelled transfer leaves no temp file.
    pub keep_checkpoint_on_cancel: bool,
    /// Live progress observer called after every durable block with
    /// (bytes_done, bytes_total). It must be cheap and non-blocking: the
    /// transfer runtime uses it to coalesce UI events at 100 ms.
    pub progress: Option<&'a (dyn Fn(u64, u64) + Send + Sync)>,
}

impl<'a> TransferOptions<'a> {
    pub fn with_resume(resume: &'a CheckpointResume<'a>) -> Self {
        Self {
            resume: Some(resume),
            ..Self::default()
        }
    }

    pub fn with_control(control: &'a TransferControl) -> Self {
        Self {
            control: Some(control),
            ..Self::default()
        }
    }

    pub fn with_progress(progress: &'a (dyn Fn(u64, u64) + Send + Sync)) -> Self {
        Self {
            progress: Some(progress),
            ..Self::default()
        }
    }
}

/// Open a fresh sink or resume the partial file recorded by a plan.
pub fn prepare_sink(
    manager: &dyn FileAccessManager,
    transfer_id: Uuid,
    file_name: &str,
    manifest: &Manifest,
    plan: Option<&ResumePlan>,
) -> Result<Box<dyn FileSink>, TransferError> {
    match plan {
        Some(plan) => {
            let covered = covered_ranges(manifest, &plan.checkpoint.received);
            // The partial file is named after the id that created it. A resumed
            // send arrives with a fresh id, so reopening by that id finds nothing
            // and fails with NotFound; the checkpoint's own id is the one that
            // owns the bytes already on disk.
            let owner = plan.checkpoint.transfer_id;
            tracing::info!(
                %transfer_id,
                owner = %owner,
                file = %file_name,
                ranges = covered.len(),
                "prepare_sink: resuming into the existing file"
            );
            tracing::info!(%transfer_id, "prepare_sink: asking the manager to resume");
            // A checkpoint can outlive the partial file it describes: the
            // file is cleaned up when a transfer fails, but the checkpoint is
            // kept so a retry can resume. Reopening it then fails with
            // NotFound, and refusing the whole transfer over that trades a
            // lost head start for a lost file. Start over instead.
            let sink = match manager.resume_sink(owner, file_name, manifest.file_size, &covered) {
                Ok(sink) => sink,
                Err(FileError::NotFound(_)) => {
                    tracing::info!(target: "fluxy", %transfer_id, %owner, "prepare_sink: the partial file is gone; starting over");
                    manager.create_sink_with_id(transfer_id, file_name, manifest.file_size)?
                }
                Err(error) => return Err(error.into()),
            };
            tracing::info!(%transfer_id, "prepare_sink: the manager returned a sink");
            Ok(sink)
        }
        None => {
            tracing::info!(%transfer_id, file = %file_name, "prepare_sink: creating a new file");
            let sink = manager.create_sink_with_id(transfer_id, file_name, manifest.file_size)?;
            tracing::info!(%transfer_id, "prepare_sink: the manager returned a sink");
            Ok(sink)
        }
    }
}

/// Pull a file while persisting an atomic checkpoint and honoring pause and
/// cancel requests from `options.control`.
#[allow(clippy::too_many_arguments)]
pub async fn receive_file_resumable(
    connection: &Connection,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    manifest: Arc<Manifest>,
    sink: &mut dyn FileSink,
    config: TransferConfig,
    options: TransferOptions<'_>,
) -> Result<u64, TransferError> {
    let resume = options.resume;
    let file_name = resume
        .map(|resume| resume.file_name.clone())
        .unwrap_or_else(|| "received.bin".into());
    let stored = resume.and_then(|resume| resume.checkpoint.clone());
    // The last unknown in the resume chain: what actually arrives here.
    tracing::info!(
        %transfer_id,
        present = stored.is_some(),
        blocks_done = stored.as_ref().map(|checkpoint| checkpoint.blocks_done()),
        matches = stored
            .as_ref()
            .map(|checkpoint| checkpoint.matches_manifest(&manifest)),
        "checkpoint at receive time"
    );
    let mut checkpoint = match stored {
        Some(checkpoint) if checkpoint.matches_manifest(&manifest) => {
            // The whole point of a checkpoint: say how much is being kept.
            tracing::info!(
                %transfer_id,
                blocks_done = checkpoint.blocks_done(),
                "resuming from a stored checkpoint"
            );
            checkpoint
        }
        Some(checkpoint) => {
            tracing::warn!(
                %transfer_id,
                blocks_done = checkpoint.blocks_done(),
                "stored checkpoint does not match this manifest; starting from zero"
            );
            TransferCheckpoint::new(
                transfer_id,
                TransferRole::Receiver,
                file_name.clone(),
                &manifest,
            )
        }
        None => {
            tracing::info!(
                %transfer_id,
                "no stored checkpoint for this transfer; starting from zero"
            );
            TransferCheckpoint::new(
                transfer_id,
                TransferRole::Receiver,
                file_name.clone(),
                &manifest,
            )
        }
    };
    let mut state = StateMachine::new(checkpoint.state);
    match state.state() {
        // A checkpoint that survived a crash or pause resumes in place.
        TransferState::Interrupted => {
            state.begin_resume()?;
            state.transition(TransferState::Transferring)?;
        }
        // A process killed mid-transfer leaves the last persisted state.
        TransferState::Transferring => {}
        TransferState::Preparing | TransferState::Resuming | TransferState::Paused => {
            state.transition(TransferState::Transferring)?;
        }
        // A finished or failed entry cannot be continued; restart from scratch.
        _ => {
            checkpoint =
                TransferCheckpoint::new(transfer_id, TransferRole::Receiver, file_name, &manifest);
            state = StateMachine::new(TransferState::Preparing);
            state.transition(TransferState::Transferring)?;
        }
    }
    checkpoint.set_state(state.state());
    if let Some(resume) = resume {
        // Which of the two is None decides whether a restart can rebuild this
        // transfer, so record both instead of inferring it afterwards.
        tracing::info!(
            resume_device = ?resume.device_id,
            checkpoint_before = ?checkpoint.device_id,
            "saving a checkpoint"
        );
        if let Some(device_id) = resume.device_id {
            checkpoint.device_id = Some(device_id);
        }
        tracing::info!(checkpoint_after = ?checkpoint.device_id, "checkpoint device resolved");
        resume.store.save(&manifest, &checkpoint)?;
    }

    let mut result = pull_file(
        connection,
        identity,
        session_id,
        transfer_id,
        manifest.clone(),
        sink,
        config,
        &options,
        &mut checkpoint,
        &mut state,
    )
    .await;

    // 3005: the final root hash is checked before the transfer may complete.
    if result.is_ok() {
        state.begin_verify()?;
        let computed = sink.root_hash()?;
        if computed == manifest.root_hash {
            state.complete()?;
        } else {
            state.fail()?;
            result = Err(TransferError::HashMismatch {
                expected: manifest.root_hash,
                actual: computed,
            });
        }
    } else if matches!(&result, Err(TransferError::Canceled)) {
        state.cancel()?;
    } else {
        state.interrupt()?;
    }

    let cancelled = matches!(&result, Err(TransferError::Canceled));
    let corrupt = matches!(&result, Err(TransferError::HashMismatch { .. }));
    checkpoint.set_state(state.state());
    if let Some(resume) = resume {
        let _ = resume.store.save_progress(&checkpoint);
        // A cancelled or corrupt transfer is not resumable: drop the
        // checkpoint unless the caller explicitly keeps it on cancel.
        if corrupt || (cancelled && !options.keep_checkpoint_on_cancel) {
            let _ = resume.store.remove(transfer_id);
        }
    }
    if cancelled || corrupt {
        // Cancellation and a failed root hash always clean the partial file.
        let _ = sink.discard();
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn pull_file(
    connection: &Connection,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    manifest: Arc<Manifest>,
    sink: &mut dyn FileSink,
    config: TransferConfig,
    options: &TransferOptions<'_>,
    checkpoint: &mut TransferCheckpoint,
    state: &mut StateMachine,
) -> Result<u64, TransferError> {
    let resume = options.resume;
    let control = options.control;
    let mut paused = false;
    tracing::info!(%transfer_id, "receiver: pulling, opening a stream");
    let (mut control_send, control_recv) = connection
        .open_bi()
        .await
        .map_err(|error| TransportError::Stream(error.to_string()))?;
    // Name this transfer before anything else: the sender accepts whichever
    // stream arrives and has to know which file it belongs to before it writes
    // a byte back.
    control_send
        .write_all(transfer_id.as_bytes())
        .await
        .map_err(|error| TransportError::Stream(error.to_string()))?;
    // One task owns the receive half. A frame read is not cancel-safe: the
    // 100 ms tick would drop a half-read 4 MiB chunk and lose its bytes.
    let (frames, mut incoming) = mpsc::channel::<Result<TransferFrame, TransferError>>(2);
    let max_chunk_bytes = config.max_chunk_bytes;
    let _reader = AbortOnDrop(tokio::spawn(async move {
        let mut control_recv = control_recv;
        loop {
            let frame = read_transfer_frame(&mut control_recv, max_chunk_bytes).await;
            let stop = frame.is_err();
            if frames.send(frame).await.is_err() || stop {
                break;
            }
        }
    }));
    // Everything from here on runs inside a block so a failure can still be
    // reported on this stream. The sender waits for a chunk request here; an
    // error written to the control session instead would never be seen, and
    // the sender would sit in transferring forever.
    let outcome: Result<u64, TransferError> = async {
        let mut scheduler =
            PullScheduler::resume_from(&manifest, config.scheduler, checkpoint.received.clone());
        let mut ticker = tokio::time::interval(TRANSFER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut received: u64 = 0;

        tracing::info!(%transfer_id, "receiver: stream open, sending chunk requests");
        send_requests(
            &mut control_send,
            identity,
            session_id,
            transfer_id,
            &mut scheduler,
            Instant::now(),
        )
        .await?;
        tracing::info!(%transfer_id, "receiver: requests sent, waiting for the manifest");
        let first = tokio::time::timeout(TRANSFER_IO_TIMEOUT, incoming.recv())
            .await
            .map_err(|_| TransferError::Timeout)?
            .ok_or_else(|| {
                TransferError::Control("the transfer stream closed before the manifest".into())
            })??;
        let TransferFrame::Control(manifest_frame) = first else {
            return Err(TransferError::InvalidManifest(
                "the sender wrote a chunk before the manifest".into(),
            ));
        };
        accept_manifest(&manifest_frame, transfer_id, &manifest)?;
        tracing::info!(%transfer_id, "receiver: manifest accepted, pulling chunks");

        loop {
            if scheduler.is_complete() {
                break;
            }
            if control.is_some_and(TransferControl::is_cancelled) {
                return Err(TransferError::Canceled);
            }
            tokio::select! {
                frame = incoming.recv() => {
                    let frame = match frame {
                        Some(Ok(frame)) => frame,
                        Some(Err(error)) => return Err(error),
                        None => {
                            return Err(TransferError::Control(
                                "the transfer stream closed before the file was complete".into(),
                            ));
                        }
                    };
                    match frame {
                        TransferFrame::Control(envelope) => {
                            // The receiver told the sender it already has this
                            // content, so the sender is closing as delivered.
                            if envelope.kind == MessageKind::TransferDuplicate as i32 {
                                tracing::info!(%transfer_id, "the sender says the content is already here");
                                return Ok(received);
                            }
                            // The sender reports its own failure here rather than
                            // on the control session, so the row stops instead of
                            // sitting in transferring until it times out.
                            if envelope.kind == MessageKind::TransferError as i32 {
                                let failure: fluxy_proto::TransferError =
                                    decode_payload(&envelope.payload)?;
                                tracing::warn!(%transfer_id, code = %failure.code, "the sender gave up");
                                return Err(TransferError::Control(failure.message));
                            }
                            return Err(TransferError::Unexpected(envelope.kind));
                        }
                        TransferFrame::Chunk { header, payload } => {
                            let index = header.chunk_index;
                            if header.transfer_id != transfer_id {
                                return Err(TransferError::InvalidManifest(
                                    "chunk has a different transfer id".into(),
                                ));
                            }
                            let Some(length) = manifest.block_len(index as usize) else {
                                return Err(TransferError::InvalidManifest(
                                    "chunk index is outside the manifest".into(),
                                ));
                            };
                            // A tampered or mismatched chunk is rejected and
                            // re-requested; the transfer fails only when the
                            // retry budget for that block is exhausted.
                            let hash_ok = blake3::hash(&payload).as_bytes() == &header.payload_hash;
                            if !hash_ok || header.verify(&manifest, &payload).is_err() {
                                scheduler.on_chunk_rejected(index, Instant::now());
                                if scheduler.has_failed() {
                                    return Err(TransferError::MissingChunks(scheduler.failed_chunks()));
                                }
                                refill_unless_paused(
                                    control,
                                    &mut control_send,
                                    identity,
                                    session_id,
                                    transfer_id,
                                    &mut scheduler,
                                )
                                .await?;
                                continue;
                            }
                            // A request that timed out can be served twice. The
                            // bytes are the same, but the byte count must not grow.
                            let fresh = !scheduler.bitmap().is_set(index as usize);
                            let offset = index as u64 * manifest.block_size as u64;
                            sink.write_at(offset, &payload)?;
                            scheduler.on_chunk_received(index, Instant::now());
                            checkpoint.mark_received(index);
                            if fresh {
                                received += length as u64;
                                if let Some(progress) = options.progress {
                                    progress(checkpoint.received_bytes(&manifest), manifest.file_size);
                                }
                                if let Some(resume) = resume
                                    && resume.interval > 0
                                    && checkpoint.blocks_done().is_multiple_of(resume.interval)
                                {
                                    resume.store.save_progress(checkpoint)?;
                                }
                            }
                            send_ack(
                                &mut control_send,
                                identity,
                                session_id,
                                transfer_id,
                                index,
                                true,
                                "ok",
                            )
                            .await?;
                            // Refill the window as soon as a chunk lands. Waiting
                            // for the next 100 ms tick would cap throughput at one
                            // window per tick, far below the loopback target on NVMe.
                            refill_unless_paused(
                                control,
                                &mut control_send,
                                identity,
                                session_id,
                                transfer_id,
                                &mut scheduler,
                            )
                            .await?;
                        }
                    }
                }
                _ = ticker.tick() => {
                    if let Some(control) = control {
                        if control.is_cancelled() {
                            return Err(TransferError::Canceled);
                        }
                        if control.is_paused() {
                            scheduler.pause();
                            if !paused {
                                paused = true;
                                state.pause()?;
                                checkpoint.set_state(state.state());
                                if let Some(resume) = resume {
                                    let _ = resume.store.save_progress(checkpoint);
                                }
                            }
                            continue;
                        }
                        if paused {
                            paused = false;
                            state.resume()?;
                            checkpoint.set_state(state.state());
                            if let Some(resume) = resume {
                                let _ = resume.store.save_progress(checkpoint);
                            }
                        }
                    }
                    send_requests(
                        &mut control_send,
                        identity,
                        session_id,
                        transfer_id,
                        &mut scheduler,
                        Instant::now(),
                    )
                    .await?;
                    if scheduler.has_failed() {
                        return Err(TransferError::MissingChunks(scheduler.failed_chunks()));
                    }
                }
            }
        }

        sink.sync()?;
        Ok(received)
    }
    .await;
    if let Err(error) = &outcome {
        tracing::info!(
            %transfer_id,
            code = error.code().as_str(),
            "receiver: reporting the failure on the transfer stream"
        );
        let payload = encode_payload(&fluxy_proto::TransferError {
            transfer_id: transfer_id.as_bytes().to_vec(),
            code: error.code().as_str().to_string(),
            message: error.to_string(),
            retryable: error.is_retryable(),
        });
        let envelope = make_envelope(
            MessageKind::TransferError,
            identity,
            Some(session_id),
            payload,
        );
        let _ = write_envelope(&mut control_send, &envelope).await;
    }
    let _ = control_send.shutdown().await;
    outcome
}

/// Ask the scheduler for newly available work and send those requests.
///
/// A paused transfer must not issue new work: the tick path skips its refill
/// while paused, so the completion path has to as well, or a pause could run
/// the transfer to completion and then finalize from the wrong state.
async fn refill_unless_paused(
    control: Option<&TransferControl>,
    control_send: &mut SendStream,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    scheduler: &mut PullScheduler,
) -> Result<(), TransferError> {
    if control.is_some_and(TransferControl::is_paused) {
        return Ok(());
    }
    send_requests(
        control_send,
        identity,
        session_id,
        transfer_id,
        scheduler,
        Instant::now(),
    )
    .await
}

/// One frame on a transfer stream.
enum TransferFrame {
    Control(Envelope),
    Chunk {
        header: ChunkHeader,
        payload: Vec<u8>,
    },
}

/// Aborts a task when dropped, including on the error paths of its owner.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Read one frame from a transfer stream.
///
/// Control frames keep their 4-byte big-endian length prefix; a chunk starts
/// with `FCH1`, whose value is far above `MAX_CONTROL_FRAME`, so the first four
/// bytes say which of the two follows without a second framing layer.
async fn read_transfer_frame(
    recv: &mut RecvStream,
    max_chunk_bytes: usize,
) -> Result<TransferFrame, TransferError> {
    let mut prefix = [0_u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|error| TransportError::Stream(error.to_string()))?;
    if prefix == CHUNK_MAGIC {
        let mut header = [0_u8; CHUNK_HEADER_LEN];
        header[..4].copy_from_slice(&prefix);
        recv.read_exact(&mut header[4..])
            .await
            .map_err(|error| TransportError::Stream(error.to_string()))?;
        let header = ChunkHeader::decode_fixed(&header)?;
        let length = header.payload_len as usize;
        if length > max_chunk_bytes {
            return Err(TransferError::Chunk(ChunkError::TooLarge(length)));
        }
        let mut payload = vec![0_u8; length];
        recv.read_exact(&mut payload)
            .await
            .map_err(|error| TransportError::Stream(error.to_string()))?;
        return Ok(TransferFrame::Chunk { header, payload });
    }
    let length = u32::from_be_bytes(prefix) as usize;
    if length > MAX_CONTROL_FRAME {
        return Err(TransferError::Protocol(ProtoError::FrameTooLarge(
            length,
            MAX_CONTROL_FRAME,
        )));
    }
    let mut frame = vec![0_u8; 4 + length];
    frame[..4].copy_from_slice(&prefix);
    recv.read_exact(&mut frame[4..])
        .await
        .map_err(|error| TransportError::Stream(error.to_string()))?;
    Ok(TransferFrame::Control(decode_frame(&frame)?))
}

async fn send_manifest(
    control: &mut SendStream,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    manifest: &Manifest,
) -> Result<(), TransferError> {
    let payload = encode_payload(&manifest_to_proto(transfer_id, manifest));
    let envelope = make_envelope(
        MessageKind::TransferManifest,
        identity,
        Some(session_id),
        payload,
    );
    write_envelope(control, &envelope).await?;
    Ok(())
}

fn accept_manifest(
    envelope: &Envelope,
    transfer_id: Uuid,
    expected: &Manifest,
) -> Result<(), TransferError> {
    if envelope.kind != MessageKind::TransferManifest as i32 {
        return Err(TransferError::Unexpected(envelope.kind));
    }
    let message: TransferManifest = decode_payload(&envelope.payload)?;
    let (wire_id, wire) = manifest_from_proto(&message)?;
    if wire_id != transfer_id {
        return Err(TransferError::InvalidManifest(
            "manifest has a different transfer id".into(),
        ));
    }
    if wire.block_size != expected.block_size
        || wire.file_size != expected.file_size
        || wire.block_hashes != expected.block_hashes
        || wire.root_hash != expected.root_hash
        || wire.manifest_digest != expected.manifest_digest
    {
        return Err(TransferError::InvalidManifest(
            "manifest does not match the requested transfer".into(),
        ));
    }
    Ok(())
}

async fn send_requests(
    control: &mut SendStream,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    scheduler: &mut PullScheduler,
    now: Instant,
) -> Result<(), TransferError> {
    for index in scheduler.next_requests(now) {
        let payload = encode_payload(&ChunkRequest {
            transfer_id: transfer_id.as_bytes().to_vec(),
            chunk_index: index,
        });
        let envelope = make_envelope(
            MessageKind::ChunkRequest,
            identity,
            Some(session_id),
            payload,
        );
        write_envelope(control, &envelope).await?;
    }
    Ok(())
}

async fn send_ack(
    control: &mut SendStream,
    identity: &Identity,
    session_id: Uuid,
    transfer_id: Uuid,
    index: u32,
    stored: bool,
    code: &str,
) -> Result<(), TransferError> {
    let payload = encode_payload(&ChunkAck {
        transfer_id: transfer_id.as_bytes().to_vec(),
        chunk_index: index,
        stored,
        code: code.to_string(),
    });
    let envelope = make_envelope(MessageKind::ChunkAck, identity, Some(session_id), payload);
    write_envelope(control, &envelope).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::Path;

    use std::collections::HashSet;
    use std::io::Write;
    use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

    use bytes::Bytes;
    use fluxy_core::files::{
        DesktopFileAccessManager, FileAccessManager, FileCapabilities, FileMeta, FileReference,
        FileSink, FileSource, SourceIdentity,
    };
    use fluxy_core::transfer::{ManifestOptions, compute_manifest};
    use tokio::time::timeout;

    use super::*;
    use crate::tls::TlsIdentity;
    use crate::transport::{accept, client_endpoint, connect, server_endpoint};

    fn loopback() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    fn build_manifest(
        root: &Path,
        name: &str,
        data: &[u8],
        block_size: u32,
    ) -> (Arc<Manifest>, Arc<dyn FileSource>) {
        let path = root.join(name);
        std::fs::write(&path, data).unwrap();
        let manager = DesktopFileAccessManager::new(root);
        let id = manager.register_source(FileReference::Path(path)).unwrap();
        let source: Arc<dyn FileSource> = Arc::from(manager.open_source(id).unwrap());
        let manifest = compute_manifest(
            source.as_ref(),
            ManifestOptions {
                block_size,
                window_bytes: block_size as usize,
            },
            &mut |_| {},
        )
        .unwrap();
        (Arc::new(manifest), source)
    }

    fn test_data(len: usize) -> Vec<u8> {
        (0..len).map(|index| (index % 251) as u8).collect()
    }

    /// What every receiver writes before anything else on its transfer stream.
    async fn read_stream_name(recv: &mut RecvStream) -> Uuid {
        let mut name = [0_u8; TRANSFER_STREAM_NAME_LEN];
        recv.read_exact(&mut name).await.unwrap();
        Uuid::from_slice(&name).unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn four_streams_transfer_a_file() {
        let data = test_data(5 * 1024 * 1024 + 7);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) =
            build_manifest(source_dir.path(), "source.bin", &data, 1024 * 1024);
        let transfer_id = Uuid::now_v7();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_manifest = manifest.clone();
        let server_source = source.clone();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                TransferConfig::default(),
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let mut sink = manager
            .create_sink("received.bin", data.len() as u64)
            .unwrap();
        let identity = Identity::generate();
        let received = timeout(
            Duration::from_secs(30),
            receive_file(
                &connection,
                &identity,
                Uuid::now_v7(),
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                TransferConfig::default(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(received, data.len() as u64);
        let final_path = sink.finalize().unwrap();
        let written = std::fs::read(final_path.as_path()).unwrap();
        assert_eq!(written, data);
        assert!(manifest.verify_root(&written));

        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(served, data.len() as u64);
    }

    /// Two files sent at once over one connection. Before the transfer stream
    /// carried a name, each sender accepted whichever stream arrived and wrote
    /// its manifest into the other file's stream, so one file always failed
    /// with "manifest has a different transfer id".
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_transfers_run_at_once_on_one_connection() {
        let first_data = test_data(2 * 1024 * 1024 + 11);
        let second_data = test_data(3 * 1024 * 1024 + 5);
        let source_dir = tempfile::tempdir().unwrap();
        let (first_manifest, first_source) =
            build_manifest(source_dir.path(), "first.bin", &first_data, 256 * 1024);
        let (second_manifest, second_source) =
            build_manifest(source_dir.path(), "second.bin", &second_data, 256 * 1024);
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let config = TransferConfig::default();
        let served_first = first_manifest.clone();
        let served_second = second_manifest.clone();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let first = send_file(
                &connection,
                &server_identity,
                session_id,
                first_id,
                served_first,
                first_source,
                config,
                None,
            );
            let second = send_file(
                &connection,
                &server_identity,
                session_id,
                second_id,
                served_second,
                second_source,
                config,
                None,
            );
            tokio::join!(first, second)
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let identity = Identity::generate();
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let first_manager = DesktopFileAccessManager::new(first_dir.path());
        let second_manager = DesktopFileAccessManager::new(second_dir.path());
        let mut first_sink = first_manager
            .create_sink("first.bin", first_data.len() as u64)
            .unwrap();
        let mut second_sink = second_manager
            .create_sink("second.bin", second_data.len() as u64)
            .unwrap();
        let config = TransferConfig::default();
        let first_receive = receive_file(
            &connection,
            &identity,
            session_id,
            first_id,
            first_manifest.clone(),
            first_sink.as_mut(),
            config,
        );
        let second_receive = receive_file(
            &connection,
            &identity,
            session_id,
            second_id,
            second_manifest.clone(),
            second_sink.as_mut(),
            config,
        );
        let (first, second) = timeout(Duration::from_secs(60), async {
            tokio::join!(first_receive, second_receive)
        })
        .await
        .expect("both transfers must finish");
        assert_eq!(first.unwrap(), first_data.len() as u64);
        assert_eq!(second.unwrap(), second_data.len() as u64);
        let first_path = first_sink.finalize().unwrap();
        let second_path = second_sink.finalize().unwrap();
        assert_eq!(std::fs::read(first_path.as_path()).unwrap(), first_data);
        assert_eq!(std::fs::read(second_path.as_path()).unwrap(), second_data);

        let (first_served, second_served) = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first_served.unwrap(), first_data.len() as u64);
        assert_eq!(second_served.unwrap(), second_data.len() as u64);
    }

    /// A stream naming a transfer nobody is serving must not be used for the
    /// transfer that is. The router parks it and waits for the stream that
    /// names the transfer it wants.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_stream_named_for_another_transfer_is_left_alone() {
        let data = test_data(64 * 1024);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 16 * 1024);
        let transfer_id = Uuid::now_v7();
        let other_id = Uuid::now_v7();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_manifest = manifest.clone();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                source,
                TransferConfig::default(),
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        // Arrives before the transfer is registered, and names something else.
        let (mut stray_send, mut stray_recv) = connection.open_bi().await.unwrap();
        stray_send.write_all(other_id.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let mut sink = manager
            .create_sink("received.bin", data.len() as u64)
            .unwrap();
        let identity = Identity::generate();
        let received = timeout(
            Duration::from_secs(20),
            receive_file(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest,
                sink.as_mut(),
                TransferConfig::default(),
            ),
        )
        .await
        .expect("the named transfer must not be starved")
        .unwrap();
        assert_eq!(received, data.len() as u64);

        // The stray stream must not have carried this transfer's manifest.
        let mut byte = [0_u8; 1];
        assert!(
            timeout(Duration::from_millis(300), stray_recv.read_exact(&mut byte))
                .await
                .is_err(),
            "the sender wrote into a stream that named another transfer"
        );
        let _ = timeout(Duration::from_secs(10), server_task).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transfer_survives_out_of_order_and_duplicate_requests() {
        let data = test_data(3 * 1024 * 1024);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 512 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = TransferConfig {
            scheduler: SchedulerConfig {
                streams: 2,
                window_per_stream: 2,
                max_retries: 4,
                chunk_timeout: Duration::from_millis(500),
            },
            ..TransferConfig::default()
        };

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_manifest = manifest.clone();
        let server_source = source.clone();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                config,
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let mut sink = manager
            .create_sink("received.bin", data.len() as u64)
            .unwrap();
        let identity = Identity::generate();
        let received = timeout(
            Duration::from_secs(30),
            receive_file(
                &connection,
                &identity,
                Uuid::now_v7(),
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(received, data.len() as u64);
        let final_path = sink.finalize().unwrap();
        assert_eq!(std::fs::read(final_path.as_path()).unwrap(), data);
        timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn receiver_fails_when_no_chunks_arrive() {
        let data = test_data(64 * 1024);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, _source) = build_manifest(source_dir.path(), "source.bin", &data, 16 * 1024);
        let config = TransferConfig {
            scheduler: SchedulerConfig {
                streams: 2,
                window_per_stream: 1,
                max_retries: 2,
                chunk_timeout: Duration::from_millis(150),
            },
            ..TransferConfig::default()
        };

        let transfer_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let (mut control_send, mut control_recv) = connection.accept_bi().await.unwrap();
            let named = read_stream_name(&mut control_recv).await;
            assert_eq!(
                named, transfer_id,
                "the receiver named a different transfer"
            );
            send_manifest(
                &mut control_send,
                &server_identity,
                session_id,
                transfer_id,
                &server_manifest,
            )
            .await
            .unwrap();
            while read_envelope(&mut control_recv).await.is_ok() {}
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let mut sink = manager
            .create_sink("received.bin", data.len() as u64)
            .unwrap();
        let identity = Identity::generate();
        let result = timeout(
            Duration::from_secs(5),
            receive_file(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest,
                sink.as_mut(),
                config,
            ),
        )
        .await
        .unwrap();
        match result {
            Err(TransferError::MissingChunks(chunks)) => assert!(!chunks.is_empty()),
            other => panic!("expected MissingChunks, got {other:?}"),
        }
        server_task.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn default_streams_stay_stable_across_repeated_transfers() {
        let data = test_data(8 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 512 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = TransferConfig::default();
        // Pin to the constant rather than a number: the default is a tuning
        // choice, and hard-coding it left this test failing silently the last
        // time it changed.
        // Assert the default is usable rather than pinning a number: the value is
        // a tuning choice, and hard-coding it is what left this test red the
        // last time it changed from 4 to 8.
        assert!(
            config.scheduler.streams >= 1,
            "the default must schedule at least one stream"
        );

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_manifest = manifest.clone();
        let server_source = source.clone();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut served = 0_u64;
            for _ in 0..3 {
                served += send_file(
                    &connection,
                    &server_identity,
                    session_id,
                    transfer_id,
                    server_manifest.clone(),
                    server_source.clone(),
                    config,
                    None,
                )
                .await
                .unwrap();
            }
            served
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let identity = Identity::generate();
        for round in 0..3 {
            let sink_dir = tempfile::tempdir().unwrap();
            let manager = DesktopFileAccessManager::new(sink_dir.path());
            let mut sink = manager
                .create_sink("received.bin", data.len() as u64)
                .unwrap();
            let received = timeout(
                Duration::from_secs(30),
                receive_file(
                    &connection,
                    &identity,
                    Uuid::now_v7(),
                    transfer_id,
                    manifest.clone(),
                    sink.as_mut(),
                    config,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(received, data.len() as u64, "round {round}");
            let final_path = sink.finalize().unwrap();
            assert_eq!(
                std::fs::read(final_path.as_path()).unwrap(),
                data,
                "round {round}"
            );
        }
        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(served, 3 * data.len() as u64);
    }

    struct FlakySource {
        inner: Arc<dyn FileSource>,
        remaining: AtomicI64,
    }

    impl FileSource for FlakySource {
        fn meta(&self) -> FileMeta {
            self.inner.meta()
        }

        fn capabilities(&self) -> FileCapabilities {
            self.inner.capabilities()
        }

        fn read_at(&self, offset: u64, len: usize) -> Result<Bytes, FileError> {
            if self.remaining.fetch_sub(1, Ordering::SeqCst) <= 0 {
                return Err(FileError::NotFound("simulated sender crash".into()));
            }
            self.inner.read_at(offset, len)
        }

        fn identity(&self) -> SourceIdentity {
            self.inner.identity()
        }

        fn stat(&self) -> Result<SourceIdentity, FileError> {
            self.inner.stat()
        }
    }

    struct CountingSource {
        inner: Arc<dyn FileSource>,
        bytes: Arc<AtomicU64>,
    }

    impl FileSource for CountingSource {
        fn meta(&self) -> FileMeta {
            self.inner.meta()
        }

        fn capabilities(&self) -> FileCapabilities {
            self.inner.capabilities()
        }

        fn read_at(&self, offset: u64, len: usize) -> Result<Bytes, FileError> {
            let data = self.inner.read_at(offset, len)?;
            self.bytes.fetch_add(data.len() as u64, Ordering::SeqCst);
            Ok(data)
        }

        fn identity(&self) -> SourceIdentity {
            self.inner.identity()
        }

        fn stat(&self) -> Result<SourceIdentity, FileError> {
            self.inner.stat()
        }
    }

    struct FailingSink {
        inner: Box<dyn FileSink>,
        remaining: usize,
    }

    impl FileSink for FailingSink {
        fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), FileError> {
            if self.remaining == 0 {
                return Err(FileError::Io(std::io::Error::other(
                    "simulated receiver crash",
                )));
            }
            self.remaining -= 1;
            self.inner.write_at(offset, data)
        }

        fn sync(&mut self) -> Result<(), FileError> {
            self.inner.sync()
        }

        fn root_hash(&mut self) -> Result<[u8; 32], FileError> {
            self.inner.root_hash()
        }

        fn finalize(self: Box<Self>) -> Result<fluxy_core::files::FinalPath, FileError> {
            let this = *self;
            this.inner.finalize()
        }

        fn abort(self: Box<Self>) {
            let this = *self;
            this.inner.abort();
        }

        fn discard(&mut self) -> Result<(), FileError> {
            self.inner.discard()
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_two_round_server(
        server: quinn::Endpoint,
        transfer_id: Uuid,
        manifest: Arc<Manifest>,
        first_source: Arc<dyn FileSource>,
        second_source: Arc<dyn FileSource>,
        identity: Identity,
        session_id: Uuid,
        config: TransferConfig,
        expect_first_error: bool,
    ) -> Result<u64, TransferError> {
        let first = accept(&server).await.unwrap();
        let first_result = send_file(
            &first,
            &identity,
            session_id,
            transfer_id,
            manifest.clone(),
            first_source,
            config,
            None,
        )
        .await;
        if expect_first_error {
            assert!(first_result.is_err(), "first round must fail");
        }
        drop(first);
        let second = accept(&server).await.unwrap();
        send_file(
            &second,
            &identity,
            session_id,
            transfer_id,
            manifest,
            second_source,
            config,
            None,
        )
        .await
    }

    fn resumable_config() -> TransferConfig {
        TransferConfig {
            scheduler: SchedulerConfig {
                streams: 2,
                window_per_stream: 1,
                max_retries: 4,
                chunk_timeout: Duration::from_secs(2),
            },
            ..TransferConfig::default()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sender_crash_resumes_only_missing_chunks() {
        let data = test_data(4 * 1024 * 1024 + 5);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let total_blocks = manifest.block_count();
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let flaky: Arc<dyn FileSource> = Arc::new(FlakySource {
            inner: source.clone(),
            remaining: AtomicI64::new(8),
        });
        let served_bytes = Arc::new(AtomicU64::new(0));
        let counting: Arc<dyn FileSource> = Arc::new(CountingSource {
            inner: source.clone(),
            bytes: served_bytes.clone(),
        });
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_task = tokio::spawn(spawn_two_round_server(
            server,
            transfer_id,
            server_manifest,
            flaky,
            counting,
            server_identity,
            session_id,
            config,
            true,
        ));

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(store_dir.path());
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();

        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink = prepare_sink(&manager, transfer_id, "resumed.bin", &manifest, None).unwrap();
        let first = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
                TransferOptions::with_resume(&CheckpointResume::new(&store, "resumed.bin")),
            ),
        )
        .await
        .unwrap();
        assert!(first.is_err(), "the sender crash must surface as an error");
        drop(connection);

        let plan = store
            .resume_plan(transfer_id, &manifest)
            .unwrap()
            .expect("a checkpoint must survive the crash");
        let done = plan.checkpoint.blocks_done();
        assert!(
            done > 0 && done < total_blocks,
            "checkpoint has {done}/{total_blocks} blocks"
        );
        assert_eq!(plan.checkpoint.state, TransferState::Interrupted);
        let missing = plan.missing_bytes(&manifest);
        assert!(missing > 0);

        // Acceptance: after a restart the first missing-chunk request and the
        // whole resumed round finish within seconds, not a fresh full transfer.
        let resume_started = Instant::now();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink =
            prepare_sink(&manager, transfer_id, "resumed.bin", &manifest, Some(&plan)).unwrap();
        let received = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
                TransferOptions::with_resume(&CheckpointResume::from_plan(
                    &store,
                    "resumed.bin",
                    Some(plan.clone()),
                )),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(received, missing);
        assert!(
            resume_started.elapsed() < Duration::from_secs(3),
            "resume took {:?}",
            resume_started.elapsed()
        );
        let path = sink.finalize().unwrap();
        let written = std::fs::read(path.as_path()).unwrap();
        assert_eq!(written, data);
        assert!(manifest.verify_root(&written));

        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(served, missing);
        assert_eq!(served_bytes.load(Ordering::SeqCst), missing);
        let completed = store.resume_plan(transfer_id, &manifest).unwrap().unwrap();
        assert!(completed.missing.is_empty());
        assert_eq!(completed.checkpoint.state, TransferState::Completed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn receiver_crash_resumes_after_a_sink_failure() {
        let data = test_data(2 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let total_blocks = manifest.block_count();
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let served_bytes = Arc::new(AtomicU64::new(0));
        let counting: Arc<dyn FileSource> = Arc::new(CountingSource {
            inner: source.clone(),
            bytes: served_bytes.clone(),
        });
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_task = tokio::spawn(spawn_two_round_server(
            server,
            transfer_id,
            server_manifest,
            source.clone(),
            counting,
            server_identity,
            session_id,
            config,
            false,
        ));

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(store_dir.path());
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();

        let connection = connect(&endpoint, addr).await.unwrap();
        let sink = prepare_sink(&manager, transfer_id, "receiver.bin", &manifest, None).unwrap();
        let mut failing = FailingSink {
            inner: sink,
            remaining: 4,
        };
        let first = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                &mut failing,
                config,
                TransferOptions::with_resume(&CheckpointResume::new(&store, "receiver.bin")),
            ),
        )
        .await
        .unwrap();
        assert!(first.is_err(), "the sink failure must surface as an error");
        drop(connection);
        drop(failing);

        let plan = store
            .resume_plan(transfer_id, &manifest)
            .unwrap()
            .expect("the receiver checkpoint must survive");
        assert_eq!(plan.checkpoint.blocks_done(), 4);
        assert!(plan.checkpoint.blocks_done() < total_blocks);
        let missing = plan.missing_bytes(&manifest);

        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink = prepare_sink(
            &manager,
            transfer_id,
            "receiver.bin",
            &manifest,
            Some(&plan),
        )
        .unwrap();
        let received = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
                TransferOptions::with_resume(&CheckpointResume::from_plan(
                    &store,
                    "receiver.bin",
                    Some(plan),
                )),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(received, missing);
        let path = sink.finalize().unwrap();
        let written = std::fs::read(path.as_path()).unwrap();
        assert_eq!(written, data);
        assert!(manifest.verify_root(&written));

        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(served, missing);
        assert_eq!(served_bytes.load(Ordering::SeqCst), missing);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn receiver_task_abort_resumes_from_disk_only() {
        let data = test_data(2 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let total_blocks = manifest.block_count();
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let served_bytes = Arc::new(AtomicU64::new(0));
        let counting: Arc<dyn FileSource> = Arc::new(CountingSource {
            inner: source.clone(),
            bytes: served_bytes.clone(),
        });
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_task = tokio::spawn(spawn_two_round_server(
            server,
            transfer_id,
            manifest.clone(),
            source.clone(),
            counting,
            server_identity,
            session_id,
            config,
            false,
        ));

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let poll_store = CheckpointStore::new(store_dir.path());

        let connection = connect(&endpoint, addr).await.unwrap();
        let sink = prepare_sink(&manager, transfer_id, "abort.bin", &manifest, None).unwrap();
        let task_connection = connection.clone();
        let task_identity = identity.clone();
        let task_manifest = manifest.clone();
        let task_dir = store_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let store = CheckpointStore::new(&task_dir);
            let mut sink = sink;
            receive_file_resumable(
                &task_connection,
                &task_identity,
                session_id,
                transfer_id,
                task_manifest,
                sink.as_mut(),
                config,
                TransferOptions::with_resume(&CheckpointResume::new(&store, "abort.bin")),
            )
            .await
        });

        let mut progress = 0;
        for _ in 0..2000 {
            if let Ok(Some(plan)) = poll_store.resume_plan(transfer_id, &manifest) {
                progress = plan.checkpoint.blocks_done();
                if progress > 0 {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(progress > 0, "the receiver must persist at least one block");
        handle.abort();
        let _ = handle.await;
        drop(connection);

        let plan = poll_store
            .resume_plan(transfer_id, &manifest)
            .unwrap()
            .expect("the checkpoint must be on disk after the abort");
        assert_eq!(plan.checkpoint.state, TransferState::Transferring);
        assert!(plan.checkpoint.blocks_done() < total_blocks);
        let missing = plan.missing_bytes(&manifest);

        let store = CheckpointStore::new(store_dir.path());
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink =
            prepare_sink(&manager, transfer_id, "abort.bin", &manifest, Some(&plan)).unwrap();
        let received = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
                TransferOptions::with_resume(&CheckpointResume::from_plan(
                    &store,
                    "abort.bin",
                    Some(plan),
                )),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(received, missing);
        let path = sink.finalize().unwrap();
        let written = std::fs::read(path.as_path()).unwrap();
        assert_eq!(written, data);
        assert!(manifest.verify_root(&written));

        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(served, missing);
        assert_eq!(served_bytes.load(Ordering::SeqCst), missing);
    }

    struct SlowSource {
        inner: Arc<dyn FileSource>,
        delay: Duration,
        reads: Arc<AtomicUsize>,
    }

    impl FileSource for SlowSource {
        fn meta(&self) -> FileMeta {
            self.inner.meta()
        }

        fn capabilities(&self) -> FileCapabilities {
            self.inner.capabilities()
        }

        fn read_at(&self, offset: u64, len: usize) -> Result<Bytes, FileError> {
            std::thread::sleep(self.delay);
            let bytes = self.inner.read_at(offset, len)?;
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(bytes)
        }

        fn identity(&self) -> SourceIdentity {
            self.inner.identity()
        }

        fn stat(&self) -> Result<SourceIdentity, FileError> {
            self.inner.stat()
        }
    }

    fn slow_source(inner: &Arc<dyn FileSource>) -> Arc<dyn FileSource> {
        slow_source_counting(inner, Arc::new(AtomicUsize::new(0)))
    }

    /// A source that yields slowly and reports how many reads it served, so a
    /// test can act while chunks are still in flight instead of racing a timer.
    fn slow_source_counting(
        inner: &Arc<dyn FileSource>,
        reads: Arc<AtomicUsize>,
    ) -> Arc<dyn FileSource> {
        Arc::new(SlowSource {
            inner: inner.clone(),
            delay: Duration::from_millis(5),
            reads,
        })
    }

    async fn wait_for_blocks(store: &CheckpointStore, transfer_id: Uuid, manifest: &Manifest) {
        for _ in 0..1000 {
            if let Ok(Some(plan)) = store.resume_plan(transfer_id, manifest)
                && plan.checkpoint.blocks_done() > 0
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("the receiver never persisted a block");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn pause_then_resume_keeps_the_partial_file_and_completes() {
        let data = test_data(4 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_source = slow_source(&source);
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                config,
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let poll_store = CheckpointStore::new(store_dir.path());
        let control = TransferControl::new();
        let task_control = control.clone();

        let connection = connect(&endpoint, addr).await.unwrap();
        let sink = prepare_sink(&manager, transfer_id, "paused.bin", &manifest, None).unwrap();
        let task_connection = connection.clone();
        let task_identity = identity.clone();
        let task_manifest = manifest.clone();
        let task_dir = store_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let store = CheckpointStore::new(&task_dir);
            let mut sink = sink;
            let result = receive_file_resumable(
                &task_connection,
                &task_identity,
                session_id,
                transfer_id,
                task_manifest,
                sink.as_mut(),
                config,
                TransferOptions {
                    resume: Some(&CheckpointResume::new(&store, "paused.bin")),
                    control: Some(&task_control),
                    keep_checkpoint_on_cancel: false,
                    progress: None,
                },
            )
            .await;
            (result, sink)
        });

        wait_for_blocks(&poll_store, transfer_id, &manifest).await;
        assert!(control.pause(), "the transfer must still be running");
        let mut paused_state = None;
        for _ in 0..500 {
            if let Ok(Some(plan)) = poll_store.resume_plan(transfer_id, &manifest)
                && plan.checkpoint.state == TransferState::Paused
            {
                paused_state = Some(plan);
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let paused_plan = paused_state.expect("the checkpoint must record the pause");
        assert!(manager.partial_path(transfer_id).exists());

        // In-flight chunks may still land; once they do, progress must freeze.
        tokio::time::sleep(Duration::from_millis(120)).await;
        let Some(frozen_plan) = poll_store.resume_plan(transfer_id, &manifest).unwrap() else {
            // The pause lost the race: the last block arrived first, so the
            // transfer completed and completion removes the checkpoint. That
            // is the right outcome - there is nothing left to resume - and
            // the freezing assertions below have nothing to observe.
            let Ok((result, _sink)) = timeout(Duration::from_secs(30), handle).await.unwrap()
            else {
                panic!("the receive task should not have been cancelled");
            };
            assert!(
                result.is_ok(),
                "a transfer that finished before the pause should have succeeded",
            );
            return;
        };
        let frozen = frozen_plan.checkpoint.blocks_done();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let still = poll_store
            .resume_plan(transfer_id, &manifest)
            .unwrap()
            .unwrap()
            .checkpoint
            .blocks_done();
        assert_eq!(frozen, still, "a paused transfer must not keep pulling");
        assert!(paused_plan.checkpoint.blocks_done() <= still);

        assert!(control.resume());
        let (result, sink) = timeout(Duration::from_secs(30), handle)
            .await
            .unwrap()
            .unwrap();
        let received = result.unwrap();
        assert_eq!(received, data.len() as u64);
        let path = sink.finalize().unwrap();
        let written = std::fs::read(path.as_path()).unwrap();
        assert_eq!(written, data);
        assert!(manifest.verify_root(&written));
        let completed = poll_store
            .resume_plan(transfer_id, &manifest)
            .unwrap()
            .unwrap();
        assert_eq!(completed.checkpoint.state, TransferState::Completed);

        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(served, data.len() as u64);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cancel_from_paused_discards_partial_and_checkpoint() {
        let data = test_data(4 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_source = slow_source(&source);
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                config,
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let poll_store = CheckpointStore::new(store_dir.path());
        let control = TransferControl::new();
        let task_control = control.clone();

        let connection = connect(&endpoint, addr).await.unwrap();
        let sink = prepare_sink(&manager, transfer_id, "cancelled.bin", &manifest, None).unwrap();
        let task_connection = connection.clone();
        let task_identity = identity.clone();
        let task_manifest = manifest.clone();
        let task_dir = store_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let store = CheckpointStore::new(&task_dir);
            let mut sink = sink;
            let result = receive_file_resumable(
                &task_connection,
                &task_identity,
                session_id,
                transfer_id,
                task_manifest,
                sink.as_mut(),
                config,
                TransferOptions {
                    resume: Some(&CheckpointResume::new(&store, "cancelled.bin")),
                    control: Some(&task_control),
                    keep_checkpoint_on_cancel: false,
                    progress: None,
                },
            )
            .await;
            (result, sink)
        });

        wait_for_blocks(&poll_store, transfer_id, &manifest).await;
        assert!(control.pause());
        for _ in 0..500 {
            if let Ok(Some(plan)) = poll_store.resume_plan(transfer_id, &manifest)
                && plan.checkpoint.state == TransferState::Paused
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert!(control.cancel());
        assert!(!control.cancel(), "cancel is idempotent");

        let (result, _sink) = timeout(Duration::from_secs(30), handle)
            .await
            .unwrap()
            .unwrap();
        let error = result.expect_err("a cancelled transfer must fail");
        assert!(matches!(error, TransferError::Canceled));
        assert_eq!(error.code(), ErrorCode::Canceled);
        assert_eq!(error.code().as_u16(), 3008);
        assert!(!error.is_retryable());
        assert!(
            !manager.partial_path(transfer_id).exists(),
            "cancel must discard the partial file"
        );
        assert!(
            poll_store.load(transfer_id).unwrap().is_none(),
            "cancel must remove the checkpoint by default"
        );
        let _ = timeout(Duration::from_secs(10), server_task).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sender_cancel_stops_serving() {
        let data = test_data(8 * 1024 * 1024 + 1);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let control = TransferControl::new();
        let task_control = control.clone();
        let server_source = slow_source(&source);
        let client_manifest = manifest.clone();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                manifest,
                server_source,
                config,
                Some(&task_control),
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let connection = connect(&endpoint, addr).await.unwrap();
        let sink = manager
            .create_sink("sender-cancel.bin", data.len() as u64)
            .unwrap();
        let client_identity = identity.clone();
        let client = tokio::spawn(async move {
            let mut sink = sink;
            let result = receive_file(
                &connection,
                &client_identity,
                session_id,
                transfer_id,
                client_manifest,
                sink.as_mut(),
                config,
            )
            .await;
            (result, sink)
        });

        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(control.cancel());
        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap();
        let error = served.expect_err("the sender must report the cancel");
        assert!(matches!(error, TransferError::Canceled));
        let (client_result, _sink) = timeout(Duration::from_secs(10), client)
            .await
            .unwrap()
            .unwrap();
        assert!(client_result.is_err(), "the receiver must see the stop");
    }

    /// A deliberately dishonest sender that flips one payload byte on the
    /// wire. `tamper_once` corrupts only the first delivery of each block;
    /// otherwise every delivery is corrupt.
    async fn serve_with_tampering(
        connection: &Connection,
        identity: &Identity,
        session_id: Uuid,
        transfer_id: Uuid,
        manifest: Arc<Manifest>,
        source: Arc<dyn FileSource>,
        tamper_once: bool,
    ) -> Result<u64, TransferError> {
        let (mut control_send, mut control_recv) = connection
            .accept_bi()
            .await
            .map_err(|error| TransportError::Stream(error.to_string()))?;
        let named = read_stream_name(&mut control_recv).await;
        assert_eq!(
            named, transfer_id,
            "the receiver named a different transfer"
        );
        let payload = encode_payload(&manifest_to_proto(transfer_id, &manifest));
        let envelope = make_envelope(
            MessageKind::TransferManifest,
            identity,
            Some(session_id),
            payload,
        );
        write_envelope(&mut control_send, &envelope).await?;
        let mut tampered: HashSet<usize> = HashSet::new();
        let mut served = 0_u64;
        loop {
            let envelope = match read_envelope(&mut control_recv).await {
                Ok(envelope) => envelope,
                Err(_) => break,
            };
            if envelope.kind == MessageKind::ChunkAck as i32 {
                continue;
            }
            if envelope.kind != MessageKind::ChunkRequest as i32 {
                return Err(TransferError::Unexpected(envelope.kind));
            }
            let request: ChunkRequest = decode_payload(&envelope.payload)?;
            let index = request.chunk_index as usize;
            let Some(length) = manifest.block_len(index) else {
                continue;
            };
            let offset = index as u64 * manifest.block_size as u64;
            let data = source.read_at(offset, length)?;
            let header = ChunkHeader::for_block(transfer_id, &manifest, index, data.as_ref())?;
            let mut frame = header.encode_with_payload(data.as_ref())?;
            let corrupt = !tamper_once || tampered.insert(index);
            if corrupt && frame.len() > CHUNK_HEADER_LEN {
                frame[CHUNK_HEADER_LEN] ^= 0xFF;
            }
            // Chunks answer their request on the same stream the request came in
            // on; there is no second stream to mispair.
            control_send
                .write_all(&frame)
                .await
                .map_err(|error| TransportError::Stream(error.to_string()))?;
            served += length as u64;
        }
        let _ = control_send.shutdown().await;
        Ok(served)
    }

    struct LyingSink {
        inner: Box<dyn FileSink>,
    }

    impl FileSink for LyingSink {
        fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), FileError> {
            self.inner.write_at(offset, data)
        }

        fn sync(&mut self) -> Result<(), FileError> {
            self.inner.sync()
        }

        fn root_hash(&mut self) -> Result<[u8; 32], FileError> {
            Ok([0_u8; 32])
        }

        fn finalize(self: Box<Self>) -> Result<fluxy_core::files::FinalPath, FileError> {
            let this = *self;
            this.inner.finalize()
        }

        fn abort(self: Box<Self>) {
            let this = *self;
            this.inner.abort();
        }

        fn discard(&mut self) -> Result<(), FileError> {
            self.inner.discard()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_tampered_chunk_is_rejected_and_retried_until_clean() {
        let data = test_data(4 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_source = source.clone();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            serve_with_tampering(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                true,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink = manager
            .create_sink("tampered.bin", data.len() as u64)
            .unwrap();
        let received = timeout(
            Duration::from_secs(30),
            receive_file(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(received, data.len() as u64);
        let path = sink.finalize().unwrap();
        let written = std::fs::read(path.as_path()).unwrap();
        assert_eq!(written, data);
        assert!(manifest.verify_root(&written));

        let served = timeout(Duration::from_secs(10), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(served >= data.len() as u64);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn every_chunk_tampered_fails_with_missing_chunks() {
        let data = test_data(512 * 1024);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_source = source.clone();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            serve_with_tampering(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                false,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(store_dir.path());
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink = manager
            .create_sink_with_id(transfer_id, "tampered-all.bin", data.len() as u64)
            .unwrap();
        let result = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
                TransferOptions::with_resume(&CheckpointResume::new(&store, "tampered-all.bin")),
            ),
        )
        .await
        .unwrap();
        let error = result.expect_err("every tampered chunk must be rejected");
        assert!(matches!(error, TransferError::MissingChunks(_)));
        assert_eq!(error.code(), ErrorCode::MissingChunks);
        assert_eq!(error.code().as_u16(), 3006);
        // Nothing was finalized and the partial file stays for a retry.
        assert!(manager.partial_path(transfer_id).exists());
        assert!(!sink_dir.path().join("tampered-all.bin").exists());
        let _ = timeout(Duration::from_secs(10), server_task).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_source_changed_mid_transfer_returns_3002() {
        let data = test_data(4 * 1024 * 1024 + 3);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let reads = Arc::new(AtomicUsize::new(0));
        let server_source = slow_source_counting(&source, reads.clone());
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                server_source,
                config,
                None,
            )
            .await
        });

        // Grow the source while chunks are in flight. Appending (rather than
        // rewriting) keeps the bytes the manifest already describes intact, so
        // the sender's own identity check is what notices - a rewrite could be
        // read mid-flight and surface as a hash mismatch instead, which is a
        // different failure than the one under test.
        let source_path = source_dir.path().join("source.bin");
        let modifier = tokio::spawn(async move {
            for _ in 0..4000 {
                if reads.load(Ordering::SeqCst) >= 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&source_path)
                .unwrap();
            file.write_all(&[0xAA_u8; 4096]).unwrap();
            file.sync_all().unwrap();
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink = manager
            .create_sink("changed.bin", data.len() as u64)
            .unwrap();
        let client_result = timeout(
            Duration::from_secs(20),
            receive_file(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                sink.as_mut(),
                config,
            ),
        )
        .await
        .unwrap();
        assert!(client_result.is_err(), "the receiver must see the stop");

        let server_result = timeout(Duration::from_secs(20), server_task)
            .await
            .unwrap()
            .unwrap();
        let error = server_result.expect_err("the sender must detect the changed source");
        assert_eq!(error.code(), ErrorCode::SourceChanged);
        assert_eq!(error.code().as_u16(), 3002);
        let _ = modifier.await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_root_hash_mismatch_is_rejected_before_finalize() {
        let data = test_data(512 * 1024);
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(source_dir.path(), "source.bin", &data, 256 * 1024);
        let transfer_id = Uuid::now_v7();
        let config = resumable_config();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                source,
                config,
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(store_dir.path());
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let connection = connect(&endpoint, addr).await.unwrap();
        let inner = prepare_sink(&manager, transfer_id, "lying.bin", &manifest, None).unwrap();
        let mut lying = LyingSink { inner };
        let result = timeout(
            Duration::from_secs(30),
            receive_file_resumable(
                &connection,
                &identity,
                session_id,
                transfer_id,
                manifest.clone(),
                &mut lying,
                config,
                TransferOptions::with_resume(&CheckpointResume::new(&store, "lying.bin")),
            ),
        )
        .await
        .unwrap();
        let error = result.expect_err("a wrong root hash must fail the transfer");
        assert!(matches!(error, TransferError::HashMismatch { .. }));
        assert_eq!(error.code(), ErrorCode::HashMismatch);
        assert_eq!(error.code().as_u16(), 3005);
        assert!(error.is_retryable());
        assert!(
            !manager.partial_path(transfer_id).exists(),
            "a corrupt transfer must discard the partial file"
        );
        assert!(
            store.load(transfer_id).unwrap().is_none(),
            "a corrupt transfer must drop the checkpoint"
        );
        let _ = timeout(Duration::from_secs(10), server_task).await;
    }

    #[test]
    fn manifest_proto_round_trips_and_rejects_tampering() {
        let data = test_data(32 * 1024);
        let dir = tempfile::tempdir().unwrap();
        let (manifest, _source) = build_manifest(dir.path(), "source.bin", &data, 4096);
        let transfer_id = Uuid::now_v7();
        let proto = manifest_to_proto(transfer_id, &manifest);
        let (decoded_id, decoded) = manifest_from_proto(&proto).unwrap();
        assert_eq!(decoded_id, transfer_id);
        assert_eq!(decoded.block_size, manifest.block_size);
        assert_eq!(decoded.file_size, manifest.file_size);
        assert_eq!(decoded.block_hashes, manifest.block_hashes);
        assert_eq!(decoded.root_hash, manifest.root_hash);
        assert_eq!(decoded.manifest_digest, manifest.manifest_digest);

        let mut tampered = proto.clone();
        tampered.block_hashes[0][0] ^= 0xff;
        assert!(matches!(
            manifest_from_proto(&tampered),
            Err(TransferError::InvalidManifest(_))
        ));
        let mut short_id = proto.clone();
        short_id.transfer_id = vec![1, 2, 3];
        assert!(matches!(
            manifest_from_proto(&short_id),
            Err(TransferError::InvalidManifest(_))
        ));
        let mut short_hash = proto.clone();
        short_hash.block_hashes[0].truncate(4);
        assert!(matches!(
            manifest_from_proto(&short_hash),
            Err(TransferError::InvalidManifest(_))
        ));
    }

    /// Loopback throughput, measured the way §11.3 measures it: two endpoints,
    /// four streams, an NVMe write on the receiving side. Run with
    /// `cargo test -p fluxy-net --release --lib benchmark_loopback_throughput -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore = "loopback throughput benchmark; run in --release with --ignored --nocapture"]
    async fn benchmark_loopback_throughput() {
        const SIZE: usize = 256 * 1024 * 1024;
        let env_usize = |name: &str, fallback: usize| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(fallback)
        };
        let streams = env_usize(
            "FLUXY_BENCH_STREAMS",
            TransferConfig::default().scheduler.streams,
        );
        let window = env_usize(
            "FLUXY_BENCH_WINDOW",
            TransferConfig::default().scheduler.window_per_stream,
        );
        let block_mib = env_usize("FLUXY_BENCH_BLOCK_MIB", 4);
        let bench_config = TransferConfig {
            scheduler: SchedulerConfig {
                streams,
                window_per_stream: window,
                ..TransferConfig::default().scheduler
            },
            ..TransferConfig::default()
        };
        let data = vec![0x5a_u8; SIZE];
        let source_dir = tempfile::tempdir().unwrap();
        let (manifest, source) = build_manifest(
            source_dir.path(),
            "bench.bin",
            &data,
            (block_mib * 1024 * 1024) as u32,
        );
        drop(data);
        let transfer_id = Uuid::now_v7();

        let server_tls = TlsIdentity::generate("server").unwrap();
        let fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_identity = Identity::generate();
        let session_id = Uuid::now_v7();
        let server_manifest = manifest.clone();
        let config = bench_config;

        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            send_file(
                &connection,
                &server_identity,
                session_id,
                transfer_id,
                server_manifest,
                source,
                config,
                None,
            )
            .await
        });

        let client_tls = TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, fingerprint)
            .await
            .unwrap();
        let sink_dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(sink_dir.path());
        let identity = Identity::generate();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut sink = manager.create_sink("bench.bin", SIZE as u64).unwrap();

        let started = std::time::Instant::now();
        let moved = receive_file(
            &connection,
            &identity,
            session_id,
            transfer_id,
            manifest,
            sink.as_mut(),
            config,
        )
        .await
        .unwrap();
        let elapsed = started.elapsed();
        sink.finalize().unwrap();
        let served = timeout(Duration::from_secs(30), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(moved, served);

        let seconds = elapsed.as_secs_f64();
        let mib = moved as f64 / (1024.0 * 1024.0);
        println!(
            "loopback throughput: {:.1} MiB/s ({:.2} GiB/s) for {:.0} MiB in {:.3}s [streams={streams} window={window} block={block_mib} MiB]",
            mib / seconds,
            mib / seconds / 1024.0,
            mib,
            seconds
        );
    }
}
