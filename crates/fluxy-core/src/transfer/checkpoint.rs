//! Atomic transfer checkpoints for cross-restart resume.
//!
//! On-disk layout from the design contract:
//!
//! ```text
//! <root>/checkpoints/<transfer-id>/manifest.bin
//! <root>/checkpoints/<transfer-id>/progress.bin
//! ```
//!
//! Both files are written with the same atomic replace helper: a temporary
//! sibling is flushed and fsynced, the previous entry is removed, and the
//! temporary is renamed into place. `progress.bin` is framed with a BLAKE3
//! checksum, so a torn or truncated write is detected instead of resumed.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorCode};
use crate::files::SourceIdentity;

use super::bitmap::{BitmapError, ChunkBitmap};
use super::manifest::{Manifest, ManifestError};
pub use super::state::TransferState;
use super::state::{StateError, StateMachine};

/// Progress-file magic.
pub const CHECKPOINT_MAGIC: [u8; 4] = *b"FCP1";
/// Checkpoint schema version written by this build.
pub const CHECKPOINT_FORMAT_VERSION: u32 = 1;
/// Directory under the app data root.
pub const CHECKPOINT_DIR: &str = "checkpoints";
/// Manifest file name inside one transfer directory.
pub const MANIFEST_FILE: &str = "manifest.bin";
/// Progress file name inside one transfer directory.
pub const PROGRESS_FILE: &str = "progress.bin";
/// Refuse progress files larger than this; the bitmap for 32768 blocks is small.
pub const MAX_PROGRESS_BYTES: usize = 4 * 1024 * 1024;

/// Which side of the transfer owns this checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferRole {
    Sender,
    Receiver,
}

/// Resume metadata persisted for one transfer and one role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferCheckpoint {
    pub schema: u32,
    pub transfer_id: Uuid,
    pub role: TransferRole,
    pub file_name: String,
    pub state: TransferState,
    /// Sender-side snapshot used to detect a changed source file.
    pub source_identity: Option<SourceIdentity>,
    pub updated_unix_ms: u64,
    /// Received-block bitmap; senders keep an empty bitmap.
    pub received: ChunkBitmap,
    /// Peer this transfer belongs to, so a restart can rebuild the record and
    /// offer a retry. Absent on checkpoints written before the field existed.
    #[serde(default)]
    pub device_id: Option<Uuid>,
    /// Set for a sub-transfer of a folder batch.
    #[serde(default)]
    pub batch_id: Option<Uuid>,
    /// Display name of the batch root, so a resumed receiver can rebuild the
    /// same directory instead of inventing a new one.
    #[serde(default)]
    pub root_name: Option<String>,
    /// This file's path inside the batch root, "/"-separated.
    #[serde(default)]
    pub relative_path: Option<String>,
}

impl TransferCheckpoint {
    pub fn new(
        transfer_id: Uuid,
        role: TransferRole,
        file_name: impl Into<String>,
        manifest: &Manifest,
    ) -> Self {
        Self {
            schema: CHECKPOINT_FORMAT_VERSION,
            transfer_id,
            role,
            file_name: file_name.into(),
            state: TransferState::Preparing,
            source_identity: None,
            updated_unix_ms: now_unix_ms(),
            received: ChunkBitmap::new(manifest.block_count()),
            device_id: None,
            batch_id: None,
            root_name: None,
            relative_path: None,
        }
    }

    /// Record which peer this transfer belongs to.
    pub fn for_device(mut self, device_id: Uuid) -> Self {
        self.device_id = Some(device_id);
        self
    }

    /// Attach this checkpoint to one file of a folder batch.
    pub fn in_batch(mut self, batch_id: Uuid, root_name: &str, relative_path: &str) -> Self {
        self.batch_id = Some(batch_id);
        self.root_name = Some(root_name.to_string());
        self.relative_path = Some(relative_path.to_string());
        self
    }

    /// A resumed sub-transfer must belong to the same batch, root, and relative
    /// path, or it would land somewhere the user never approved.
    pub fn matches_batch(&self, batch_id: Option<Uuid>, relative_path: Option<&str>) -> bool {
        self.batch_id == batch_id && self.relative_path.as_deref() == relative_path
    }

