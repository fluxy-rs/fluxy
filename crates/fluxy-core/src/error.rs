use serde::{Deserialize, Serialize};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Stable error codes from the v1 protocol plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Protocol,
    UnsupportedVersion,
    Identity,
    Trust,
    Timeout,
    NetworkUnavailable,
    Transfer,
    SourceChanged,
    FileAccess,
    DiskFull,
    HashMismatch,
    MissingChunks,
    Canceled,
    ClipboardTooLarge,
    PeerBusy,
    Internal,
}

impl ErrorCode {
    pub const fn as_u16(self) -> u16 {
        match self {
            ErrorCode::Protocol => 1001,
            ErrorCode::UnsupportedVersion => 1002,
            ErrorCode::Identity => 1003,
            ErrorCode::Trust => 1004,
            ErrorCode::Timeout => 2001,
            ErrorCode::NetworkUnavailable => 2003,
            ErrorCode::Transfer => 3001,
            ErrorCode::SourceChanged => 3002,
            ErrorCode::FileAccess => 3003,
            ErrorCode::DiskFull => 3004,
            ErrorCode::HashMismatch => 3005,
            ErrorCode::MissingChunks => 3006,
            ErrorCode::Canceled => 3008,
            ErrorCode::ClipboardTooLarge => 4002,
            ErrorCode::PeerBusy => 2002,
            ErrorCode::Internal => 5001,
        }
    }

    /// Stable snake_case name from the v1 protocol table. The wire uses this
    /// string so a peer can log a code it does not know.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Protocol => "protocol",
            ErrorCode::UnsupportedVersion => "unsupported_version",
            ErrorCode::Identity => "identity",
            ErrorCode::Trust => "trust",
            ErrorCode::Timeout => "timeout",
            ErrorCode::NetworkUnavailable => "network_unavailable",
            ErrorCode::Transfer => "transfer",
            ErrorCode::SourceChanged => "source_changed",
            ErrorCode::FileAccess => "file_access",
            ErrorCode::DiskFull => "disk_full",
            ErrorCode::HashMismatch => "hash_mismatch",
            ErrorCode::MissingChunks => "missing_chunks",
            ErrorCode::Canceled => "canceled",
            ErrorCode::ClipboardTooLarge => "clipboard_too_large",
            ErrorCode::PeerBusy => "peer_busy",
            ErrorCode::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub code_number: u16,
    pub message: String,
    pub retryable: bool,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            code_number: code.as_u16(),
            message: message.into(),
            retryable,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message, false)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {:?}: {}",
            self.code_number, self.code, self.message
        )
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::internal(value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::new(ErrorCode::Protocol, value.to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_the_protocol_table() {
        assert_eq!(ErrorCode::Transfer.as_u16(), 3001);
        assert_eq!(ErrorCode::SourceChanged.as_u16(), 3002);
        assert_eq!(ErrorCode::FileAccess.as_u16(), 3003);
        assert_eq!(ErrorCode::DiskFull.as_u16(), 3004);
        assert_eq!(ErrorCode::HashMismatch.as_u16(), 3005);
        assert_eq!(ErrorCode::MissingChunks.as_u16(), 3006);
        assert_eq!(ErrorCode::Canceled.as_u16(), 3008);
        assert_eq!(ErrorCode::Internal.as_u16(), 5001);
    }

    #[test]
    fn every_code_has_a_stable_name_and_number() {
        let table = [
            (ErrorCode::Protocol, "protocol", 1001),
            (ErrorCode::UnsupportedVersion, "unsupported_version", 1002),
            (ErrorCode::Identity, "identity", 1003),
            (ErrorCode::Trust, "trust", 1004),
            (ErrorCode::Timeout, "timeout", 2001),
            (ErrorCode::PeerBusy, "peer_busy", 2002),
            (ErrorCode::NetworkUnavailable, "network_unavailable", 2003),
            (ErrorCode::Transfer, "transfer", 3001),
            (ErrorCode::SourceChanged, "source_changed", 3002),
            (ErrorCode::FileAccess, "file_access", 3003),
            (ErrorCode::DiskFull, "disk_full", 3004),
            (ErrorCode::HashMismatch, "hash_mismatch", 3005),
            (ErrorCode::MissingChunks, "missing_chunks", 3006),
            (ErrorCode::Canceled, "canceled", 3008),
            (ErrorCode::ClipboardTooLarge, "clipboard_too_large", 4002),
            (ErrorCode::Internal, "internal", 5001),
        ];
        for (code, name, number) in table {
            assert_eq!(code.as_str(), name);
            assert_eq!(code.as_u16(), number);
            let json = serde_json::to_string(&code).unwrap();
            assert_eq!(json, format!("\"{name}\""));
        }
    }

    #[test]
    fn error_serializes_with_a_snake_case_code() {
        let error = Error::new(ErrorCode::Canceled, "user cancelled", false);
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"canceled\""));
        assert!(json.contains("\"code_number\":3008"));
    }
}
