//! Receiver-side pull scheduler.
//!
//! The receiver owns a chunk bitmap and a bounded window of outstanding
//! requests. It never lets the sender push unrequested chunks: each request
//! carries a deadline, and a timed-out request is retried until the retry
//! budget is exhausted.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::bitmap::ChunkBitmap;
use super::manifest::Manifest;

/// Window sizing unit.
///
/// The name is historical: a file's chunks all share its transfer stream now,
/// so this no longer counts streams. It multiplies `window_per_stream` into the
/// number of requests the receiver keeps outstanding.
pub const DEFAULT_STREAMS: usize = 8;
/// Default requests in flight per window unit.
pub const DEFAULT_WINDOW_PER_STREAM: usize = 4;
/// Default retry budget per chunk.
pub const DEFAULT_MAX_RETRIES: u32 = 3;
/// Default per-chunk timeout from the transfer plan.
pub const DEFAULT_CHUNK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Window sizing unit; see [`DEFAULT_STREAMS`]. Not a stream count.
    pub streams: usize,
    /// Requests in flight per unit.
    pub window_per_stream: usize,
    pub max_retries: u32,
    pub chunk_timeout: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            streams: DEFAULT_STREAMS,
            window_per_stream: DEFAULT_WINDOW_PER_STREAM,
            max_retries: DEFAULT_MAX_RETRIES,
            chunk_timeout: DEFAULT_CHUNK_TIMEOUT,
        }
    }
}