    pub fn is_batched(&self) -> bool {
        self.batch_id.is_some()
    }

    pub fn with_source_identity(mut self, identity: SourceIdentity) -> Self {
        self.source_identity = Some(identity);
        self
    }

    pub fn set_state(&mut self, state: TransferState) {
        self.state = state;
        self.updated_unix_ms = now_unix_ms();
    }

    /// Validate a transition before persisting it. Loading a checkpoint does
    /// not validate, so a stale file can still be inspected and discarded.
    pub fn try_set_state(&mut self, next: TransferState) -> Result<(), StateError> {
        StateMachine::new(self.state).transition(next)?;
        self.set_state(next);
        Ok(())
    }

    pub fn mark_received(&mut self, index: u32) -> bool {
        let changed = self.received.set(index as usize);
        if changed {
            self.updated_unix_ms = now_unix_ms();
        }
        changed
    }

    pub fn blocks_done(&self) -> usize {
        self.received.count_set()
    }

    pub fn is_complete(&self, manifest: &Manifest) -> bool {
        self.received.len() == manifest.block_count() && self.received.is_complete()
    }

    /// Chunks still missing for this manifest, in index order.
    pub fn missing(&self, manifest: &Manifest) -> Vec<u32> {
        (0..manifest.block_count())
            .filter(|index| !self.received.is_set(*index))
            .map(|index| index as u32)
            .collect()
    }

    /// Bytes already covered by the bitmap.
    pub fn received_bytes(&self, manifest: &Manifest) -> u64 {
        (0..self.received.len())
            .filter(|index| self.received.is_set(*index))
            .filter_map(|index| manifest.block_len(index))
            .map(|length| length as u64)
            .sum()
    }

    /// A checkpoint only applies to a manifest with the same block count.
    pub fn matches_manifest(&self, manifest: &Manifest) -> bool {
        self.received.len() == manifest.block_count()
    }

    /// Sender-side guard: the source must still match the recorded snapshot.
    pub fn matches_source(&self, identity: SourceIdentity) -> bool {
        match self.source_identity {
            Some(recorded) => recorded.matches(identity),
            None => true,
        }
    }

    /// Serialize as `FCP1 | version | len | json | blake3(json)`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, CheckpointError> {
        let payload = serde_json::to_vec(self)?;
        let checksum = blake3::hash(&payload);
        let mut out = Vec::with_capacity(4 + 4 + 8 + payload.len() + 32);
        out.extend_from_slice(&CHECKPOINT_MAGIC);
        out.extend_from_slice(&CHECKPOINT_FORMAT_VERSION.to_be_bytes());
        out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(checksum.as_bytes());
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CheckpointError> {
        if bytes.len() < 4 + 4 + 8 + 32 {
            return Err(CheckpointError::Corrupt(
                "progress file is truncated".into(),
            ));
        }
        if bytes[..4] != CHECKPOINT_MAGIC {
            return Err(CheckpointError::Corrupt(
                "progress file magic is not FCP1".into(),
            ));
        }
        let version = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        if version != CHECKPOINT_FORMAT_VERSION {
            return Err(CheckpointError::Corrupt(format!(
                "unsupported checkpoint version {version}"
            )));
        }
        let payload_len = u64::from_be_bytes(bytes[8..16].try_into().unwrap()) as usize;
        let expected = 16 + payload_len + 32;
        if bytes.len() != expected {
            return Err(CheckpointError::Corrupt(format!(
                "progress file length {} does not match {expected}",
                bytes.len()
            )));
        }
        let payload = &bytes[16..16 + payload_len];
        let checksum = &bytes[16 + payload_len..];
        if blake3::hash(payload).as_bytes() != checksum {
            return Err(CheckpointError::Corrupt(
                "progress file checksum does not match".into(),
            ));
        }
        let checkpoint: Self = serde_json::from_slice(payload)?;
        if checkpoint.schema != CHECKPOINT_FORMAT_VERSION {
            return Err(CheckpointError::Corrupt(format!(
                "checkpoint schema {} is not supported",
                checkpoint.schema
            )));
        }
        Ok(checkpoint)
    }
}

