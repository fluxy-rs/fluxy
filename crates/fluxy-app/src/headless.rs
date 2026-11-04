//! UI-agnostic single-file transfer driver.
//!
//! The desktop app layers its record state machine, event hub, and queue on top
//! of the same primitives. The CLI and the failure-injection suite drive this
//! module directly, so a script can cut the network, change the source under a
//! running transfer, or kill the process and then observe the resume.
//!
//! Only the offer, the decision, and the lifecycle notifications use the
//! control stream; the manifest and every chunk travel on a transfer data
//! stream opened by the receiver.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fluxy_core::files::{
    DesktopFileAccessManager, FileAccessManager, FileError, FileReference, FileSource,
    validate_file_name,
};
use fluxy_core::transfer::{
    CacheStatus, CachedManifest, CheckpointError, CheckpointStore, HashProgress, Manifest,
    ManifestCache, ManifestError, ManifestOptions, ResumePlan,
};
use fluxy_core::{ErrorCode, Identity};
use fluxy_net::{
    CheckpointResume, ControlSender, ControlSession, SessionEvent, TransferConfig, TransferError,
    TransferMessage, TransferOfferDraft, TransferOptions, prepare_sink, receive_file_resumable,
    send_file,
};
use quinn::Connection;
use uuid::Uuid;

use crate::config::AppPaths;
use crate::error::Error;

/// How long an offer waits for the peer's decision before it is abandoned.
pub const OFFER_TIMEOUT: Duration = Duration::from_secs(60);
/// Block size used when the caller does not choose one.
pub const DEFAULT_BLOCK_SIZE: u32 = 4 * 1024 * 1024;

/// Failure of a headless transfer, carrying the protocol code that names it.
#[derive(Debug, thiserror::Error)]
pub enum HeadlessError {
    #[error("the peer rejected the transfer with {code}: {message}")]
    Rejected { code: String, message: String },
    #[error("the peer did not answer the offer within {seconds} seconds")]
    OfferTimeout { seconds: u64 },
    #[error("the control session closed before the peer answered the offer")]
    SessionClosed,
    #[error("the offered manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("the offered file name is invalid: {0}")]
    InvalidName(String),
    #[error(transparent)]
    Transfer(#[from] TransferError),
    #[error(transparent)]
    File(#[from] FileError),
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    App(#[from] Error),
}

impl HeadlessError {
    /// Stable error code from the v1 protocol table.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Rejected { .. } | Self::InvalidManifest(_) | Self::InvalidName(_) => {
                ErrorCode::Transfer
            }
            Self::OfferTimeout { .. } | Self::SessionClosed => ErrorCode::Timeout,
            Self::Transfer(error) => error.code(),
            Self::File(error) => error.code(),
            Self::Checkpoint(error) => error.code(),
            Self::Manifest(error) => error.code(),
            Self::App(error) => error.code,
        }
    }

    pub fn code_name(&self) -> &'static str {
        self.code().as_str()
    }

    pub fn code_number(&self) -> u16 {
        self.code().as_u16()
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transfer(error) => error.is_retryable(),
            Self::File(error) => error.retryable(),
            Self::Manifest(error) => error.retryable(),
            Self::Checkpoint(_) => true,
            Self::OfferTimeout { .. } | Self::SessionClosed => true,
            Self::Rejected { .. } | Self::InvalidManifest(_) | Self::InvalidName(_) => false,
            Self::App(error) => error.retryable,
        }
    }
}

/// One file offered by a peer, as it arrives on the control stream.
#[derive(Debug, Clone)]
pub struct OfferView {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub block_size: u32,
    pub root_hash: [u8; 32],
    pub manifest_digest: [u8; 32],
    pub total_files: u32,
    pub total_bytes: u64,
    pub manifest: Vec<u8>,
}

impl OfferView {
    /// Extract an offer from a session event, if it is one.
    pub fn from_event(event: &SessionEvent) -> Option<Self> {
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
                ..
            } => Some(Self {
                transfer_id: *transfer_id,
                file_name: file_name.clone(),
                file_size: *file_size,
                block_size: *block_size,
                root_hash: *root_hash,
                manifest_digest: *manifest_digest,
                total_files: *total_files,
                total_bytes: *total_bytes,
                manifest: manifest.clone(),
            }),
            _ => None,
        }
    }

    /// Validate the manifest bytes against the summary that carried them.
    pub fn manifest(&self) -> Result<Manifest, HeadlessError> {
        let manifest = Manifest::from_bytes(&self.manifest)
            .map_err(|error| HeadlessError::InvalidManifest(error.to_string()))?;
        if manifest.file_size != self.file_size
            || manifest.block_size != self.block_size
            || manifest.root_hash != self.root_hash
            || manifest.manifest_digest != self.manifest_digest
        {
            return Err(HeadlessError::InvalidManifest(
                "the offered manifest does not match its summary".into(),
            ));
        }
        Ok(manifest)
    }
}

