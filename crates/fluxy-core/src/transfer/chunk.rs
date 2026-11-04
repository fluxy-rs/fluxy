//! Binary chunk header for QUIC data streams.
//!
//! Layout from the protocol document:
//!
//! ```text
//! offset  size  field
//! 0       4     magic "FCH1"
//! 4       16    transfer_id
//! 20      4     chunk_index (u32 BE)
//! 24      8     offset (u64 BE)
//! 32      4     payload_len (u32 BE)
//! 36      32    BLAKE3(payload)
//! 68      N     payload
//! ```

use uuid::Uuid;

use super::manifest::Manifest;
use crate::error::{Error, ErrorCode};

pub const CHUNK_MAGIC: [u8; 4] = *b"FCH1";
pub const CHUNK_HEADER_LEN: usize = 68;
/// Chunk payload cap from the transfer limits table.
pub const MAX_CHUNK_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkHeader {
    pub transfer_id: Uuid,
    pub chunk_index: u32,
    pub offset: u64,
    pub payload_len: u32,
    pub payload_hash: [u8; 32],
}

impl ChunkHeader {
    /// Build and validate a header for one manifest block.
    pub fn for_block(
        transfer_id: Uuid,
        manifest: &Manifest,
        index: usize,
        payload: &[u8],
    ) -> Result<Self, ChunkError> {
        let Some(length) = manifest.block_len(index) else {
            return Err(ChunkError::IndexOutOfRange(index));
        };
        if payload.len() != length {
            return Err(ChunkError::LengthMismatch {
                expected: length,
                actual: payload.len(),
            });
        }
        if payload.len() > MAX_CHUNK_BYTES {
            return Err(ChunkError::TooLarge(payload.len()));
        }
        let hash = *blake3::hash(payload).as_bytes();
        if manifest.block_hashes.get(index) != Some(&hash) {
            return Err(ChunkError::ManifestHashMismatch(index));
        }
        Ok(Self {
            transfer_id,
            chunk_index: index as u32,
            offset: index as u64 * manifest.block_size as u64,
            payload_len: payload.len() as u32,
            payload_hash: hash,
        })
    }

    /// The 68-byte frame header on its own, so a sender can write it and then
    /// the payload without copying the payload into a new frame buffer.
    pub fn header_bytes(&self) -> [u8; CHUNK_HEADER_LEN] {
        let mut out = [0_u8; CHUNK_HEADER_LEN];
        out[..4].copy_from_slice(&CHUNK_MAGIC);
        out[4..20].copy_from_slice(self.transfer_id.as_bytes());
        out[20..24].copy_from_slice(&self.chunk_index.to_be_bytes());
        out[24..32].copy_from_slice(&self.offset.to_be_bytes());
        out[32..36].copy_from_slice(&self.payload_len.to_be_bytes());
        out[36..68].copy_from_slice(&self.payload_hash);
        out
    }

    pub fn encode_with_payload(&self, payload: &[u8]) -> Result<Vec<u8>, ChunkError> {
        if payload.len() > MAX_CHUNK_BYTES {
            return Err(ChunkError::TooLarge(payload.len()));
        }
        if payload.len() != self.payload_len as usize {
            return Err(ChunkError::LengthMismatch {
                expected: self.payload_len as usize,
                actual: payload.len(),
            });
        }
        if blake3::hash(payload).as_bytes() != &self.payload_hash {
            return Err(ChunkError::HashMismatch);
        }
        let mut out = Vec::with_capacity(CHUNK_HEADER_LEN + payload.len());
        out.extend_from_slice(&CHUNK_MAGIC);
        out.extend_from_slice(self.transfer_id.as_bytes());
        out.extend_from_slice(&self.chunk_index.to_be_bytes());
        out.extend_from_slice(&self.offset.to_be_bytes());
        out.extend_from_slice(&self.payload_len.to_be_bytes());
        out.extend_from_slice(&self.payload_hash);
        out.extend_from_slice(payload);
        Ok(out)
    }

