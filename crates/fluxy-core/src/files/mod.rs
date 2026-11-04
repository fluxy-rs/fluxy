//! Cross-platform file access abstraction.
//!
//! The transfer layer only ever receives a FileHandleId, never a platform
//! path or URI. A FileAccessManager registers local references, opens
//! FileSource handles for reading, and creates FileSink handles for
//! receiving. The desktop implementation lives in the desktop module.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode};

pub mod android;
mod desktop;
pub mod folder;

pub use android::{AndroidFileAccessManager, AndroidFileSource};
pub use desktop::{DesktopFileAccessManager, DesktopFileSink, DesktopFileSource};
pub use folder::scan_folder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileHandleId(pub uuid::Uuid);

impl FileHandleId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl Default for FileHandleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for FileHandleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Bounded file metadata. It never contains a platform path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    pub modified_unix_ms: Option<u64>,
}

impl FileMeta {
    pub fn new(name: impl Into<String>, size: u64, modified_unix_ms: Option<u64>) -> Self {
        Self {
            name: name.into(),
            size,
            modified_unix_ms,
        }
    }
}

/// Capabilities probed from a real handle, never inferred from a path suffix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCapabilities {
    pub random_read: bool,
    pub random_write: bool,
    pub persistent_access: bool,
    pub size_known: bool,
}

/// A platform reference supplied by a file picker or drag-and-drop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileReference {
    Path(PathBuf),
    Uri(String),
    Bookmark(Vec<u8>),
}

/// A cheap identity snapshot used to detect a changed source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub size: u64,
    pub modified_unix_ms: Option<u64>,
}

impl SourceIdentity {
    pub fn matches(self, other: Self) -> bool {
        self == other
    }
}

/// The final visible path returned after a sink commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalPath(pub PathBuf);

impl FinalPath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

/// File access failures with a stable mapping to ErrorCode.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("path is not a regular file: {0}")]
    NotRegularFile(String),
    #[error("unknown file handle")]
    InvalidHandle,
    #[error("file reference is not supported on this platform")]
    Unsupported,
    #[error("read outside the file bounds: offset={offset} len={len} size={size}")]
    ReadOutOfRange { offset: u64, len: usize, size: u64 },
    #[error("write outside the file bounds: offset={offset} len={len} size={size}")]
    WriteOutOfRange { offset: u64, len: usize, size: u64 },
    #[error("invalid file name: {0}")]
    InvalidName(String),
    #[error("destination already exists: {0}")]
    AlreadyExists(String),
    #[error("sink is incomplete: {written} of {size} bytes covered")]
    Incomplete { written: u64, size: u64 },
    #[error("source changed while reading")]
    SourceChanged,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl FileError {
    pub fn code(&self) -> ErrorCode {
        match self {
            FileError::SourceChanged => ErrorCode::SourceChanged,
            FileError::Io(error) if error.kind() == std::io::ErrorKind::StorageFull => {
                ErrorCode::DiskFull
            }
            FileError::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                ErrorCode::FileAccess
            }
            FileError::NotFound(_)
            | FileError::NotRegularFile(_)
            | FileError::InvalidHandle
            | FileError::Unsupported
            | FileError::ReadOutOfRange { .. }
            | FileError::WriteOutOfRange { .. }
            | FileError::InvalidName(_)
            | FileError::AlreadyExists(_)
            | FileError::Incomplete { .. }
            | FileError::Io(_) => ErrorCode::Transfer,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, FileError::Io(_))
    }
}

impl From<FileError> for Error {
    fn from(value: FileError) -> Self {
        let code = value.code();
        Error::new(code, value.to_string(), value.retryable())
    }
}

/// Streaming block size for final verification hashing.
pub const HASH_READ_BYTES: usize = 1024 * 1024;

/// Hash a reader with BLAKE3, streaming in bounded blocks so a multi-gigabyte
/// file is never loaded into memory.
pub fn hash_reader(reader: &mut impl std::io::Read) -> Result<[u8; 32], FileError> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; HASH_READ_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Random-access or sequential reader for one registered file.
pub trait FileSource: Send + Sync {
    fn meta(&self) -> FileMeta;
    fn capabilities(&self) -> FileCapabilities;
    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes, FileError>;
    fn identity(&self) -> SourceIdentity;

    /// Re-stat the underlying file. A value that differs from `identity` means
    /// the source was modified after it was opened.
    fn stat(&self) -> Result<SourceIdentity, FileError>;

    /// Compare the live stat with the identity captured at open, returning
    /// 3002 when the source changed mid-transfer.
    fn verify_unchanged(&self) -> Result<(), FileError> {
        if self.stat()? == self.identity() {
            Ok(())
        } else {
            Err(FileError::SourceChanged)
        }
    }
}

/// Writer for one received file. A sink is single-owner and not shared.
pub trait FileSink: Send {
    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), FileError>;
    fn sync(&mut self) -> Result<(), FileError>;

    /// Hash the partial file with BLAKE3 for the final root-hash check. The
    /// sink is flushed first so the hash covers every written block.
    fn root_hash(&mut self) -> Result<[u8; 32], FileError>;

    fn finalize(self: Box<Self>) -> Result<FinalPath, FileError>;
    fn abort(self: Box<Self>);

    /// Remove the partial file in place without finalizing. Used when a
    /// transfer is cancelled, so no temporary file is left behind.
    fn discard(&mut self) -> Result<(), FileError>;
}