/// One send request.
#[derive(Debug, Clone)]
pub struct SendRequest {
    pub transfer_id: Uuid,
    pub path: PathBuf,
    pub block_size: u32,
    /// Wire name; defaults to the file name of the source path.
    pub file_name: Option<String>,
}

impl SendRequest {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            transfer_id: Uuid::now_v7(),
            path: path.into(),
            block_size: DEFAULT_BLOCK_SIZE,
            file_name: None,
        }
    }

    /// Reuse a transfer id so a retry resumes the peer's checkpoint.
    pub fn with_transfer_id(mut self, transfer_id: Uuid) -> Self {
        self.transfer_id = transfer_id;
        self
    }

    pub fn with_block_size(mut self, block_size: u32) -> Self {
        self.block_size = block_size;
        self
    }

    pub fn with_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    /// The name that crosses the wire.
    pub fn wire_name(&self) -> Result<String, HeadlessError> {
        let name = match &self.file_name {
            Some(name) => name.clone(),
            None => self
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    HeadlessError::InvalidName("the source path has no file name".into())
                })?,
        };
        validate_file_name(&name).map_err(|error| HeadlessError::InvalidName(error.to_string()))
    }
}

/// What a completed send produced.
#[derive(Debug, Clone)]
pub struct SendOutcome {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub block_size: u32,
    pub root_hash: String,
    pub manifest_digest: String,
    pub bytes_served: u64,
    /// True when the manifest cache answered instead of hashing again.
    pub manifest_reused: bool,
}

/// An offered, accepted send that is ready to serve chunks.
pub struct PreparedSend {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub manifest: Arc<Manifest>,
    /// True when the manifest cache answered instead of hashing again.
    pub manifest_reused: bool,
    source: Arc<dyn FileSource>,
}

/// What a completed receive produced.
#[derive(Debug, Clone)]
pub struct ReceiveOutcome {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub root_hash: String,
    pub output_path: PathBuf,
    /// Payload bytes pulled in this run.
    pub bytes_written: u64,
    /// Payload bytes still missing when the run started.
    pub missing_bytes_before: u64,
    /// True when a checkpoint from an earlier run was resumed.
    pub resumed: bool,
}

/// Durable single-file transfer driver: manifest cache, checkpoints, and the
/// receiver root all live under one data directory.
pub struct HeadlessTransfer {
    manager: DesktopFileAccessManager,
    manifests: ManifestCache,
    checkpoints: CheckpointStore,
    config: TransferConfig,
}

impl HeadlessTransfer {
    /// Create a driver rooted at a data directory, mirroring the app layout.
    pub fn new(data_dir: impl Into<PathBuf>, config: TransferConfig) -> Result<Self, Error> {
        let root = data_dir.into();
        let received = root.join("received");
        let checkpoints = root.join("checkpoints");
        let manifests = root.join("manifests");
        for dir in [&received, &checkpoints, &manifests] {
            std::fs::create_dir_all(dir).map_err(|error| {
                Error::new(
                    ErrorCode::Internal,
                    format!("cannot create {}: {error}", dir.display()),
                    false,
                )
            })?;
        }
        Ok(Self {
            manager: DesktopFileAccessManager::new(received),
            manifests: ManifestCache::new(manifests),
            checkpoints: CheckpointStore::new(checkpoints),
            config,
        })
    }

    /// Create a driver for an application data directory.
    pub fn from_paths(paths: &AppPaths, config: TransferConfig) -> Result<Self, Error> {
        Self::new(paths.data_dir.clone(), config)
    }

    /// Replace the transfer tuning (streams, window, chunk ceiling).
    pub fn with_config(mut self, config: TransferConfig) -> Self {
        self.config = config;
        self
    }

    pub fn received_dir(&self) -> &Path {
        self.manager.root()
    }

    pub fn checkpoints(&self) -> &CheckpointStore {
        &self.checkpoints
    }

    pub fn config(&self) -> TransferConfig {
        self.config
    }

    /// Path of the partial file a receiver keeps for one transfer.
    pub fn partial_path(&self, transfer_id: Uuid) -> PathBuf {
        self.manager.partial_path(transfer_id)
    }

