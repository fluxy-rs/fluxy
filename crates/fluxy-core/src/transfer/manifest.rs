//! File manifest and parallel BLAKE3 pre-hashing.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode};
use crate::files::{FileError, FileSource, SourceIdentity};

/// Default 4 MiB chunk size from the transfer plan.
pub const DEFAULT_BLOCK_SIZE: u32 = 4 * 1024 * 1024;
/// Largest accepted chunk size.
pub const MAX_BLOCK_SIZE: u32 = 64 * 1024 * 1024;
/// Manifest limit: 32768 block hashes fit the 4 MiB control-frame cap.
pub const MAX_MANIFEST_BLOCKS: usize = 32_768;
/// Default read window for parallel hashing.
pub const DEFAULT_WINDOW_BYTES: usize = 64 * 1024 * 1024;
const HASH_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub block_size: u32,
    pub file_size: u64,
    pub block_hashes: Vec<[u8; HASH_BYTES]>,
    pub root_hash: [u8; HASH_BYTES],
    pub manifest_digest: [u8; HASH_BYTES],
}

impl Manifest {
    pub fn block_count(&self) -> usize {
        self.block_hashes.len()
    }

    pub fn block_len(&self, index: usize) -> Option<usize> {
        if index >= self.block_hashes.len() {
            return None;
        }
        let start = (index as u64).checked_mul(self.block_size as u64)?;
        let remaining = self.file_size.checked_sub(start)?;
        Some(std::cmp::min(remaining, self.block_size as u64) as usize)
    }

    pub fn verify_block(&self, index: usize, data: &[u8]) -> bool {
        match (self.block_hashes.get(index), self.block_len(index)) {
            (Some(expected), Some(length)) => {
                length == data.len() && blake3::hash(data).as_bytes() == expected
            }
            _ => false,
        }
    }

    pub fn verify_root(&self, data: &[u8]) -> bool {
        blake3::hash(data).as_bytes() == &self.root_hash
    }

    pub fn root_hash_hex(&self) -> String {
        hex::encode(self.root_hash)
    }

    pub fn manifest_digest_hex(&self) -> String {
        hex::encode(self.manifest_digest)
    }

    pub fn digest_for_hashes(hashes: &[[u8; HASH_BYTES]]) -> [u8; HASH_BYTES] {
        let mut hasher = blake3::Hasher::new();
        for hash in hashes {
            hasher.update(hash);
        }
        *hasher.finalize().as_bytes()
    }
}

/// Binary manifest framing used by the resume checkpoint store.
pub const MANIFEST_MAGIC: [u8; 4] = *b"FMF1";
/// Checkpoint manifest schema version.
pub const MANIFEST_FORMAT_VERSION: u32 = 1;
const MANIFEST_FIXED_LEN: usize = 4 + 4 + 4 + 8 + HASH_BYTES + HASH_BYTES + 4;

