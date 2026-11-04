//! Transfer domain model: manifests, chunk bitmaps, scheduling, checkpoints,
//! lifecycle state, and pause/cancel control.

pub mod batch;
pub mod bitmap;
pub mod checkpoint;
pub mod chunk;
pub mod control;
pub mod manifest;
pub mod scheduler;
pub mod state;

pub use batch::{
    BATCH_DIR, BatchEntry, BatchError, BatchPlan, DEFAULT_ROOT_NAME, MAX_BATCH_ENTRIES,
    MAX_RELATIVE_PATH_BYTES, MAX_SEGMENT_BYTES, sanitize_root_name, validate_relative_path,
};
pub use bitmap::{BitmapError, ChunkBitmap};
pub use checkpoint::{
    CHECKPOINT_DIR, CHECKPOINT_FORMAT_VERSION, CHECKPOINT_MAGIC, CheckpointError, CheckpointStore,
    MAX_PROGRESS_BYTES, ResumePlan, TransferCheckpoint, TransferRole, TransferState,
    covered_ranges, write_atomic,
};
pub use chunk::{CHUNK_HEADER_LEN, CHUNK_MAGIC, ChunkError, ChunkHeader, MAX_CHUNK_BYTES};
pub use control::{ControlState, TransferControl};
pub use manifest::{
    CacheStatus, CachedManifest, HashProgress, Manifest, ManifestCache, ManifestError,
    ManifestOptions, compute_manifest,
};
pub use scheduler::{
    DEFAULT_CHUNK_TIMEOUT, DEFAULT_MAX_RETRIES, DEFAULT_STREAMS, DEFAULT_WINDOW_PER_STREAM,
    PullScheduler, SchedulerConfig, TransferProgress,
};
pub use state::{StateError, StateMachine};