/// One resumable transfer reconstructed from the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePlan {
    pub checkpoint: TransferCheckpoint,
    pub missing: Vec<u32>,
}

impl ResumePlan {
    pub fn missing_bytes(&self, manifest: &Manifest) -> u64 {
        self.missing
            .iter()
            .filter_map(|index| manifest.block_len(*index as usize))
            .map(|length| length as u64)
            .sum()
    }
}

/// Filesystem-backed checkpoint store rooted at the app data directory.
#[derive(Debug, Clone)]
pub struct CheckpointStore {
    root: PathBuf,
}

impl CheckpointStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dir_for(&self, transfer_id: Uuid) -> PathBuf {
        self.root.join(CHECKPOINT_DIR).join(transfer_id.to_string())
    }

    pub fn manifest_path(&self, transfer_id: Uuid) -> PathBuf {
        self.dir_for(transfer_id).join(MANIFEST_FILE)
    }

    pub fn progress_path(&self, transfer_id: Uuid) -> PathBuf {
        self.dir_for(transfer_id).join(PROGRESS_FILE)
    }

    /// Persist both the manifest and the progress file.
    pub fn save(
        &self,
        manifest: &Manifest,
        checkpoint: &TransferCheckpoint,
    ) -> Result<(), CheckpointError> {
        if !checkpoint.matches_manifest(manifest) {
            return Err(CheckpointError::Corrupt(
                "checkpoint bitmap does not match the manifest block count".into(),
            ));
        }
        std::fs::create_dir_all(self.dir_for(checkpoint.transfer_id))?;
        write_atomic(
            &self.manifest_path(checkpoint.transfer_id),
            &manifest.to_bytes(),
        )?;
        self.save_progress(checkpoint)
    }

    /// Fast path used by periodic progress updates.
    pub fn save_progress(&self, checkpoint: &TransferCheckpoint) -> Result<(), CheckpointError> {
        std::fs::create_dir_all(self.dir_for(checkpoint.transfer_id))?;
        write_atomic(
            &self.progress_path(checkpoint.transfer_id),
            &checkpoint.to_bytes()?,
        )
    }

    pub fn load(
        &self,
        transfer_id: Uuid,
    ) -> Result<Option<(Manifest, TransferCheckpoint)>, CheckpointError> {
        let Some(manifest) = self.load_manifest(transfer_id)? else {
            return Ok(None);
        };
        let Some(checkpoint) = self.load_progress(transfer_id)? else {
            return Ok(None);
        };
        if checkpoint.transfer_id != transfer_id || !checkpoint.matches_manifest(&manifest) {
            return Ok(None);
        }
        Ok(Some((manifest, checkpoint)))
    }

    pub fn load_manifest(&self, transfer_id: Uuid) -> Result<Option<Manifest>, CheckpointError> {
        let path = self.manifest_path(transfer_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        Ok(Some(Manifest::from_bytes(&bytes)?))
    }

    pub fn load_progress(
        &self,
        transfer_id: Uuid,
    ) -> Result<Option<TransferCheckpoint>, CheckpointError> {
        let path = self.progress_path(transfer_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_limited(&path, MAX_PROGRESS_BYTES)?;
        Ok(Some(TransferCheckpoint::from_bytes(&bytes)?))
    }

    /// Rebuild a resume plan, degrading to a fresh transfer when the stored
    /// checkpoint is corrupt, incomplete, or for a different manifest.
    pub fn resume_plan(
        &self,
        transfer_id: Uuid,
        manifest: &Manifest,
    ) -> Result<Option<ResumePlan>, CheckpointError> {
        match self.load(transfer_id) {
            Ok(Some((stored, checkpoint))) => {
                if stored.manifest_digest != manifest.manifest_digest
                    || stored.block_size != manifest.block_size
                    || stored.file_size != manifest.file_size
                {
                    tracing::info!(
                        %transfer_id,
                        "resume rejected: stored manifest does not match the offer"
                    );
                    return Ok(None);
                }
                let missing = checkpoint.missing(manifest);
                Ok(Some(ResumePlan {
                    checkpoint,
                    missing,
                }))
            }
            Ok(None) => {
                tracing::info!(%transfer_id, "resume rejected: no checkpoint on disk");
                Ok(None)
            }
            Err(CheckpointError::Corrupt(_)) => {
                tracing::warn!(%transfer_id, "resume rejected: checkpoint is corrupt");
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Find a checkpoint that matches what is being offered, regardless of the
    /// transfer id the sender minted this time.
    ///
    /// A sender keeps no record of its own sends, so after a restart it has no
    /// way to reuse the previous id - every resend gets a fresh one. What is
    /// stable is the content: the file name and the manifest digest. Those are
    /// what "I already have three gigabytes of this" actually means.
    pub fn resume_plan_for(
        &self,
        file_name: &str,
        manifest: &Manifest,
    ) -> Result<Option<(Uuid, ResumePlan)>, CheckpointError> {
        // Several checkpoints can match one file - every interrupted attempt
        // leaves one - so take the one with the most progress rather than
        // whichever the directory listing happens to return first.
        let mut best: Option<(Uuid, TransferCheckpoint)> = None;
        for id in self.list()? {
            let Ok(Some((stored, checkpoint))) = self.load(id) else {
                continue;
            };
            if stored.manifest_digest != manifest.manifest_digest
                || stored.block_size != manifest.block_size
                || stored.file_size != manifest.file_size
            {
                continue;
            }
            if checkpoint.file_name != file_name {
                continue;
            }
            if checkpoint.is_complete(manifest) {
                tracing::info!(%id, file = %file_name, "resume plan: that checkpoint is already complete");
                continue;
            }
            let done = checkpoint.blocks_done();
            if best
                .as_ref()
                .is_none_or(|(_, existing)| done > existing.blocks_done())
            {
                tracing::info!(%id, file = %file_name, blocks_done = done, "resume plan: candidate");
                best = Some((id, checkpoint));
            }
        }
        match best {
            Some((id, checkpoint)) => {
                tracing::info!(
                    %id,
                    file = %file_name,
                    blocks_done = checkpoint.blocks_done(),
                    "resume plan found by content"
                );
                let missing = checkpoint.missing(manifest);
                Ok(Some((
                    id,
                    ResumePlan {
                        checkpoint,
                        missing,
                    },
                )))
            }
            None => {
                tracing::info!(file = %file_name, "resume plan: no checkpoint matches this file");
                Ok(None)
            }
        }
    }

    pub fn remove(&self, transfer_id: Uuid) -> Result<(), CheckpointError> {
        let dir = self.dir_for(transfer_id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Uuid>, CheckpointError> {
        let root = self.root.join(CHECKPOINT_DIR);
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy()) {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }
}

/// Merge the set bits of a bitmap into byte ranges for sink resume.
pub fn covered_ranges(manifest: &Manifest, received: &ChunkBitmap) -> Vec<(u64, u64)> {
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for index in 0..manifest.block_count().min(received.len()) {
        if !received.is_set(index) {
            continue;
        }
        let start = index as u64 * manifest.block_size as u64;
        let end = start + manifest.block_len(index).unwrap_or(0) as u64;
        match ranges.last_mut() {
            Some((_, last_end)) if *last_end == start => *last_end = end,
            _ => ranges.push((start, end)),
        }
    }
    ranges
}

/// Write a file through a fsynced temporary sibling and an atomic rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CheckpointError> {
    let parent = path
        .parent()
        .ok_or_else(|| CheckpointError::Corrupt("checkpoint path has no parent".into()))?;
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "checkpoint".into());
    let temp = parent.join(format!(".{name}.tmp-{}", Uuid::now_v7()));
    let result = (|| -> Result<(), CheckpointError> {
        {
            let mut file = File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>, CheckpointError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() as usize > limit {
        return Err(CheckpointError::Corrupt(format!(
            "checkpoint file is {} bytes, limit is {limit}",
            metadata.len()
        )));
    }
    Ok(std::fs::read(path)?)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint io error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Bitmap(#[from] BitmapError),
    #[error("checkpoint format error: {0}")]
    Format(#[from] serde_json::Error),
    #[error("checkpoint is corrupt: {0}")]
    Corrupt(String),
}

impl CheckpointError {
    pub fn code(&self) -> ErrorCode {
        match self {
            CheckpointError::Manifest(error) => error.code(),
            CheckpointError::Io(_)
            | CheckpointError::Bitmap(_)
            | CheckpointError::Format(_)
            | CheckpointError::Corrupt(_) => ErrorCode::Internal,
        }
    }
}

impl From<CheckpointError> for Error {
    fn from(value: CheckpointError) -> Self {
        Error::new(value.code(), value.to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{DesktopFileAccessManager, FileAccessManager, FileReference};
    use crate::transfer::manifest::{ManifestOptions, compute_manifest};

    fn manifest_for(data: &[u8], block_size: u32) -> Manifest {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoint.bin");
        std::fs::write(&path, data).unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let id = manager.register_source(FileReference::Path(path)).unwrap();
        let source = manager.open_source(id).unwrap();
        compute_manifest(
            source.as_ref(),
            ManifestOptions {
                block_size,
                window_bytes: block_size as usize,
            },
            &mut |_| {},
        )
        .unwrap()
    }

    fn data(len: usize) -> Vec<u8> {
        (0..len).map(|index| (index % 253) as u8).collect()
    }

    #[test]
    fn progress_round_trips_with_a_checksum() {
        let manifest = manifest_for(&data(4096), 1024);
        let transfer_id = Uuid::now_v7();
        let mut checkpoint =
            TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "file.bin", &manifest);
        assert_eq!(checkpoint.received.len(), manifest.block_count());
        assert!(checkpoint.mark_received(0));
        assert!(checkpoint.mark_received(2));
        checkpoint.set_state(TransferState::Interrupted);
        let bytes = checkpoint.to_bytes().unwrap();
        assert_eq!(TransferCheckpoint::from_bytes(&bytes).unwrap(), checkpoint);
    }

    #[test]
    fn progress_rejects_torn_or_tampered_bytes() {
        let manifest = manifest_for(&data(4096), 1024);
        let checkpoint = TransferCheckpoint::new(
            Uuid::now_v7(),
            TransferRole::Receiver,
            "file.bin",
            &manifest,
        );
        let bytes = checkpoint.to_bytes().unwrap();

        assert!(matches!(
            TransferCheckpoint::from_bytes(&bytes[..12]),
            Err(CheckpointError::Corrupt(_))
        ));
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(matches!(
            TransferCheckpoint::from_bytes(&bad_magic),
            Err(CheckpointError::Corrupt(_))
        ));
        let mut bad_version = bytes.clone();
        bad_version[4..8].copy_from_slice(&7_u32.to_be_bytes());
        assert!(matches!(
            TransferCheckpoint::from_bytes(&bad_version),
            Err(CheckpointError::Corrupt(_))
        ));
        let mut torn = bytes.clone();
        torn.truncate(bytes.len() - 1);
        assert!(matches!(
            TransferCheckpoint::from_bytes(&torn),
            Err(CheckpointError::Corrupt(_))
        ));
        let mut flipped = bytes.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0xff;
        assert!(matches!(
            TransferCheckpoint::from_bytes(&flipped),
            Err(CheckpointError::Corrupt(_))
        ));
    }

    #[test]
    fn store_saves_and_resumes_missing_blocks() {
        let root = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(root.path());
        let manifest = manifest_for(&data(4096), 1024);
        let transfer_id = Uuid::now_v7();
        let mut checkpoint =
            TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "file.bin", &manifest);
        checkpoint.set_state(TransferState::Transferring);
        checkpoint.mark_received(0);
        checkpoint.mark_received(1);
        store.save(&manifest, &checkpoint).unwrap();

        let (stored, loaded) = store.load(transfer_id).unwrap().unwrap();
        assert_eq!(stored.manifest_digest, manifest.manifest_digest);
        assert_eq!(loaded.blocks_done(), 2);
        assert_eq!(loaded.received_bytes(&manifest), 2048);
        assert_eq!(store.list().unwrap(), vec![transfer_id]);

        let plan = store.resume_plan(transfer_id, &manifest).unwrap().unwrap();
        assert_eq!(plan.missing, vec![2, 3]);
        assert_eq!(plan.missing_bytes(&manifest), 2048);

        store.remove(transfer_id).unwrap();
        assert!(store.load(transfer_id).unwrap().is_none());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn store_ignores_a_leftover_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(root.path());
        let manifest = manifest_for(&data(4096), 1024);
        let transfer_id = Uuid::now_v7();
        let mut checkpoint =
            TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "file.bin", &manifest);
        checkpoint.mark_received(0);
        store.save(&manifest, &checkpoint).unwrap();

        // A crash between the temporary write and the rename leaves a sibling
        // that must never be mistaken for the committed progress file.
        let leftover = store.dir_for(transfer_id).join(".progress.bin.tmp-1234");
        std::fs::write(&leftover, b"torn").unwrap();
        let loaded = store.load_progress(transfer_id).unwrap().unwrap();
        assert_eq!(loaded.blocks_done(), 1);
        assert_eq!(store.list().unwrap(), vec![transfer_id]);
        std::fs::remove_file(&leftover).unwrap();
    }

    #[test]
    fn saving_progress_replaces_the_existing_entry() {
        let root = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(root.path());
        let manifest = manifest_for(&data(4096), 1024);
        let transfer_id = Uuid::now_v7();
        let mut checkpoint =
            TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "file.bin", &manifest);
        store.save(&manifest, &checkpoint).unwrap();
        checkpoint.mark_received(3);
        checkpoint.set_state(TransferState::Paused);
        store.save_progress(&checkpoint).unwrap();
        let loaded = store.load_progress(transfer_id).unwrap().unwrap();
        assert_eq!(loaded.blocks_done(), 1);
        assert_eq!(loaded.state, TransferState::Paused);
    }

    #[test]
    fn resume_plan_degrades_on_manifest_mismatch() {
        let root = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(root.path());
        let manifest = manifest_for(&data(4096), 1024);
        let other = manifest_for(&data(8192), 1024);
        let transfer_id = Uuid::now_v7();
        let checkpoint =
            TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "file.bin", &manifest);
        store.save(&manifest, &checkpoint).unwrap();
        assert!(store.resume_plan(transfer_id, &other).unwrap().is_none());
        assert!(store.resume_plan(transfer_id, &manifest).unwrap().is_some());
    }

    #[test]
    fn resume_plan_degrades_on_corrupt_progress() {
        let root = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(root.path());
        let manifest = manifest_for(&data(4096), 1024);
        let transfer_id = Uuid::now_v7();
        let checkpoint =
            TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "file.bin", &manifest);
        store.save(&manifest, &checkpoint).unwrap();
        std::fs::write(store.progress_path(transfer_id), b"garbage").unwrap();
        assert!(store.resume_plan(transfer_id, &manifest).unwrap().is_none());
    }

    #[test]
    fn sender_checkpoint_records_and_checks_source_identity() {
        let manifest = manifest_for(&data(2048), 512);
        let identity = SourceIdentity {
            size: 2048,
            modified_unix_ms: Some(42),
        };
        let checkpoint =
            TransferCheckpoint::new(Uuid::now_v7(), TransferRole::Sender, "file.bin", &manifest)
                .with_source_identity(identity);
        assert!(checkpoint.matches_source(identity));
        assert!(!checkpoint.matches_source(SourceIdentity {
            size: 2049,
            modified_unix_ms: Some(42)
        }));
        assert_eq!(checkpoint.received.count_set(), 0);
    }

    #[test]
    fn covered_ranges_merge_adjacent_blocks() {
        let manifest = manifest_for(&data(4096), 1024);
        let mut bitmap = ChunkBitmap::new(4);
        bitmap.set(0);
        bitmap.set(1);
        bitmap.set(3);
        assert_eq!(
            covered_ranges(&manifest, &bitmap),
            vec![(0, 2048), (3072, 4096)]
        );
    }

    #[test]
    fn checkpoint_error_maps_to_a_stable_code() {
        assert_eq!(
            CheckpointError::Corrupt("bad".into()).code(),
            ErrorCode::Internal
        );
        let converted: Error = CheckpointError::Corrupt("bad".into()).into();
        assert_eq!(converted.code_number, 5001);
    }
}