impl Manifest {
    /// Serialize the manifest for `checkpoints/<transfer-id>/manifest.bin`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MANIFEST_FIXED_LEN + self.block_hashes.len() * HASH_BYTES);
        out.extend_from_slice(&MANIFEST_MAGIC);
        out.extend_from_slice(&MANIFEST_FORMAT_VERSION.to_be_bytes());
        out.extend_from_slice(&self.block_size.to_be_bytes());
        out.extend_from_slice(&self.file_size.to_be_bytes());
        out.extend_from_slice(&self.root_hash);
        out.extend_from_slice(&self.manifest_digest);
        out.extend_from_slice(&(self.block_hashes.len() as u32).to_be_bytes());
        for hash in &self.block_hashes {
            out.extend_from_slice(hash);
        }
        out
    }

    /// Parse and validate a checkpoint manifest.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() < MANIFEST_FIXED_LEN {
            return Err(ManifestError::Invalid(
                "manifest bytes are truncated".into(),
            ));
        }
        if bytes[..4] != MANIFEST_MAGIC {
            return Err(ManifestError::Invalid("manifest magic is not FMF1".into()));
        }
        let version = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        if version != MANIFEST_FORMAT_VERSION {
            return Err(ManifestError::Invalid(format!(
                "unsupported manifest format version {version}"
            )));
        }
        let block_size = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            return Err(ManifestError::InvalidBlockSize(block_size));
        }
        let file_size = u64::from_be_bytes(bytes[12..20].try_into().unwrap());
        let mut root_hash = [0_u8; HASH_BYTES];
        root_hash.copy_from_slice(&bytes[20..52]);
        let mut manifest_digest = [0_u8; HASH_BYTES];
        manifest_digest.copy_from_slice(&bytes[52..84]);
        let block_count = u32::from_be_bytes(bytes[84..88].try_into().unwrap()) as usize;
        if block_count > MAX_MANIFEST_BLOCKS {
            return Err(ManifestError::TooManyBlocks {
                blocks: block_count as u64,
                max: MAX_MANIFEST_BLOCKS,
            });
        }
        let expected_len = MANIFEST_FIXED_LEN + block_count * HASH_BYTES;
        if bytes.len() != expected_len {
            return Err(ManifestError::Invalid(format!(
                "manifest length {} does not match {expected_len}",
                bytes.len()
            )));
        }
        let expected_blocks = file_size.div_ceil(block_size as u64) as usize;
        if block_count != expected_blocks {
            return Err(ManifestError::Invalid(format!(
                "manifest has {block_count} blocks but the size needs {expected_blocks}"
            )));
        }
        let mut block_hashes = Vec::with_capacity(block_count);
        let (chunks, _rest) = bytes[MANIFEST_FIXED_LEN..].as_chunks::<HASH_BYTES>();
        for chunk in chunks {
            block_hashes.push(*chunk);
        }
        if Self::digest_for_hashes(&block_hashes) != manifest_digest {
            return Err(ManifestError::Invalid(
                "manifest digest does not match the block hashes".into(),
            ));
        }
        Ok(Self {
            block_size,
            file_size,
            block_hashes,
            root_hash,
            manifest_digest,
        })
    }
}

/// Hashing parameters. `window_bytes` is rounded down to a block multiple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestOptions {
    pub block_size: u32,
    pub window_bytes: usize,
}

impl Default for ManifestOptions {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
            window_bytes: DEFAULT_WINDOW_BYTES,
        }
    }
}

impl ManifestOptions {
    pub fn effective_window_bytes(&self) -> usize {
        let block = self.block_size as usize;
        let window = std::cmp::max(self.window_bytes, block);
        std::cmp::max(block, window - window % block)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashProgress {
    pub bytes_done: u64,
    pub total_bytes: u64,
    pub blocks_done: u64,
    pub total_blocks: u64,
}

impl HashProgress {
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 100.0;
        }
        (self.bytes_done as f64 / self.total_bytes as f64) * 100.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error(transparent)]
    File(#[from] FileError),
    #[error("invalid block size: {0}")]
    InvalidBlockSize(u32),
    #[error("manifest would have {blocks} blocks, limit is {max}")]
    TooManyBlocks { blocks: u64, max: usize },
    #[error("invalid manifest: {0}")]
    Invalid(String),
    #[error("manifest cache io error: {0}")]
    CacheIo(#[from] std::io::Error),
    #[error("manifest cache format error: {0}")]
    CacheFormat(#[from] serde_json::Error),
}

impl ManifestError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ManifestError::File(error) => error.code(),
            ManifestError::InvalidBlockSize(_)
            | ManifestError::TooManyBlocks { .. }
            | ManifestError::Invalid(_) => ErrorCode::Transfer,
            ManifestError::CacheIo(_) | ManifestError::CacheFormat(_) => ErrorCode::Internal,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, ManifestError::File(error) if error.retryable())
    }
}

impl From<ManifestError> for Error {
    fn from(value: ManifestError) -> Self {
        let code = value.code();
        Error::new(code, value.to_string(), value.retryable())
    }
}