impl SchedulerConfig {
    pub fn window(&self) -> usize {
        self.streams.saturating_mul(self.window_per_stream).max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Outstanding {
    attempts: u32,
    deadline: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferProgress {
    pub blocks_done: u64,
    pub total_blocks: u64,
    pub bytes_done: u64,
    pub total_bytes: u64,
}

impl TransferProgress {
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 100.0;
        }
        (self.bytes_done as f64 / self.total_bytes as f64) * 100.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullScheduler {
    block_size: u32,
    file_size: u64,
    received: ChunkBitmap,
    received_bytes: u64,
    outstanding: HashMap<u32, Outstanding>,
    failed: BTreeSet<u32>,
    cursor: usize,
    config: SchedulerConfig,
}

impl PullScheduler {
    pub fn from_manifest(manifest: &Manifest, config: SchedulerConfig) -> Self {
        Self::new(
            manifest.block_count(),
            manifest.block_size,
            manifest.file_size,
            config,
        )
    }

    pub fn new(
        block_count: usize,
        block_size: u32,
        file_size: u64,
        config: SchedulerConfig,
    ) -> Self {
        Self {
            block_size,
            file_size,
            received: ChunkBitmap::new(block_count),
            received_bytes: 0,
            outstanding: HashMap::new(),
            failed: BTreeSet::new(),
            cursor: 0,
            config,
        }
    }

    /// Rebuild a scheduler from a persisted bitmap. A bitmap whose length does
    /// not match the manifest is discarded so a stale checkpoint degrades to a
    /// fresh transfer instead of corrupting the file.
    pub fn resume_from(
        manifest: &Manifest,
        config: SchedulerConfig,
        received: ChunkBitmap,
    ) -> Self {
        let mut scheduler = Self::new(
            manifest.block_count(),
            manifest.block_size,
            manifest.file_size,
            config,
        );
        if received.len() != manifest.block_count() {
            return scheduler;
        }
        let received_bytes = (0..received.len())
            .filter(|index| received.is_set(*index))
            .filter_map(|index| manifest.block_len(index))
            .map(|length| length as u64)
            .sum();
        scheduler.received = received;
        scheduler.received_bytes = received_bytes;
        scheduler
    }

    pub fn config(&self) -> SchedulerConfig {
        self.config
    }

    pub fn total_blocks(&self) -> usize {
        self.received.len()
    }

    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }

    pub fn bitmap(&self) -> &ChunkBitmap {
        &self.received
    }

    pub fn failed_chunks(&self) -> Vec<u32> {
        self.failed.iter().copied().collect()
    }

    pub fn has_failed(&self) -> bool {
        !self.failed.is_empty()
    }

    pub fn is_complete(&self) -> bool {
        self.received.is_complete()
    }

    pub fn progress(&self) -> TransferProgress {
        TransferProgress {
            blocks_done: self.received.count_set() as u64,
            total_blocks: self.received.len() as u64,
            bytes_done: self.received_bytes,
            total_bytes: self.file_size,
        }
    }

    /// Expire timed-out requests, then fill the window with missing chunks.
    ///
    /// The returned indices are the newly issued requests. A chunk whose retry
    /// budget is exhausted moves to the failed set and is never requested again.
    pub fn next_requests(&mut self, now: Instant) -> Vec<u32> {
        let expired: Vec<u32> = self
            .outstanding
            .iter()
            .filter(|(_, state)| state.deadline <= now)
            .map(|(index, _)| *index)
            .collect();
        let mut issued = Vec::new();
        for index in expired {
            if let Some(mut state) = self.outstanding.remove(&index) {
                if state.attempts >= self.config.max_retries {
                    self.failed.insert(index);
                } else {
                    state.attempts += 1;
                    state.deadline = now + self.config.chunk_timeout;
                    self.outstanding.insert(index, state);
                    issued.push(index);
                }
            }
        }
        if self.is_complete() || self.has_failed() {
            return issued;
        }
        let total = self.received.len();
        let window = self.config.window();
        while self.outstanding.len() < window && issued.len() < window {
            let Some(index) = self.find_missing(total) else {
                break;
            };
            self.outstanding.insert(
                index,
                Outstanding {
                    attempts: 1,
                    deadline: now + self.config.chunk_timeout,
                },
            );
            issued.push(index);
        }
        issued
    }

    fn find_missing(&mut self, total: usize) -> Option<u32> {
        if total == 0 {
            return None;
        }
        for step in 0..total {
            let index = (self.cursor + step) % total;
            let index_u32 = index as u32;
            if !self.received.is_set(index)
                && !self.outstanding.contains_key(&index_u32)
                && !self.failed.contains(&index_u32)
            {
                self.cursor = (index + 1) % total;
                return Some(index_u32);
            }
        }
        None
    }

    /// Requeue the whole window so a paused transfer stops timing out.
    ///
    /// A pause is not a failure: the retry budget is not consumed, and the
    /// outstanding requests are simply re-issued from scratch on resume.
    pub fn pause(&mut self) {
        self.outstanding.clear();
    }

    /// Mark a chunk as received. Returns false for duplicates or unknown indices.
    pub fn on_chunk_received(&mut self, index: u32, now: Instant) -> bool {
        let _ = now;
        let index_usize = index as usize;
        if index_usize >= self.received.len() || self.received.is_set(index_usize) {
            return false;
        }
        self.outstanding.remove(&index);
        self.failed.remove(&index);
        if self.received.set(index_usize) {
            let start = index as u64 * self.block_size as u64;
            let end = std::cmp::min(start + self.block_size as u64, self.file_size);
            self.received_bytes += end.saturating_sub(start);
            true
        } else {
            false
        }
    }

    /// Report a rejected or corrupt chunk. The request is expired immediately
    /// so the next tick re-issues it instead of waiting out the full timeout;
    /// the attempt still counts toward the retry budget.
    pub fn on_chunk_rejected(&mut self, index: u32, now: Instant) {
        if let Some(state) = self.outstanding.get_mut(&index) {
            if state.attempts >= self.config.max_retries {
                self.outstanding.remove(&index);
                self.failed.insert(index);
            } else {
                state.attempts += 1;
                state.deadline = now;
            }
        }
    }

    pub fn mark_received(&mut self, index: u32) {
        let _ = self.on_chunk_received(index, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheduler(blocks: usize) -> PullScheduler {
        PullScheduler::new(
            blocks,
            4,
            blocks as u64 * 4,
            SchedulerConfig {
                streams: 4,
                window_per_stream: 2,
                max_retries: 3,
                chunk_timeout: Duration::from_secs(15),
            },
        )
    }

    #[test]
    fn fills_the_window_without_duplicates() {
        let now = Instant::now();
        let mut scheduler = scheduler(20);
        let first = scheduler.next_requests(now);
        assert_eq!(first, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(scheduler.outstanding(), 8);
        assert!(scheduler.next_requests(now).is_empty());
    }

    #[test]
    fn issues_the_next_batch_after_receipts() {
        let now = Instant::now();
        let mut scheduler = scheduler(20);
        let first = scheduler.next_requests(now);
        for index in first.iter().take(4) {
            assert!(scheduler.on_chunk_received(*index, now));
        }
        let second = scheduler.next_requests(now);
        assert_eq!(second.len(), 4);
        for index in second {
            assert!(!first.contains(&index));
        }
        assert_eq!(scheduler.outstanding(), 8);
    }

    #[test]
    fn retries_a_timed_out_chunk_then_fails_it() {
        let now = Instant::now();
        let mut scheduler = scheduler(1);
        assert_eq!(scheduler.next_requests(now), vec![0]);
        let later = now + Duration::from_secs(16);
        assert_eq!(scheduler.next_requests(later), vec![0]);
        assert_eq!(scheduler.outstanding(), 1);
        let later = later + Duration::from_secs(16);
        assert_eq!(scheduler.next_requests(later), vec![0]);
        let later = later + Duration::from_secs(16);
        assert!(scheduler.next_requests(later).is_empty());
        assert!(scheduler.has_failed());
        assert_eq!(scheduler.failed_chunks(), vec![0]);
        assert_eq!(scheduler.outstanding(), 0);
    }

    #[test]
    fn rejects_duplicate_and_unknown_receipts() {
        let now = Instant::now();
        let mut scheduler = scheduler(2);
        scheduler.next_requests(now);
        assert!(scheduler.on_chunk_received(0, now));
        assert!(!scheduler.on_chunk_received(0, now));
        assert!(!scheduler.on_chunk_received(9, now));
        assert_eq!(scheduler.progress().blocks_done, 1);
    }

    #[test]
    fn completes_and_reports_progress() {
        let now = Instant::now();
        let mut scheduler = scheduler(3);
        let requests = scheduler.next_requests(now);
        assert_eq!(requests.len(), 3);
        for index in requests {
            scheduler.on_chunk_received(index, now);
        }
        assert!(scheduler.is_complete());
        assert!(scheduler.next_requests(now).is_empty());
        let progress = scheduler.progress();
        assert_eq!(progress.blocks_done, 3);
        assert_eq!(progress.total_blocks, 3);
        assert_eq!(progress.bytes_done, 12);
        assert_eq!(progress.total_bytes, 12);
        assert_eq!(progress.percent(), 100.0);
    }

    #[test]
    fn immediate_rejection_counts_toward_the_budget() {
        let now = Instant::now();
        let mut scheduler = scheduler(1);
        scheduler.next_requests(now);
        scheduler.on_chunk_rejected(0, now);
        scheduler.on_chunk_rejected(0, now);
        scheduler.on_chunk_rejected(0, now);
        assert!(scheduler.has_failed());
        assert!(scheduler.next_requests(now).is_empty());
    }

    #[test]
    fn a_rejected_chunk_is_reissued_on_the_next_tick() {
        let now = Instant::now();
        let mut scheduler = scheduler(1);
        assert_eq!(scheduler.next_requests(now), vec![0]);
        scheduler.on_chunk_rejected(0, now);
        assert_eq!(scheduler.next_requests(now), vec![0]);
        scheduler.on_chunk_rejected(0, now);
        assert!(scheduler.next_requests(now).is_empty());
        assert!(scheduler.has_failed());
        assert_eq!(scheduler.failed_chunks(), vec![0]);
    }

    #[test]
    fn pause_requeues_the_window_without_burning_retries() {
        let now = Instant::now();
        let mut scheduler = scheduler(4);
        assert_eq!(scheduler.next_requests(now), vec![0, 1, 2, 3]);
        scheduler.pause();
        assert_eq!(scheduler.outstanding(), 0);
        assert!(!scheduler.has_failed());
        assert_eq!(scheduler.next_requests(now), vec![0, 1, 2, 3]);
        assert_eq!(scheduler.outstanding(), 4);
    }

    #[test]
    fn resumes_from_a_persisted_bitmap() {
        let manifest = Manifest {
            block_size: 4,
            file_size: 16,
            block_hashes: vec![[0_u8; 32]; 4],
            root_hash: [0_u8; 32],
            manifest_digest: [0_u8; 32],
        };
        let mut bitmap = ChunkBitmap::new(4);
        bitmap.set(1);
        bitmap.set(3);
        let now = Instant::now();
        let mut scheduler =
            PullScheduler::resume_from(&manifest, SchedulerConfig::default(), bitmap);
        assert_eq!(scheduler.progress().bytes_done, 8);
        assert_eq!(scheduler.next_requests(now), vec![0, 2]);

        let stale =
            PullScheduler::resume_from(&manifest, SchedulerConfig::default(), ChunkBitmap::new(3));
        assert_eq!(stale.progress().bytes_done, 0);
        assert_eq!(stale.bitmap().len(), 4);
    }

    #[test]
    fn progress_handles_an_empty_file() {
        let scheduler = scheduler(0);
        assert!(scheduler.is_complete());
        assert_eq!(scheduler.progress().percent(), 100.0);
    }
}
