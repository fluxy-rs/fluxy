//! Wire format for Fluxy v1.
//!
//! Control frames are `u32 length + protobuf Envelope`; data chunks use a separate
//! fixed binary header and never pass through protobuf.

use prost::Message;
use thiserror::Error;

/// The only protocol version accepted by this build.
pub const PROTOCOL_VERSION: u32 = 1;
/// Maximum encoded Envelope size, excluding the four-byte length prefix.
pub const MAX_CONTROL_FRAME: usize = 8 * 1024 * 1024;
/// Every device/message/session identifier is a 16-byte UUID.
pub const ID_LEN: usize = 16;

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/fluxy.v1.rs"));
}

pub use v1::*;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtoError {
    #[error("control frame is shorter than the 4-byte length prefix")]
    FrameTooShort,
    #[error("control frame length {0} exceeds the {1}-byte limit")]
    FrameTooLarge(usize, usize),
    #[error("frame length prefix {declared} does not match {actual} body bytes")]
    LengthMismatch { declared: usize, actual: usize },
    #[error("envelope could not be decoded: {0}")]
    Decode(String),
    #[error("envelope could not be encoded: {0}")]
    Encode(String),
    #[error("protocol version {0} is not supported")]
    UnsupportedVersion(u32),
    #[error("message_id must be exactly 16 bytes")]
    InvalidMessageIdLength,
    #[error("source_device_id must be exactly 16 bytes")]
    InvalidSourceIdLength,
    #[error("session_id must be exactly 16 bytes")]
    InvalidSessionIdLength,
    #[error("message_id must be a UUIDv7")]
    InvalidMessageIdVersion,
    #[error("source_device_id must not be all zero")]
    InvalidSourceId,
    #[error("unknown message kind {0}")]
    UnknownKind(i32),
    #[error("Hello/HelloAck must use an all-zero session_id")]
    NonZeroHandshakeSession,
}

