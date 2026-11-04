//! Compact per-chunk received bitmap.

use serde::{Deserialize, Serialize};

/// Bit set for every received chunk. The serialized form carries its length so
/// a checkpoint can be validated after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkBitmap {
    len: usize,
    bits: Vec<u8>,
}

impl ChunkBitmap {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            bits: vec![0; len.div_ceil(8)],
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_set(&self, index: usize) -> bool {
        if index >= self.len {
            return false;
        }
        self.bits[index / 8] & (1 << (index % 8)) != 0
    }

    /// Set a chunk bit. Returns true only when it was previously clear.
    pub fn set(&mut self, index: usize) -> bool {
        if index >= self.len || self.is_set(index) {
            return false;
        }
        self.bits[index / 8] |= 1 << (index % 8);
        true
    }

    pub fn clear(&mut self) {
        self.bits.fill(0);
    }

    pub fn count_set(&self) -> usize {
        self.bits
            .iter()
            .map(|byte| byte.count_ones() as usize)
            .sum()
    }

    pub fn is_complete(&self) -> bool {
        self.count_set() == self.len
    }

    pub fn first_missing(&self) -> Option<usize> {
        (0..self.len).find(|index| !self.is_set(*index))
    }

    pub fn missing_indices(&self) -> Vec<usize> {
        (0..self.len).filter(|index| !self.is_set(*index)).collect()
    }

    /// Serialize as an 8-byte big-endian length followed by the bit bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.bits.len());
        out.extend_from_slice(&(self.len as u64).to_be_bytes());
        out.extend_from_slice(&self.bits);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BitmapError> {
        if bytes.len() < 8 {
            return Err(BitmapError::Truncated);
        }
        let len = u64::from_be_bytes(bytes[..8].try_into().unwrap()) as usize;
        let expected = len.div_ceil(8);
        if bytes.len() - 8 != expected {
            return Err(BitmapError::LengthMismatch {
                expected,
                actual: bytes.len() - 8,
            });
        }
        Ok(Self {
            len,
            bits: bytes[8..].to_vec(),
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BitmapError {
    #[error("bitmap is shorter than its 8-byte length header")]
    Truncated,
    #[error("bitmap length {actual} does not match the expected {expected} bytes")]
    LengthMismatch { expected: usize, actual: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_and_reports_missing_chunks() {
        let mut bitmap = ChunkBitmap::new(10);
        assert_eq!(bitmap.len(), 10);
        assert!(!bitmap.is_set(0));
        assert!(bitmap.set(0));
        assert!(!bitmap.set(0));
        assert!(bitmap.set(9));
        assert!(bitmap.set(4));
        assert_eq!(bitmap.count_set(), 3);
        assert!(!bitmap.is_complete());
        assert_eq!(bitmap.first_missing(), Some(1));
        assert_eq!(bitmap.missing_indices(), vec![1, 2, 3, 5, 6, 7, 8]);
        assert!(!bitmap.set(10));
        assert!(!bitmap.is_set(10));
    }

    #[test]
    fn completes_when_every_bit_is_set() {
        let mut bitmap = ChunkBitmap::new(3);
        for index in 0..3 {
            bitmap.set(index);
        }
        assert!(bitmap.is_complete());
        assert_eq!(bitmap.first_missing(), None);
        assert!(bitmap.missing_indices().is_empty());
        bitmap.clear();
        assert!(!bitmap.is_complete());
        assert_eq!(bitmap.count_set(), 0);
    }

    #[test]
    fn round_trips_through_bytes() {
        let mut bitmap = ChunkBitmap::new(13);
        bitmap.set(0);
        bitmap.set(7);
        bitmap.set(12);
        let bytes = bitmap.to_bytes();
        assert_eq!(ChunkBitmap::from_bytes(&bytes).unwrap(), bitmap);
    }

    #[test]
    fn rejects_truncated_or_mismatched_bytes() {
        assert_eq!(
            ChunkBitmap::from_bytes(&[0, 0, 0]),
            Err(BitmapError::Truncated)
        );
        let mut bytes = ChunkBitmap::new(9).to_bytes();
        bytes.push(0);
        assert!(matches!(
            ChunkBitmap::from_bytes(&bytes),
            Err(BitmapError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn empty_bitmap_is_complete() {
        let bitmap = ChunkBitmap::new(0);
        assert!(bitmap.is_empty());
        assert!(bitmap.is_complete());
        assert_eq!(bitmap.first_missing(), None);
    }
}