/// Compute a manifest for a source using a parallel BLAKE3 window pass.
///
/// Blocks are hashed in parallel inside each read window while the root hash
/// is fed the same window through BLAKE3's rayon tree update, so the file is
/// read once. The callback runs on the calling thread.
pub fn compute_manifest(
    source: &dyn FileSource,
    options: ManifestOptions,
    progress: &mut dyn FnMut(HashProgress),
) -> Result<Manifest, ManifestError> {
    let block_size = options.block_size;
    if block_size == 0 || block_size > MAX_BLOCK_SIZE {
        return Err(ManifestError::InvalidBlockSize(block_size));
    }
    let meta = source.meta();
    let file_size = meta.size;
    let total_blocks = file_size.div_ceil(block_size as u64);
    if total_blocks > MAX_MANIFEST_BLOCKS as u64 {
        return Err(ManifestError::TooManyBlocks {
            blocks: total_blocks,
            max: MAX_MANIFEST_BLOCKS,
        });
    }
    if file_size == 0 {
        let empty = *blake3::hash(&[]).as_bytes();
        let manifest = Manifest {
            block_size,
            file_size,
            block_hashes: Vec::new(),
            root_hash: empty,
            manifest_digest: empty,
        };
        progress(HashProgress {
            bytes_done: 0,
            total_bytes: 0,
            blocks_done: 0,
            total_blocks: 0,
        });
        return Ok(manifest);
    }

    let window_bytes = options.effective_window_bytes();
    let block_size_usize = block_size as usize;
    let mut root_hasher = blake3::Hasher::new();
    let mut block_hashes: Vec<[u8; HASH_BYTES]> = Vec::with_capacity(total_blocks as usize);
    let mut offset: u64 = 0;
    // Temporary instrumentation: how much of the pre-hash is I/O and how much is
    // BLAKE3. Printed to stderr so it shows up in adb logcat without adding a
    // logging dependency to this crate.
    let mut read_ns: u128 = 0;
    let mut hash_ns: u128 = 0;
    let mut windows: u64 = 0;
    while offset < file_size {
        let remaining = file_size - offset;
        let wanted = std::cmp::min(window_bytes as u64, remaining) as usize;
        let read_started = std::time::Instant::now();
        let window = source.read_at(offset, wanted)?;
        read_ns += read_started.elapsed().as_nanos();
        if window.len() != wanted {
            return Err(ManifestError::File(FileError::SourceChanged));
        }
        let hash_started = std::time::Instant::now();
        root_hasher.update_rayon(window.as_ref());
        let window_hashes: Vec<[u8; HASH_BYTES]> = window
            .as_ref()
            .par_chunks(block_size_usize)
            .map(|chunk| *blake3::hash(chunk).as_bytes())
            .collect();
        hash_ns += hash_started.elapsed().as_nanos();
        windows += 1;
        for (index, hash) in window_hashes.into_iter().enumerate() {
            block_hashes.push(hash);
            let block_start = offset + index as u64 * block_size as u64;
            let block_end = std::cmp::min(block_start + block_size as u64, file_size);
            progress(HashProgress {
                bytes_done: block_end,
                total_bytes: file_size,
                blocks_done: block_hashes.len() as u64,
                total_blocks,
            });
        }
        offset += wanted as u64;
    }
    let root_hash = *root_hasher.finalize().as_bytes();
    let manifest_digest = Manifest::digest_for_hashes(&block_hashes);
    eprintln!(
        "manifest timing read_ms={} hash_ms={} windows={} window_mib={}",
        read_ns / 1_000_000,
        hash_ns / 1_000_000,
        windows,
        window_bytes / (1024 * 1024)
    );
    Ok(Manifest {
        block_size,
        file_size,
        block_hashes,
        root_hash,
        manifest_digest,
    })
}

/// Whether a manifest came from the cache or a fresh hash pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedManifest {
    pub manifest: Manifest,
    pub status: CacheStatus,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    key: String,
    block_size: u32,
    identity: SourceIdentity,
    manifest: Manifest,
}

/// On-disk manifest cache keyed by a caller label plus the source identity.
///
/// The cache never replaces verification: a hit is only used when the stored
/// identity, block size, block count, and manifest digest all match. A source
/// change invalidates the entry.
pub struct ManifestCache {
    root: PathBuf,
}