    /// The resume plan a fresh receiver would use for one transfer.
    pub fn resume_plan(
        &self,
        transfer_id: Uuid,
        manifest: &Manifest,
    ) -> Result<Option<ResumePlan>, CheckpointError> {
        self.checkpoints.resume_plan(transfer_id, manifest)
    }

    /// Build or reuse the manifest for a local file.
    pub fn manifest_for(
        &self,
        path: &Path,
        block_size: u32,
    ) -> Result<CachedManifest, HeadlessError> {
        self.manifest_with_progress(path, block_size, &mut |_| {})
    }

    /// Build or reuse the manifest, reporting hashing progress.
    pub fn manifest_with_progress(
        &self,
        path: &Path,
        block_size: u32,
        progress: &mut dyn FnMut(HashProgress),
    ) -> Result<CachedManifest, HeadlessError> {
        let handle = self
            .manager
            .register_source(FileReference::Path(path.to_path_buf()))?;
        let source = self.manager.open_source(handle)?;
        let key = ManifestCache::key_for(path, block_size);
        let options = ManifestOptions {
            block_size,
            ..ManifestOptions::default()
        };
        Ok(self
            .manifests
            .get_or_compute(&key, source.as_ref(), options, progress)?)
    }

    /// Hash (or reuse) the manifest, send the offer, and wait for the
    /// receiver's decision. Events that arrive meanwhile are handed to the
    /// on_event callback, so a caller can keep serving chat traffic.
    pub async fn offer(
        &self,
        control: &mut ControlSession,
        request: &SendRequest,
        on_event: &mut (dyn FnMut(SessionEvent) + Send),
    ) -> Result<PreparedSend, HeadlessError> {
        let file_name = request.wire_name()?;
        let cached = self.manifest_for(&request.path, request.block_size)?;
        let manifest = Arc::new(cached.manifest);
        let handle = self
            .manager
            .register_source(FileReference::Path(request.path.clone()))?;
        let source: Arc<dyn FileSource> = Arc::from(self.manager.open_source(handle)?);

        let offer = TransferOfferDraft::file(
            request.transfer_id,
            file_name.clone(),
            manifest.file_size,
            manifest.block_size,
            manifest.root_hash,
            manifest.manifest_digest,
            manifest.to_bytes(),
        );
        control
            .sender()
            .send_transfer(TransferMessage::Offer(Box::new(offer)))
            .await
            .map_err(|error| TransferError::Control(error.to_string()))?;
        self.await_decision(control, request.transfer_id, on_event)
            .await?;
        Ok(PreparedSend {
            transfer_id: request.transfer_id,
            file_name,
            manifest,
            manifest_reused: cached.status == CacheStatus::Hit,
            source,
        })
    }

    /// Serve the chunks of an accepted offer.
    ///
    /// This borrows no session state, so a caller can keep draining control
    /// events (chat, progress) on another task while the file moves.
    pub async fn serve(
        &self,
        connection: &Connection,
        sender: &ControlSender,
        session_id: Uuid,
        identity: &Identity,
        prepared: PreparedSend,
    ) -> Result<SendOutcome, HeadlessError> {
        let bytes_served = send_file(
            connection,
            identity,
            session_id,
            prepared.transfer_id,
            prepared.manifest.clone(),
            prepared.source,
            self.config,
            None,
        )
        .await?;
        let _ = sender
            .send_transfer(TransferMessage::Complete {
                transfer_id: prepared.transfer_id,
            })
            .await;
        Ok(SendOutcome {
            transfer_id: prepared.transfer_id,
            file_name: prepared.file_name,
            file_size: prepared.manifest.file_size,
            block_size: prepared.manifest.block_size,
            root_hash: prepared.manifest.root_hash_hex(),
            manifest_digest: prepared.manifest.manifest_digest_hex(),
            bytes_served,
            manifest_reused: prepared.manifest_reused,
        })
    }

    /// Offer one file and serve it in one call.
    pub async fn send(
        &self,
        control: &mut ControlSession,
        identity: &Identity,
        request: SendRequest,
        on_event: &mut (dyn FnMut(SessionEvent) + Send),
    ) -> Result<SendOutcome, HeadlessError> {
        let prepared = self.offer(control, &request, on_event).await?;
        let connection = control.connection();
        let sender = control.sender();
        let session_id = control.session_id();
        self.serve(&connection, &sender, session_id, identity, prepared)
            .await
    }