/// Local registry that hides platform references from the transfer layer.
pub trait FileAccessManager: Send + Sync {
    fn register_source(&self, reference: FileReference) -> Result<FileHandleId, FileError>;
    fn open_source(&self, id: FileHandleId) -> Result<Box<dyn FileSource>, FileError>;

    /// Register a source from a descriptor the platform already opened. Only
    /// platforms whose pickers hand over descriptors implement this; everywhere
    /// else a path goes through [Self::register_source].
    fn register_descriptor(
        &self,
        uri: &str,
        file: std::fs::File,
        name: &str,
        mime: Option<String>,
    ) -> Result<(FileHandleId, FileMeta), FileError> {
        let _ = (uri, file, name, mime);
        Err(FileError::Unsupported)
    }

    /// Create a receiver-root temporary sink whose name is derived from the
    /// transfer id, so a checkpoint can reopen the same partial file.
    fn create_sink_with_id(
        &self,
        transfer_id: uuid::Uuid,
        name: &str,
        size: u64,
    ) -> Result<Box<dyn FileSink>, FileError>;

    /// Reopen an existing partial sink after a restart and seed its coverage
    /// from the checkpoint ranges. Missing ranges mean a fresh transfer.
    fn resume_sink(
        &self,
        transfer_id: uuid::Uuid,
        name: &str,
        size: u64,
        covered: &[(u64, u64)],
    ) -> Result<Box<dyn FileSink>, FileError>;

    /// Convenience for callers that do not persist a checkpoint.
    fn create_sink(&self, name: &str, size: u64) -> Result<Box<dyn FileSink>, FileError> {
        self.create_sink_with_id(uuid::Uuid::now_v7(), name, size)
    }

    fn export(&self, path: &Path) -> Result<(), FileError>;
}

const MAX_NAME_BYTES: usize = 255;

/// Basic portable name validation. M2-10 extends this with the full
/// overwrite, traversal, and platform-reserved policy.
pub fn validate_file_name(name: &str) -> Result<String, FileError> {
    if name.is_empty() {
        return Err(FileError::InvalidName("name is empty".into()));
    }
    if name == "." || name == ".." {
        return Err(FileError::InvalidName("name is a directory marker".into()));
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(FileError::InvalidName("name is too long".into()));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(FileError::InvalidName(
            "name ends with a dot or space".into(),
        ));
    }
    if name
        .chars()
        .any(|character| character == '/' || character == '\\' || character == '\u{0}')
    {
        return Err(FileError::InvalidName(
            "name contains a path separator or NUL".into(),
        ));
    }
    if name.chars().any(|character| character.is_control()) {
        return Err(FileError::InvalidName(
            "name contains a control character".into(),
        ));
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return Err(FileError::InvalidName("name is reserved on Windows".into()));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_handle_ids_are_unique_and_displayable() {
        let first = FileHandleId::new();
        let second = FileHandleId::new();
        assert_ne!(first, second);
        assert_eq!(first.to_string(), first.0.to_string());
        assert_eq!(FileHandleId::default().as_uuid().get_version_num(), 7);
    }

    #[test]
    fn validate_accepts_normal_names() {
        for name in ["report.pdf", "photo 1.png", "文档.txt", "a", "a.b.c"] {
            assert_eq!(validate_file_name(name).unwrap(), name);
        }
    }

    #[test]
    fn validate_rejects_traversal_and_reserved_names() {
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            "CON",
            "con.txt",
            "LPT1",
            "trail.",
        ] {
            assert!(
                validate_file_name(name).is_err(),
                "expected {name:?} to be rejected"
            );
        }
    }

    #[test]
    fn validate_rejects_control_characters_and_nul() {
        assert!(validate_file_name("line\nbreak").is_err());
        assert!(validate_file_name("nul\u{0}byte").is_err());
        assert!(validate_file_name(&"x".repeat(256)).is_err());
    }

    #[test]
    fn file_errors_map_to_stable_codes() {
        assert_eq!(FileError::SourceChanged.code(), ErrorCode::SourceChanged);
        assert_eq!(FileError::InvalidHandle.code(), ErrorCode::Transfer);
        assert_eq!(
            FileError::Io(std::io::Error::new(std::io::ErrorKind::StorageFull, "full")).code(),
            ErrorCode::DiskFull
        );
        assert_eq!(
            FileError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied"
            ))
            .code(),
            ErrorCode::FileAccess
        );
        let converted: Error = FileError::InvalidName("bad".into()).into();
        assert_eq!(converted.code_number, 3001);
        assert!(!converted.retryable);
    }

    #[test]
    fn source_identity_compares_size_and_time() {
        let first = SourceIdentity {
            size: 10,
            modified_unix_ms: Some(5),
        };
        let same = SourceIdentity {
            size: 10,
            modified_unix_ms: Some(5),
        };
        let changed = SourceIdentity {
            size: 11,
            modified_unix_ms: Some(5),
        };
        assert!(first.matches(same));
        assert!(!first.matches(changed));
    }
}