impl ManifestCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Default cache key for a local path and block size.
    pub fn key_for(path: &Path, block_size: u32) -> String {
        Self::key_for_source(&path.to_string_lossy(), block_size)
    }

    /// Cache key for any source identity: a filesystem path on desktop, a
    /// document URI on Android. Both are opaque strings to the cache.
    pub fn key_for_source(source: &str, block_size: u32) -> String {
        format!("{source}#{block_size}")
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        let digest = blake3::hash(key.as_bytes()).to_hex();
        self.root.join(format!("{digest}.json"))
    }

    pub fn load(
        &self,
        key: &str,
        identity: SourceIdentity,
        block_size: u32,
    ) -> Result<Option<Manifest>, ManifestError> {
        let path = self.entry_path(key);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ManifestError::CacheIo(error)),
        };
        let entry: CacheEntry = serde_json::from_slice(&bytes)?;
        if entry.key != key || entry.block_size != block_size || entry.identity != identity {
            return Ok(None);
        }
        let manifest = entry.manifest;
        let expected_blocks = manifest.file_size.div_ceil(block_size as u64);
        if manifest.block_size != block_size
            || manifest.block_hashes.len() as u64 != expected_blocks
            || manifest.manifest_digest != Manifest::digest_for_hashes(&manifest.block_hashes)
        {
            return Ok(None);
        }
        Ok(Some(manifest))
    }

    pub fn store(
        &self,
        key: &str,
        identity: SourceIdentity,
        manifest: &Manifest,
    ) -> Result<(), ManifestError> {
        std::fs::create_dir_all(&self.root)?;
        let entry = CacheEntry {
            key: key.to_string(),
            block_size: manifest.block_size,
            identity,
            manifest: manifest.clone(),
        };
        let bytes = serde_json::to_vec(&entry)?;
        let path = self.entry_path(key);
        let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
        std::fs::write(&temp, &bytes)?;
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        if let Err(error) = std::fs::rename(&temp, &path) {
            let _ = std::fs::remove_file(&temp);
            return Err(ManifestError::CacheIo(error));
        }
        Ok(())
    }

    pub fn remove(&self, key: &str) -> Result<(), ManifestError> {
        match std::fs::remove_file(self.entry_path(key)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ManifestError::CacheIo(error)),
        }
    }

    /// Return a cached manifest or hash the source and store the result.
    pub fn get_or_compute(
        &self,
        key: &str,
        source: &dyn FileSource,
        options: ManifestOptions,
        progress: &mut dyn FnMut(HashProgress),
    ) -> Result<CachedManifest, ManifestError> {
        let identity = source.identity();
        if let Some(manifest) = self.load(key, identity, options.block_size)?
            && manifest.file_size == source.meta().size
        {
            let total_blocks = manifest.block_count() as u64;
            progress(HashProgress {
                bytes_done: manifest.file_size,
                total_bytes: manifest.file_size,
                blocks_done: total_blocks,
                total_blocks,
            });
            return Ok(CachedManifest {
                manifest,
                status: CacheStatus::Hit,
            });
        }
        let manifest = compute_manifest(source, options, progress)?;
        self.store(key, identity, &manifest)?;
        Ok(CachedManifest {
            manifest,
            status: CacheStatus::Miss,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{DesktopFileAccessManager, FileAccessManager, FileReference};
    use std::time::{Duration, Instant};

    fn manager_for(dir: &Path) -> DesktopFileAccessManager {
        DesktopFileAccessManager::new(dir)
    }

    fn open_source(manager: &DesktopFileAccessManager, path: &Path) -> Box<dyn FileSource> {
        let id = manager
            .register_source(FileReference::Path(path.to_path_buf()))
            .unwrap();
        manager.open_source(id).unwrap()
    }

    fn pseudo_random(len: usize) -> Vec<u8> {
        let mut state = 0x1234_5678_9abc_def0_u64;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) as u8
            })
            .collect()
    }

    #[test]
    fn manifest_hashes_match_reference_values() {
        let dir = tempfile::tempdir().unwrap();
        let data = b"hello blake";
        let path = dir.path().join("small.bin");
        std::fs::write(&path, data).unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        let options = ManifestOptions {
            block_size: 4,
            window_bytes: 8,
        };
        let manifest = compute_manifest(source.as_ref(), options, &mut |_| {}).unwrap();
        assert_eq!(manifest.block_count(), 3);
        assert_eq!(manifest.block_len(0), Some(4));
        assert_eq!(manifest.block_len(1), Some(4));
        assert_eq!(manifest.block_len(2), Some(3));
        assert_eq!(manifest.block_len(3), None);
        for (index, chunk) in data.chunks(4).enumerate() {
            assert!(manifest.verify_block(index, chunk));
            assert_eq!(
                manifest.block_hashes[index],
                *blake3::hash(chunk).as_bytes()
            );
        }
        assert!(manifest.verify_root(data));
        assert_eq!(manifest.root_hash, *blake3::hash(data).as_bytes());
        assert_eq!(
            manifest.manifest_digest,
            Manifest::digest_for_hashes(&manifest.block_hashes)
        );
        assert_eq!(manifest.root_hash_hex().len(), 64);
        assert_eq!(manifest.manifest_digest_hex().len(), 64);
    }

    #[test]
    fn parallel_windows_match_a_single_pass() {
        let dir = tempfile::tempdir().unwrap();
        let data = pseudo_random(5 * 1024 * 1024);
        let path = dir.path().join("five.bin");
        std::fs::write(&path, &data).unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        let options = ManifestOptions {
            block_size: 1024 * 1024,
            window_bytes: 2 * 1024 * 1024,
        };
        let mut progress = Vec::new();
        let manifest =
            compute_manifest(source.as_ref(), options, &mut |step| progress.push(step)).unwrap();
        assert_eq!(manifest.block_count(), 5);
        for (index, chunk) in data.chunks(1024 * 1024).enumerate() {
            assert_eq!(
                manifest.block_hashes[index],
                *blake3::hash(chunk).as_bytes()
            );
        }
        assert_eq!(manifest.root_hash, *blake3::hash(&data).as_bytes());
        assert_eq!(progress.len(), 5);
        let last = progress.last().copied().unwrap();
        assert_eq!(last.bytes_done, data.len() as u64);
        assert_eq!(last.blocks_done, 5);
        assert_eq!(last.total_blocks, 5);
        assert_eq!(last.percent(), 100.0);
        for pair in progress.windows(2) {
            assert!(pair[1].bytes_done >= pair[0].bytes_done);
            assert!(pair[1].blocks_done >= pair[0].blocks_done);
        }
    }

    #[test]
    fn empty_file_has_the_empty_root_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.bin");
        std::fs::write(&path, b"").unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        let mut calls = 0;
        let manifest = compute_manifest(source.as_ref(), ManifestOptions::default(), &mut |step| {
            calls += 1;
            assert_eq!(step.percent(), 100.0);
        })
        .unwrap();
        assert_eq!(manifest.block_count(), 0);
        assert_eq!(manifest.root_hash, *blake3::hash(&[]).as_bytes());
        assert_eq!(manifest.manifest_digest, manifest.root_hash);
        assert_eq!(calls, 1);
    }

    #[test]
    fn rejects_invalid_block_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.bin");
        std::fs::write(&path, b"x").unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        assert!(matches!(
            compute_manifest(
                source.as_ref(),
                ManifestOptions {
                    block_size: 0,
                    window_bytes: 1024
                },
                &mut |_| {}
            ),
            Err(ManifestError::InvalidBlockSize(0))
        ));
        assert!(matches!(
            compute_manifest(
                source.as_ref(),
                ManifestOptions {
                    block_size: MAX_BLOCK_SIZE + 1,
                    window_bytes: 1024
                },
                &mut |_| {}
            ),
            Err(ManifestError::InvalidBlockSize(_))
        ));
    }

    #[test]
    fn rejects_a_manifest_over_the_block_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("many.bin");
        std::fs::write(&path, vec![0_u8; MAX_MANIFEST_BLOCKS + 1]).unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        let result = compute_manifest(
            source.as_ref(),
            ManifestOptions {
                block_size: 1,
                window_bytes: 4096,
            },
            &mut |_| {},
        );
        assert!(matches!(
            result,
            Err(ManifestError::TooManyBlocks { blocks, max })
                if blocks == (MAX_MANIFEST_BLOCKS as u64 + 1) && max == MAX_MANIFEST_BLOCKS
        ));
    }

    #[test]
    fn verify_block_rejects_wrong_length_and_bytes() {
        let manifest = Manifest {
            block_size: 4,
            file_size: 4,
            block_hashes: vec![*blake3::hash(b"data").as_bytes()],
            root_hash: *blake3::hash(b"data").as_bytes(),
            manifest_digest: [0; HASH_BYTES],
        };
        assert!(manifest.verify_block(0, b"data"));
        assert!(!manifest.verify_block(0, b"dat"));
        assert!(!manifest.verify_block(0, b"datb"));
        assert!(!manifest.verify_block(1, b"data"));
        assert!(manifest.verify_root(b"data"));
        assert!(!manifest.verify_root(b"other"));
    }

    #[test]
    fn cache_hits_then_invalidates_after_a_source_change() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let path = dir.path().join("cached.bin");
        std::fs::write(&path, b"first version").unwrap();
        let manager = manager_for(dir.path());
        let cache = ManifestCache::new(&cache_dir);
        let key = ManifestCache::key_for(&path, 4);
        let options = ManifestOptions {
            block_size: 4,
            window_bytes: 8,
        };
        let first = cache
            .get_or_compute(
                &key,
                open_source(&manager, &path).as_ref(),
                options,
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(first.status, CacheStatus::Miss);
        let second = cache
            .get_or_compute(
                &key,
                open_source(&manager, &path).as_ref(),
                options,
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(second.status, CacheStatus::Hit);
        assert_eq!(first.manifest, second.manifest);
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, b"second versio").unwrap();
        let third = cache
            .get_or_compute(
                &key,
                open_source(&manager, &path).as_ref(),
                options,
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(third.status, CacheStatus::Miss);
        assert_ne!(first.manifest.root_hash, third.manifest.root_hash);
    }

    #[test]
    fn cache_ignores_an_identity_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ManifestCache::new(dir.path().join("cache"));
        let manifest = Manifest {
            block_size: 4,
            file_size: 4,
            block_hashes: vec![*blake3::hash(b"data").as_bytes()],
            root_hash: *blake3::hash(b"data").as_bytes(),
            manifest_digest: Manifest::digest_for_hashes(&[*blake3::hash(b"data").as_bytes()]),
        };
        cache
            .store(
                "key",
                SourceIdentity {
                    size: 4,
                    modified_unix_ms: Some(1),
                },
                &manifest,
            )
            .unwrap();
        let hit = cache
            .load(
                "key",
                SourceIdentity {
                    size: 4,
                    modified_unix_ms: Some(2),
                },
                4,
            )
            .unwrap();
        assert!(hit.is_none());
        let hit = cache
            .load(
                "key",
                SourceIdentity {
                    size: 4,
                    modified_unix_ms: Some(1),
                },
                8,
            )
            .unwrap();
        assert!(hit.is_none());
    }

    #[test]
    fn cache_rejects_a_tampered_manifest_digest() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ManifestCache::new(dir.path().join("cache"));
        let manifest = Manifest {
            block_size: 4,
            file_size: 4,
            block_hashes: vec![*blake3::hash(b"data").as_bytes()],
            root_hash: *blake3::hash(b"data").as_bytes(),
            manifest_digest: Manifest::digest_for_hashes(&[*blake3::hash(b"data").as_bytes()]),
        };
        let identity = SourceIdentity {
            size: 4,
            modified_unix_ms: Some(1),
        };
        cache.store("key", identity, &manifest).unwrap();
        let entry_path = cache.entry_path("key");
        let mut entry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&entry_path).unwrap()).unwrap();
        entry["manifest"]["manifest_digest"][0] = serde_json::json!(99);
        std::fs::write(&entry_path, serde_json::to_vec(&entry).unwrap()).unwrap();
        assert!(cache.load("key", identity, 4).unwrap().is_none());
    }

    #[test]
    fn cache_key_includes_the_block_size() {
        let path = Path::new("some/file.bin");
        assert_ne!(
            ManifestCache::key_for(path, 4),
            ManifestCache::key_for(path, 8)
        );
        assert!(ManifestCache::key_for(path, 4).contains("some/file.bin"));
    }

    #[test]
    fn cache_store_replaces_an_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ManifestCache::new(dir.path().join("cache"));
        let identity = SourceIdentity {
            size: 4,
            modified_unix_ms: Some(1),
        };
        let first = Manifest {
            block_size: 4,
            file_size: 4,
            block_hashes: vec![*blake3::hash(b"one!").as_bytes()],
            root_hash: *blake3::hash(b"one!").as_bytes(),
            manifest_digest: Manifest::digest_for_hashes(&[*blake3::hash(b"one!").as_bytes()]),
        };
        let second = Manifest {
            block_size: 4,
            file_size: 4,
            block_hashes: vec![*blake3::hash(b"two!").as_bytes()],
            root_hash: *blake3::hash(b"two!").as_bytes(),
            manifest_digest: Manifest::digest_for_hashes(&[*blake3::hash(b"two!").as_bytes()]),
        };
        cache.store("key", identity, &first).unwrap();
        cache.store("key", identity, &second).unwrap();
        assert_eq!(cache.load("key", identity, 4).unwrap(), Some(second));
    }

    #[test]
    fn manifest_error_maps_to_stable_codes() {
        assert_eq!(
            ManifestError::TooManyBlocks { blocks: 1, max: 1 }.code(),
            ErrorCode::Transfer
        );
        assert_eq!(
            ManifestError::InvalidBlockSize(0).code(),
            ErrorCode::Transfer
        );
        let converted: Error = ManifestError::Invalid("bad".into()).into();
        assert_eq!(converted.code_number, 3001);
    }

    #[test]
    fn manifest_bytes_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let data = pseudo_random(1024 * 1024 + 13);
        let path = dir.path().join("round.bin");
        std::fs::write(&path, &data).unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        let manifest = compute_manifest(
            source.as_ref(),
            ManifestOptions {
                block_size: 256 * 1024,
                window_bytes: 256 * 1024,
            },
            &mut |_| {},
        )
        .unwrap();
        let bytes = manifest.to_bytes();
        assert_eq!(Manifest::from_bytes(&bytes).unwrap(), manifest);
    }

    #[test]
    fn manifest_bytes_reject_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let data = pseudo_random(64 * 1024);
        let path = dir.path().join("corrupt.bin");
        std::fs::write(&path, &data).unwrap();
        let manager = manager_for(dir.path());
        let source = open_source(&manager, &path);
        let manifest = compute_manifest(
            source.as_ref(),
            ManifestOptions {
                block_size: 16 * 1024,
                window_bytes: 16 * 1024,
            },
            &mut |_| {},
        )
        .unwrap();
        let bytes = manifest.to_bytes();

        assert!(matches!(
            Manifest::from_bytes(&bytes[..10]),
            Err(ManifestError::Invalid(_))
        ));
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(matches!(
            Manifest::from_bytes(&bad_magic),
            Err(ManifestError::Invalid(_))
        ));
        let mut bad_version = bytes.clone();
        bad_version[4..8].copy_from_slice(&99_u32.to_be_bytes());
        assert!(matches!(
            Manifest::from_bytes(&bad_version),
            Err(ManifestError::Invalid(_))
        ));
        let mut bad_digest = bytes.clone();
        bad_digest[52] ^= 0xff;
        assert!(matches!(
            Manifest::from_bytes(&bad_digest),
            Err(ManifestError::Invalid(_))
        ));
        let mut short_body = bytes.clone();
        short_body.truncate(bytes.len() - 1);
        assert!(matches!(
            Manifest::from_bytes(&short_body),
            Err(ManifestError::Invalid(_))
        ));
        let mut bad_count = bytes.clone();
        bad_count[84..88].copy_from_slice(&0_u32.to_be_bytes());
        assert!(matches!(
            Manifest::from_bytes(&bad_count),
            Err(ManifestError::Invalid(_))
        ));
    }

    #[test]
    #[ignore = "requires FLUXY_BENCH_FILE pointing at a 10 GiB file"]
    fn benchmark_10gib_prehash_under_15s() {
        let Ok(path) = std::env::var("FLUXY_BENCH_FILE") else {
            eprintln!("FLUXY_BENCH_FILE is not set; skipping the 10 GiB benchmark");
            return;
        };
        let path = PathBuf::from(path);
        let manager = manager_for(path.parent().unwrap_or(Path::new(".")));
        let source = open_source(&manager, &path);
        let mut last = HashProgress {
            bytes_done: 0,
            total_bytes: 0,
            blocks_done: 0,
            total_blocks: 0,
        };
        let started = Instant::now();
        let manifest = compute_manifest(source.as_ref(), ManifestOptions::default(), &mut |step| {
            last = step;
        })
        .unwrap();
        let elapsed = started.elapsed();
        eprintln!(
            "hashed {} bytes in {:?} ({:.1} MiB/s, {} blocks)",
            manifest.file_size,
            elapsed,
            manifest.file_size as f64 / 1_048_576.0 / elapsed.as_secs_f64(),
            manifest.block_count()
        );
        assert_eq!(last.bytes_done, manifest.file_size);
        assert!(
            elapsed.as_secs_f64() <= 15.0,
            "10 GiB pre-hash took {elapsed:?}, over the 15 s budget"
        );
    }
}
