//! Deterministic decoder fuzzing.
//!
//! The protocol decoders are the part of Fluxy that parses bytes from an
//! untrusted peer, so they must never panic, never allocate unbounded memory,
//! and never accept a frame whose arithmetic overflows. cargo-fuzz needs a
//! nightly toolchain and libFuzzer; this harness gives the same signal on stable
//! with a fixed seed, so a finding is reproducible by anyone:
//!
//! ```text
//! cargo test -p fluxy-core --release --test decoder_fuzz -- --nocapture
//! FLUXY_FUZZ_ITERATIONS=200000 cargo test -p fluxy-core --release --test decoder_fuzz
//! ```
//!
//! Two generators run side by side. Pure random bytes mostly stop at the magic
//! check, which proves very little, so most iterations mutate a *valid* sample:
//! that is what reaches the length, offset, and hash arithmetic inside each
//! decoder.

use fluxy_core::files::{DesktopFileAccessManager, FileAccessManager, FileReference};
use fluxy_core::transfer::batch::{
    BatchEntry, BatchPlan, sanitize_root_name, validate_relative_path,
};
use fluxy_core::transfer::{
    ChunkHeader, Manifest, ManifestOptions, TransferCheckpoint, TransferRole, compute_manifest,
};
use fluxy_proto::{Envelope, Hello, MessageKind, decode_frame, decode_payload, encode_frame};
use uuid::Uuid;

/// Small deterministic RNG; xorshift64* is enough for a fuzz generator and keeps
/// the harness dependency-free.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }

    fn byte(&mut self) -> u8 {
        (self.next() & 0xff) as u8
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }

    /// Flip a handful of bytes, so the sample stays close to valid.
    fn mutate(&mut self, sample: &[u8]) -> Vec<u8> {
        let mut out = sample.to_vec();
        if out.is_empty() {
            return out;
        }
        let flips = 1 + self.below(8);
        for _ in 0..flips {
            let index = self.below(out.len());
            out[index] ^= 1 << self.below(8);
        }
        // A quarter of the mutations also truncate, which exercises every
        // "frame is shorter than its header" path.
        if self.below(4) == 0 {
            let keep = self.below(out.len() + 1);
            out.truncate(keep);
        }
        out
    }
}

fn iterations() -> usize {
    std::env::var("FLUXY_FUZZ_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20_000)
}

fn sample_manifest() -> Manifest {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.bin");
    let data: Vec<u8> = (0..300_000u32).map(|index| (index % 251) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let manager = DesktopFileAccessManager::new(dir.path());
    let id = manager.register_source(FileReference::Path(path)).unwrap();
    let source = manager.open_source(id).unwrap();
    compute_manifest(
        source.as_ref(),
        ManifestOptions {
            block_size: 64 * 1024,
            window_bytes: 64 * 1024,
        },
        &mut |_| {},
    )
    .unwrap()
}

fn sample_envelope() -> Vec<u8> {
    let envelope = Envelope {
        protocol_version: 1,
        message_id: Uuid::now_v7().as_bytes().to_vec(),
        source_device_id: Uuid::now_v7().as_bytes().to_vec(),
        session_id: Uuid::now_v7().as_bytes().to_vec(),
        created_at_ms: 1_700_000_000_000,
        kind: MessageKind::ChatText as i32,
        payload: b"hello".to_vec(),
    };
    encode_frame(&envelope).unwrap()
}

#[test]
fn decoders_never_panic_on_random_or_mutated_input() {
    let total = iterations();
    let mut rng = Rng::new(0x5EED_F00D);

    let manifest = sample_manifest();
    let transfer_id = Uuid::now_v7();
    let block = manifest.block_len(0).unwrap();
    // The sample file is filled with `index % 251`, so the first block is
    // reproducible without reading the file back.
    let payload: Vec<u8> = (0..block).map(|index| (index % 251) as u8).collect();
    let chunk = ChunkHeader::for_block(transfer_id, &manifest, 0, &payload)
        .unwrap()
        .encode_with_payload(&payload)
        .unwrap();
    let manifest_bytes = manifest.to_bytes();
    let checkpoint =
        TransferCheckpoint::new(transfer_id, TransferRole::Receiver, "sample.bin", &manifest)
            .in_batch(Uuid::now_v7(), "root", "a/b.bin")
            .to_bytes()
            .unwrap();
    let envelope = sample_envelope();

    for _ in 0..total {
        // 1. pure garbage, capped so the harness itself stays fast
        let len = rng.below(4096);
        let garbage = rng.bytes(len);
        let _ = decode_frame(&garbage);
        let _ = Manifest::from_bytes(&garbage);
        let _ = TransferCheckpoint::from_bytes(&garbage);
        let _ = ChunkHeader::decode_header(&garbage);
        let _ = ChunkHeader::decode(&garbage);
        let _ = BatchPlan::from_json(&garbage);
        let _ = decode_payload::<Hello>(&garbage);

        // 2. mutations of valid samples reach the arithmetic
        let _ = decode_frame(&rng.mutate(&envelope));
        let _ = Manifest::from_bytes(&rng.mutate(&manifest_bytes));
        let _ = TransferCheckpoint::from_bytes(&rng.mutate(&checkpoint));
        let _ = ChunkHeader::decode(&rng.mutate(&chunk));

        // 3. path validation and root-name sanitising get text-like input
        let path_len = rng.below(64);
        let path = String::from_utf8_lossy(&rng.bytes(path_len)).to_string();
        let _ = validate_relative_path(&path);
        let _ = sanitize_root_name(&path);
    }
}

#[test]
fn the_fuzz_samples_are_themselves_valid() {
    // Guards the harness: if a sample stops being valid, the mutation test above
    // would silently stop exercising the parser.
    let manifest = sample_manifest();
    assert_eq!(
        Manifest::from_bytes(&manifest.to_bytes()).unwrap(),
        manifest
    );
    let checkpoint = TransferCheckpoint::new(
        Uuid::now_v7(),
        TransferRole::Receiver,
        "sample.bin",
        &manifest,
    );
    let restored = TransferCheckpoint::from_bytes(&checkpoint.to_bytes().unwrap()).unwrap();
    assert_eq!(restored.transfer_id, checkpoint.transfer_id);
    assert!(decode_frame(&sample_envelope()).is_ok());

    let plan = BatchPlan::new(
        Uuid::now_v7(),
        "root",
        vec![BatchEntry::file("a/b.bin", 3, 0)],
    )
    .unwrap();
    assert_eq!(
        BatchPlan::from_json(&plan.to_json().unwrap()).unwrap(),
        plan
    );
}