/// Encode an Envelope with the v1 length prefix and validate it first.
pub fn encode_frame(envelope: &Envelope) -> Result<Vec<u8>, ProtoError> {
    validate_envelope(envelope)?;
    let body = envelope.encode_to_vec();
    if body.len() > MAX_CONTROL_FRAME {
        return Err(ProtoError::FrameTooLarge(body.len(), MAX_CONTROL_FRAME));
    }
    let declared = u32::try_from(body.len())
        .map_err(|_| ProtoError::FrameTooLarge(body.len(), MAX_CONTROL_FRAME))?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&declared.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode a complete frame, enforcing length, version, identifier, and kind checks.
pub fn decode_frame(frame: &[u8]) -> Result<Envelope, ProtoError> {
    if frame.len() < 4 {
        return Err(ProtoError::FrameTooShort);
    }
    let declared = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if declared > MAX_CONTROL_FRAME {
        return Err(ProtoError::FrameTooLarge(declared, MAX_CONTROL_FRAME));
    }
    let actual = frame.len() - 4;
    if declared != actual {
        return Err(ProtoError::LengthMismatch { declared, actual });
    }
    let envelope = Envelope::decode(&frame[4..]).map_err(|e| ProtoError::Decode(e.to_string()))?;
    validate_envelope(&envelope)?;
    Ok(envelope)
}

/// Encode a message-specific payload. Payloads are bounded by the Envelope cap.
pub fn encode_payload<M: Message>(message: &M) -> Vec<u8> {
    message.encode_to_vec()
}

/// Decode a message-specific payload; malformed bytes never panic.
pub fn decode_payload<M: Message + Default>(bytes: &[u8]) -> Result<M, ProtoError> {
    M::decode(bytes).map_err(|e| ProtoError::Decode(e.to_string()))
}

pub fn validate_envelope(envelope: &Envelope) -> Result<(), ProtoError> {
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err(ProtoError::UnsupportedVersion(envelope.protocol_version));
    }
    if envelope.message_id.len() != ID_LEN {
        return Err(ProtoError::InvalidMessageIdLength);
    }
    if envelope.message_id[6] >> 4 != 7 {
        return Err(ProtoError::InvalidMessageIdVersion);
    }
    if envelope.source_device_id.len() != ID_LEN {
        return Err(ProtoError::InvalidSourceIdLength);
    }
    if envelope.source_device_id.iter().all(|b| *b == 0) {
        return Err(ProtoError::InvalidSourceId);
    }
    if envelope.session_id.len() != ID_LEN {
        return Err(ProtoError::InvalidSessionIdLength);
    }
    let kind =
        MessageKind::try_from(envelope.kind).map_err(|_| ProtoError::UnknownKind(envelope.kind))?;
    if matches!(kind, MessageKind::Hello | MessageKind::HelloAck)
        && envelope.session_id.iter().any(|b| *b != 0)
    {
        return Err(ProtoError::NonZeroHandshakeSession);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> Vec<u8> {
        vec![byte; 16]
    }

    fn valid_envelope() -> Envelope {
        let mut message_id = id(1);
        message_id[6] = 0x70; // UUIDv7 version nibble
        Envelope {
            protocol_version: PROTOCOL_VERSION,
            message_id,
            source_device_id: id(2),
            session_id: id(3),
            created_at_ms: 1_700_000_000_000,
            kind: MessageKind::Ping as i32,
            payload: vec![1, 2, 3],
        }
    }

    #[test]
    fn round_trips_envelope() {
        let envelope = valid_envelope();
        let frame = encode_frame(&envelope).expect("encode");
        let decoded = decode_frame(&frame).expect("decode");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn rejects_short_frame() {
        assert_eq!(decode_frame(&[0, 0, 0]), Err(ProtoError::FrameTooShort));
    }

    #[test]
    fn rejects_length_mismatch() {
        let frame = encode_frame(&valid_envelope()).expect("encode");
        assert!(matches!(
            decode_frame(&frame[..frame.len() - 1]),
            Err(ProtoError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn rejects_oversized_declared_frame_without_allocating() {
        let frame = ((MAX_CONTROL_FRAME + 1) as u32).to_be_bytes();
        assert!(matches!(
            decode_frame(&frame),
            Err(ProtoError::FrameTooLarge(..))
        ));
    }

    #[test]
    fn rejects_unknown_kind() {
        let mut envelope = valid_envelope();
        envelope.kind = 9999;
        assert_eq!(encode_frame(&envelope), Err(ProtoError::UnknownKind(9999)));
    }

    #[test]
    fn rejects_bad_ids() {
        let mut envelope = valid_envelope();
        envelope.message_id = vec![1; 15];
        assert_eq!(
            encode_frame(&envelope),
            Err(ProtoError::InvalidMessageIdLength)
        );

        let mut envelope = valid_envelope();
        envelope.source_device_id = vec![0; 16];
        assert_eq!(encode_frame(&envelope), Err(ProtoError::InvalidSourceId));

        let mut envelope = valid_envelope();
        envelope.session_id = vec![1; 15];
        assert_eq!(
            encode_frame(&envelope),
            Err(ProtoError::InvalidSessionIdLength)
        );
    }

    #[test]
    fn handshake_session_must_be_zero() {
        let mut envelope = valid_envelope();
        envelope.kind = MessageKind::Hello as i32;
        assert_eq!(
            encode_frame(&envelope),
            Err(ProtoError::NonZeroHandshakeSession)
        );
        envelope.session_id = vec![0; 16];
        assert!(encode_frame(&envelope).is_ok());
    }

    #[test]
    fn rejects_wrong_protocol_version() {
        let mut envelope = valid_envelope();
        envelope.protocol_version = 2;
        assert_eq!(
            encode_frame(&envelope),
            Err(ProtoError::UnsupportedVersion(2))
        );
    }
}