    /// Accept one offer and pull the file into the receiver root.
    ///
    /// A partial file and its checkpoint survive a failed pull, so a retry with
    /// the same transfer id continues from the missing blocks.
    pub async fn receive(
        &self,
        connection: &Connection,
        sender: &ControlSender,
        session_id: Uuid,
        identity: &Identity,
        offer: OfferView,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    ) -> Result<ReceiveOutcome, HeadlessError> {
        let manifest = match offer.manifest() {
            Ok(manifest) => manifest,
            Err(error) => {
                self.reject(
                    sender,
                    offer.transfer_id,
                    error.code_name(),
                    &error.to_string(),
                )
                .await;
                return Err(error);
            }
        };
        let file_name = match validate_file_name(&offer.file_name) {
            Ok(name) => name,
            Err(error) => {
                self.reject(
                    sender,
                    offer.transfer_id,
                    ErrorCode::FileAccess.as_str(),
                    &error.to_string(),
                )
                .await;
                return Err(HeadlessError::InvalidName(error.to_string()));
            }
        };
        let manifest = Arc::new(manifest);
        sender
            .send_transfer(TransferMessage::Accept {
                transfer_id: offer.transfer_id,
            })
            .await
            .map_err(|error| TransferError::Control(error.to_string()))?;

        let plan = self.checkpoints.resume_plan(offer.transfer_id, &manifest)?;
        let missing_bytes_before = plan
            .as_ref()
            .map(|plan| plan.missing_bytes(&manifest))
            .unwrap_or(manifest.file_size);
        let resumed = plan.is_some();
        let mut sink = prepare_sink(
            &self.manager,
            offer.transfer_id,
            &file_name,
            &manifest,
            plan.as_ref(),
        )?;
        let resume = CheckpointResume::from_plan(&self.checkpoints, file_name.clone(), plan);
        let options = TransferOptions {
            resume: Some(&resume),
            control: None,
            keep_checkpoint_on_cancel: false,
            progress,
        };
        let result = receive_file_resumable(
            connection,
            identity,
            session_id,
            offer.transfer_id,
            manifest.clone(),
            sink.as_mut(),
            self.config,
            options,
        )
        .await;
        match result {
            Ok(bytes_written) => {
                let output = sink.finalize()?;
                let _ = sender
                    .send_transfer(TransferMessage::Complete {
                        transfer_id: offer.transfer_id,
                    })
                    .await;
                Ok(ReceiveOutcome {
                    transfer_id: offer.transfer_id,
                    file_name,
                    file_size: manifest.file_size,
                    root_hash: manifest.root_hash_hex(),
                    output_path: output.into_path_buf(),
                    bytes_written,
                    missing_bytes_before,
                    resumed,
                })
            }
            Err(error) => {
                let _ = sender
                    .send_transfer(TransferMessage::Error {
                        transfer_id: offer.transfer_id,
                        code: error.code().as_str().into(),
                        message: error.to_string(),
                        retryable: error.is_retryable(),
                    })
                    .await;
                Err(HeadlessError::Transfer(error))
            }
        }
    }

    /// Tell the peer why an offer was refused.
    async fn reject(&self, sender: &ControlSender, transfer_id: Uuid, code: &str, message: &str) {
        let _ = sender
            .send_transfer(TransferMessage::Reject {
                transfer_id,
                code: code.to_string(),
                message: message.to_string(),
            })
            .await;
    }