    pub fn decode(frame: &[u8]) -> Result<(Self, &[u8]), ChunkError> {
        let (header, payload) = Self::decode_header(frame)?;
        if blake3::hash(payload).as_bytes() != &header.payload_hash {
            return Err(ChunkError::HashMismatch);
        }
        Ok((header, payload))
    }

    /// Parse the frame header without validating the payload hash, so a caller
    /// can attribute a rejected chunk to its index and retry it.
    pub fn decode_header(frame: &[u8]) -> Result<(Self, &[u8]), ChunkError> {
        if frame.len() < CHUNK_HEADER_LEN {
            return Err(ChunkError::Truncated);
        }
        if frame[..4] != CHUNK_MAGIC {
            return Err(ChunkError::BadMagic);
        }
        let transfer_id = Uuid::from_slice(&frame[4..20]).map_err(|_| ChunkError::BadTransferId)?;
        let chunk_index = u32::from_be_bytes(frame[20..24].try_into().unwrap());
        let offset = u64::from_be_bytes(frame[24..32].try_into().unwrap());
        let payload_len = u32::from_be_bytes(frame[32..36].try_into().unwrap());
        if payload_len as usize > MAX_CHUNK_BYTES {
            return Err(ChunkError::TooLarge(payload_len as usize));
        }
        let mut payload_hash = [0_u8; 32];
        payload_hash.copy_from_slice(&frame[36..68]);
        let payload = &frame[CHUNK_HEADER_LEN..];
        if payload.len() != payload_len as usize {
            return Err(ChunkError::LengthMismatch {
                expected: payload_len as usize,
                actual: payload.len(),
            });
        }
        Ok((
            Self {
                transfer_id,
                chunk_index,
                offset,
                payload_len,
                payload_hash,
            },
            payload,
        ))
    }

