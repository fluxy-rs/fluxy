//! Core domain types for Fluxy. This crate must not depend on Tauri or platform UI.

pub mod error;
pub mod files;
pub mod identity;
pub mod transfer;
pub mod trust;

pub use error::{Error, ErrorCode, Result};
pub use files::{
    DesktopFileAccessManager, DesktopFileSink, DesktopFileSource, FileAccessManager,
    FileCapabilities, FileError, FileHandleId, FileMeta, FileReference, FileSink, FileSource,
    FinalPath, HASH_READ_BYTES, SourceIdentity, hash_reader, validate_file_name,
};
pub use identity::{Identity, IdentityStore, SasInput, compute_sas};
pub use transfer::{
    BitmapError, CHECKPOINT_DIR, CHECKPOINT_FORMAT_VERSION, CHECKPOINT_MAGIC, CHUNK_HEADER_LEN,
    CHUNK_MAGIC, CacheStatus, CachedManifest, CheckpointError, CheckpointStore, ChunkBitmap,
    ChunkError, ChunkHeader, ControlState, DEFAULT_CHUNK_TIMEOUT, DEFAULT_MAX_RETRIES,
    DEFAULT_STREAMS, DEFAULT_WINDOW_PER_STREAM, HashProgress, MAX_CHUNK_BYTES, MAX_PROGRESS_BYTES,
    Manifest, ManifestCache, ManifestError, ManifestOptions, PullScheduler, ResumePlan,
    SchedulerConfig, StateError, StateMachine, TransferCheckpoint, TransferControl,
    TransferProgress, TransferRole, TransferState, compute_manifest, covered_ranges, write_atomic,
};
pub use trust::{ClipboardPolicy, TrustEntry, TrustStore};