    /// Wait for the receiver's decision, forwarding unrelated events.
    async fn await_decision(
        &self,
        control: &mut ControlSession,
        transfer_id: Uuid,
        on_event: &mut (dyn FnMut(SessionEvent) + Send),
    ) -> Result<(), HeadlessError> {
        let deadline = Instant::now() + OFFER_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(HeadlessError::OfferTimeout {
                    seconds: OFFER_TIMEOUT.as_secs(),
                });
            }
            let event = match tokio::time::timeout(remaining, control.next_event()).await {
                Ok(Some(event)) => event,
                Ok(None) => return Err(HeadlessError::SessionClosed),
                Err(_) => {
                    return Err(HeadlessError::OfferTimeout {
                        seconds: OFFER_TIMEOUT.as_secs(),
                    });
                }
            };
            match event {
                SessionEvent::TransferAccept {
                    transfer_id: accepted,
                } if accepted == transfer_id => return Ok(()),
                SessionEvent::TransferReject {
                    transfer_id: rejected,
                    code,
                    message,
                } if rejected == transfer_id => {
                    return Err(HeadlessError::Rejected { code, message });
                }
                SessionEvent::Disconnected { .. } => return Err(HeadlessError::SessionClosed),
                other => on_event(other),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxy_core::transfer::compute_manifest;

    fn driver() -> (HeadlessTransfer, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let driver = HeadlessTransfer::new(dir.path(), TransferConfig::default()).unwrap();
        (driver, dir)
    }

    #[test]
    fn the_driver_creates_its_three_directories() {
        let (driver, dir) = driver();
        for name in ["received", "checkpoints", "manifests"] {
            assert!(dir.path().join(name).is_dir(), "{name} was not created");
        }
        assert_eq!(driver.received_dir(), dir.path().join("received"));
    }

    #[test]
    fn a_send_request_validates_its_wire_name() {
        let valid = SendRequest::new("C:/tmp/report.bin");
        assert_eq!(valid.wire_name().unwrap(), "report.bin");

        let renamed = SendRequest::new("C:/tmp/report.bin").with_file_name("renamed.bin");
        assert_eq!(renamed.wire_name().unwrap(), "renamed.bin");

        let traversal = SendRequest::new("C:/tmp/report.bin").with_file_name("../escape.bin");
        assert!(matches!(
            traversal.wire_name(),
            Err(HeadlessError::InvalidName(_))
        ));
    }

    #[test]
    fn an_offer_view_rejects_a_mismatched_manifest() {
        let (driver, dir) = driver();
        let source = dir.path().join("source.bin");
        std::fs::write(&source, vec![7_u8; 4096]).unwrap();
        let manifest = driver
            .manifest_for(&source, DEFAULT_BLOCK_SIZE)
            .unwrap()
            .manifest;

        let mut view = OfferView {
            transfer_id: Uuid::now_v7(),
            file_name: "source.bin".into(),
            file_size: manifest.file_size,
            block_size: manifest.block_size,
            root_hash: manifest.root_hash,
            manifest_digest: manifest.manifest_digest,
            total_files: 1,
            total_bytes: manifest.file_size,
            manifest: manifest.to_bytes(),
        };
        assert_eq!(view.manifest().unwrap().root_hash, manifest.root_hash);

        view.file_size += 1;
        assert!(matches!(
            view.manifest(),
            Err(HeadlessError::InvalidManifest(_))
        ));

        view.file_size = manifest.file_size;
        view.manifest = vec![0_u8; 8];
        assert!(matches!(
            view.manifest(),
            Err(HeadlessError::InvalidManifest(_))
        ));
    }

    #[test]
    fn a_second_manifest_pass_is_a_cache_hit() {
        let (driver, dir) = driver();
        let source = dir.path().join("cached.bin");
        std::fs::write(&source, vec![1_u8; 8192]).unwrap();

        let first = driver.manifest_for(&source, DEFAULT_BLOCK_SIZE).unwrap();
        assert_eq!(first.status, CacheStatus::Miss);
        let second = driver.manifest_for(&source, DEFAULT_BLOCK_SIZE).unwrap();
        assert_eq!(second.status, CacheStatus::Hit);
        assert_eq!(
            second.manifest.manifest_digest,
            first.manifest.manifest_digest
        );

        // A changed source invalidates the cached entry.
        std::fs::write(&source, vec![2_u8; 8192]).unwrap();
        let third = driver.manifest_for(&source, DEFAULT_BLOCK_SIZE).unwrap();
        assert_eq!(third.status, CacheStatus::Miss);
        assert_ne!(third.manifest.root_hash, first.manifest.root_hash);

        // The cache stores exactly what compute_manifest produces.
        let handle = driver
            .manager
            .register_source(FileReference::Path(source.clone()))
            .unwrap();
        let opened = driver.manager.open_source(handle).unwrap();
        let direct = compute_manifest(
            opened.as_ref(),
            ManifestOptions {
                block_size: DEFAULT_BLOCK_SIZE,
                ..ManifestOptions::default()
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(direct.root_hash, third.manifest.root_hash);
    }

    #[test]
    fn error_codes_and_retryability_follow_the_protocol_table() {
        let rejected = HeadlessError::Rejected {
            code: "peer_busy".into(),
            message: "busy".into(),
        };
        assert_eq!(rejected.code_name(), "transfer");
        assert!(!rejected.is_retryable());

        let timeout = HeadlessError::OfferTimeout { seconds: 60 };
        assert_eq!(timeout.code_name(), "timeout");
        assert_eq!(timeout.code_number(), 2001);
        assert!(timeout.is_retryable());

        let changed = HeadlessError::Transfer(TransferError::File(FileError::SourceChanged));
        assert_eq!(changed.code_name(), "source_changed");
        assert_eq!(changed.code_number(), 3002);
        assert!(!changed.is_retryable());
    }
}