    /// Check the header against the manifest and payload before writing.
    pub fn verify(&self, manifest: &Manifest, payload: &[u8]) -> Result<(), ChunkError> {
        let index = self.chunk_index as usize;
        let Some(length) = manifest.block_len(index) else {
            return Err(ChunkError::IndexOutOfRange(index));
        };
        if self.offset != index as u64 * manifest.block_size as u64 {
            return Err(ChunkError::OffsetMismatch {
                index,
                offset: self.offset,
            });
        }
        if length != payload.len() || length != self.payload_len as usize {
            return Err(ChunkError::LengthMismatch {
                expected: length,
                actual: payload.len(),
            });
        }
        if manifest.block_hashes.get(index) != Some(&self.payload_hash) {
            return Err(ChunkError::ManifestHashMismatch(index));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChunkError {
    #[error("chunk frame is shorter than the {CHUNK_HEADER_LEN}-byte header")]
    Truncated,
    #[error("chunk magic is not FCH1")]
    BadMagic,
    #[error("chunk transfer id is not a valid UUID")]
    BadTransferId,
    #[error("chunk payload length {actual} does not match the expected {expected}")]
    LengthMismatch { expected: usize, actual: usize },
    #[error("chunk payload exceeds the {0}-byte cap")]
    TooLarge(usize),
    #[error("chunk payload hash does not match the header")]
    HashMismatch,
    #[error("chunk index {0} is outside the manifest")]
    IndexOutOfRange(usize),
    #[error("chunk {index} offset {offset} does not match the manifest")]
    OffsetMismatch { index: usize, offset: u64 },
    #[error("chunk {0} does not match the manifest block hash")]
    ManifestHashMismatch(usize),
}

impl ChunkError {
    pub fn code(&self) -> ErrorCode {
        ErrorCode::Transfer
    }
}

impl From<ChunkError> for Error {
    fn from(value: ChunkError) -> Self {
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
        let path = dir.path().join("chunk.bin");
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

    #[test]
    fn round_trips_a_chunk_frame() {
        let data = b"hello chunk transfer";
        let manifest = manifest_for(data, 8);
        let transfer_id = Uuid::now_v7();
        let payload = &data[..8];
        let header = ChunkHeader::for_block(transfer_id, &manifest, 0, payload).unwrap();
        let frame = header.encode_with_payload(payload).unwrap();
        assert_eq!(frame.len(), CHUNK_HEADER_LEN + payload.len());
        let (decoded, decoded_payload) = ChunkHeader::decode(&frame).unwrap();
        assert_eq!(decoded, header);
        assert_eq!(decoded_payload, payload);
        assert!(decoded.verify(&manifest, decoded_payload).is_ok());
    }

    #[test]
    fn rejects_bad_magic_truncation_and_length() {
        let data = b"abcdefgh";
        let manifest = manifest_for(data, 8);
        let transfer_id = Uuid::now_v7();
        let header = ChunkHeader::for_block(transfer_id, &manifest, 0, data).unwrap();
        let mut frame = header.encode_with_payload(data).unwrap();
        frame[0] = b'X';
        assert_eq!(ChunkHeader::decode(&frame), Err(ChunkError::BadMagic));
        frame[0] = b'F';
        assert_eq!(
            ChunkHeader::decode(&frame[..CHUNK_HEADER_LEN - 1]),
            Err(ChunkError::Truncated)
        );
        assert_eq!(
            ChunkHeader::decode(&frame[..frame.len() - 1]),
            Err(ChunkError::LengthMismatch {
                expected: 8,
                actual: 7
            })
        );
    }

    #[test]
    fn rejects_hash_and_manifest_mismatches() {
        let data = b"abcdefgh";
        let manifest = manifest_for(data, 8);
        let transfer_id = Uuid::now_v7();
        let mut frame = ChunkHeader::for_block(transfer_id, &manifest, 0, data)
            .unwrap()
            .encode_with_payload(data)
            .unwrap();
        frame[CHUNK_HEADER_LEN] ^= 0xff;
        assert_eq!(ChunkHeader::decode(&frame), Err(ChunkError::HashMismatch));
        // The header still parses, so the receiver can name the rejected block.
        let (header, _) = ChunkHeader::decode_header(&frame).unwrap();
        assert_eq!(header.chunk_index, 0);
        assert!(matches!(
            ChunkHeader::for_block(transfer_id, &manifest, 0, b"12345678"),
            Err(ChunkError::ManifestHashMismatch(0))
        ));
    }

    #[test]
    fn rejects_out_of_range_and_wrong_offset() {
        let data = b"abcdefgh";
        let manifest = manifest_for(data, 8);
        let transfer_id = Uuid::now_v7();
        assert!(matches!(
            ChunkHeader::for_block(transfer_id, &manifest, 3, data),
            Err(ChunkError::IndexOutOfRange(3))
        ));
        let mut header = ChunkHeader::for_block(transfer_id, &manifest, 0, data).unwrap();
        header.offset = 5;
        assert!(matches!(
            header.verify(&manifest, data),
            Err(ChunkError::OffsetMismatch {
                index: 0,
                offset: 5
            })
        ));
    }

    #[test]
    fn encode_rejects_a_length_or_hash_mismatch() {
        let data = b"abcdefgh";
        let manifest = manifest_for(data, 8);
        let transfer_id = Uuid::now_v7();
        let header = ChunkHeader::for_block(transfer_id, &manifest, 0, data).unwrap();
        assert!(matches!(
            header.encode_with_payload(b"short"),
            Err(ChunkError::LengthMismatch { .. })
        ));
        let mut wrong = header.clone();
        wrong.payload_hash[0] ^= 0xff;
        assert_eq!(
            wrong.encode_with_payload(data),
            Err(ChunkError::HashMismatch)
        );
    }

    #[test]
    fn error_maps_to_transfer_code() {
        assert_eq!(ChunkError::BadMagic.code(), ErrorCode::Transfer);
        let converted: Error = ChunkError::Truncated.into();
        assert_eq!(converted.code_number, 3001);
    }
}
