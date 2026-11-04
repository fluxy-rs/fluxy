//! Android file access: a descriptor handed over by the SAF picker.
//!
//! The Android side never exposes a path. The plugin opens the picked URI,
//! detaches the descriptor, and this module takes ownership of it; every read
//! is `pread` at an offset so the transfer workers never share a file cursor.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::Bytes;
use uuid::Uuid;

use super::{
    FileAccessManager, FileCapabilities, FileError, FileHandleId, FileMeta, FileReference,
    FileSink, FileSource, SourceIdentity, hash_reader, validate_file_name,
};

/// One registered source: the URI for a later reopen plus the live descriptor.
struct Registered {
    uri: String,
    file: Arc<File>,
    meta: FileMeta,
    identity: SourceIdentity,
}

/// File access backed by descriptors the platform opened for us.
pub struct AndroidFileAccessManager {
    root: std::path::PathBuf,
    registry: Mutex<HashMap<FileHandleId, Registered>>,
}

impl AndroidFileAccessManager {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            root: root.into(),
            registry: Mutex::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn registry(&self) -> Result<MutexGuard<'_, HashMap<FileHandleId, Registered>>, FileError> {
        self.registry
            .lock()
            .map_err(|_| FileError::Io(io::Error::other("file registry lock poisoned")))
    }

    /// Register a source from a descriptor the platform already opened.
    ///
    /// `name` is the display name the picker reported; a missing or oversized
    /// one is replaced with a safe fallback so the offer never carries garbage.
    pub fn register_opened(
        &self,
        uri: impl Into<String>,
        file: File,
        name: &str,
        mime: Option<String>,
    ) -> Result<(FileHandleId, FileMeta), FileError> {
        let _ = mime;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(FileError::NotRegularFile(uri.into()));
        }
        let size = metadata.len();
        let modified_unix_ms = metadata.modified().ok().and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|elapsed| elapsed.as_millis() as u64)
        });
        let fallback = format!("file-{size}");
        let name = if name.trim().is_empty() {
            fallback.as_str()
        } else {
            name.trim()
        };
        let name = validate_file_name(name).unwrap_or_else(|_| fallback.clone());
        let meta = FileMeta::new(name, size, modified_unix_ms);
        let identity = SourceIdentity {
            size,
            modified_unix_ms,
        };
        let registered = Registered {
            uri: uri.into(),
            file: Arc::new(file),
            meta: meta.clone(),
            identity,
        };
        let id = FileHandleId::new();
        self.registry()?.insert(id, registered);
        Ok((id, meta))
    }

    /// The URI a checkpoint needs to reopen this source after a restart.
    pub fn uri(&self, id: FileHandleId) -> Option<String> {
        self.registry()
            .ok()
            .and_then(|registry| registry.get(&id).map(|entry| entry.uri.clone()))
    }

    fn sink_manager(&self) -> super::DesktopFileAccessManager {
        super::DesktopFileAccessManager::new(self.root.clone())
    }

    pub fn drop_source(&self, id: FileHandleId) {
        if let Ok(mut registry) = self.registry() {
            registry.remove(&id);
        }
    }
}

impl FileAccessManager for AndroidFileAccessManager {
    fn register_descriptor(
        &self,
        uri: &str,
        file: File,
        name: &str,
        mime: Option<String>,
    ) -> Result<(FileHandleId, FileMeta), FileError> {
        self.register_opened(uri, file, name, mime)
    }

    fn register_source(&self, reference: FileReference) -> Result<FileHandleId, FileError> {
        // A URI has no descriptor until the platform opens it, which is async;
        // the facade calls register_descriptor instead of pretending here.
        match reference {
            FileReference::Uri(_) | FileReference::Path(_) | FileReference::Bookmark(_) => {
                Err(FileError::Unsupported)
            }
        }
    }

    fn open_source(&self, id: FileHandleId) -> Result<Box<dyn FileSource>, FileError> {
        let registered = self
            .registry()?
            .get(&id)
            .map(|entry| (Arc::clone(&entry.file), entry.meta.clone(), entry.identity))
            .ok_or(FileError::InvalidHandle)?;
        Ok(Box::new(AndroidFileSource {
            file: registered.0,
            meta: registered.1,
            identity: registered.2,
        }))
    }

    fn create_sink_with_id(
        &self,
        transfer_id: Uuid,
        name: &str,
        size: u64,
    ) -> Result<Box<dyn FileSink>, FileError> {
        // Receiving stays in the app sandbox, which is a real path on both
        // platforms, so the desktop sink is the right one here too.
        self.sink_manager()
            .create_sink_with_id(transfer_id, name, size)
    }

    fn resume_sink(
        &self,
        transfer_id: Uuid,
        name: &str,
        size: u64,
        covered: &[(u64, u64)],
    ) -> Result<Box<dyn FileSink>, FileError> {
        self.sink_manager()
            .resume_sink(transfer_id, name, size, covered)
    }

    fn export(&self, path: &std::path::Path) -> Result<(), FileError> {
        // Export to a user-visible location is MediaStore work (A-04); until
        // then a received file already lives in the sandbox.
        let _ = path;
        Err(FileError::Unsupported)
    }
}

/// A source whose bytes come from a platform descriptor.
pub struct AndroidFileSource {
    file: Arc<File>,
    meta: FileMeta,
    identity: SourceIdentity,
}

impl FileSource for AndroidFileSource {
    fn meta(&self) -> FileMeta {
        self.meta.clone()
    }

    fn capabilities(&self) -> FileCapabilities {
        FileCapabilities {
            random_read: true,
            random_write: false,
            persistent_access: true,
            size_known: true,
        }
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes, FileError> {
        let size = self.meta.size;
        if offset > size || offset.saturating_add(len as u64) > size {
            return Err(FileError::ReadOutOfRange { offset, len, size });
        }
        let mut buffer = vec![0u8; len];
        let read = read_at(&self.file, &mut buffer, offset)?;
        buffer.truncate(read);
        Ok(Bytes::from(buffer))
    }

    fn identity(&self) -> SourceIdentity {
        self.identity
    }

    fn stat(&self) -> Result<SourceIdentity, FileError> {
        let metadata = self.file.metadata()?;
        let modified_unix_ms = metadata.modified().ok().and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|elapsed| elapsed.as_millis() as u64)
        });
        Ok(SourceIdentity {
            size: metadata.len(),
            modified_unix_ms,
        })
    }
}

/// Positional read without moving the shared cursor.
#[cfg(unix)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

impl Drop for AndroidFileSource {
    fn drop(&mut self) {
        // The descriptor is owned by the Arc<File>; nothing else to release.
    }
}

/// Hash a whole source, used when a resume needs to confirm the bytes still
/// match the manifest.
pub fn hash_source(source: &dyn FileSource) -> Result<[u8; 32], FileError> {
    let size = source.meta().size;
    let mut reader = SourceReader {
        source,
        offset: 0,
        size,
    };
    hash_reader(&mut reader)
}

struct SourceReader<'a> {
    source: &'a dyn FileSource,
    offset: u64,
    size: u64,
}

impl io::Read for SourceReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.offset >= self.size {
            return Ok(0);
        }
        let remaining = (self.size - self.offset).min(buffer.len() as u64) as usize;
        let chunk = self
            .source
            .read_at(self.offset, remaining)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let written = chunk.len();
        buffer[..written].copy_from_slice(&chunk);
        self.offset += written as u64;
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn manager() -> (AndroidFileAccessManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (AndroidFileAccessManager::new(dir.path()), dir)
    }

    #[test]
    fn a_descriptor_registers_meta_and_reads_at_an_offset() {
        let (manager, dir) = manager();
        let path = dir.path().join("source.bin");
        let mut file = File::create(&path).unwrap();
        file.write_all(&(0..64u8).collect::<Vec<_>>()).unwrap();
        let file = File::open(&path).unwrap();

        let (id, meta) = manager
            .register_opened("content://media/1", file, "photo.bin", None)
            .unwrap();
        assert_eq!(meta.name, "photo.bin");
        assert_eq!(meta.size, 64);

        let source = manager.open_source(id).unwrap();
        assert_eq!(source.meta().size, 64);
        assert!(source.capabilities().random_read);
        let chunk = source.read_at(16, 8).unwrap();
        assert_eq!(&chunk[..], &[16u8, 17, 18, 19, 20, 21, 22, 23]);
        assert!(matches!(
            source.read_at(60, 8),
            Err(FileError::ReadOutOfRange { .. })
        ));
        assert_eq!(manager.uri(id).as_deref(), Some("content://media/1"));
    }

    #[test]
    fn a_missing_display_name_falls_back_to_a_safe_one() {
        let (manager, dir) = manager();
        let path = dir.path().join("source.bin");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"abc").unwrap();
        let file = File::open(&path).unwrap();

        let (_, meta) = manager
            .register_opened("content://media/2", file, "   ", None)
            .unwrap();
        assert_eq!(meta.name, "file-3");
    }

    #[test]
    fn a_reopened_descriptor_resumes_from_a_checkpoint_range() {
        // A restart loses the descriptor; the saved URI reopens it and the
        // checkpoint's covered range lets the sender skip those blocks.
        let (manager, dir) = manager();
        let path = dir.path().join("big.bin");
        let data: Vec<u8> = (0..4096u32).map(|value| (value % 251) as u8).collect();
        std::fs::write(&path, &data).unwrap();

        let first = std::fs::File::open(&path).unwrap();
        let (id, meta) = manager
            .register_opened("content://media/4", first, "big.bin", None)
            .unwrap();
        let source = manager.open_source(id).unwrap();
        let first_block = source.read_at(0, 1024).unwrap();
        assert_eq!(&first_block[..], &data[..1024]);
        let identity = source.identity();
        manager.drop_source(id);
        drop(source);

        // Reopen the same URI after a "restart".
        let reopened = std::fs::File::open(&path).unwrap();
        let (id, second_meta) = manager
            .register_opened("content://media/4", reopened, "big.bin", None)
            .unwrap();
        assert_eq!(second_meta, meta);
        let source = manager.open_source(id).unwrap();
        assert_eq!(source.identity(), identity, "the source did not change");
        let later = source.read_at(2048, 512).unwrap();
        assert_eq!(&later[..], &data[2048..2560]);
    }

    #[test]
    fn a_changed_source_reports_a_different_identity() {
        let (manager, dir) = manager();
        let path = dir.path().join("source.bin");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"first").unwrap();
        let file = File::open(&path).unwrap();
        let (id, _) = manager
            .register_opened("content://media/3", file, "source.bin", None)
            .unwrap();
        let source = manager.open_source(id).unwrap();
        source.verify_unchanged().unwrap();

        let mut appender = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        appender.write_all(b"-more").unwrap();
        assert!(matches!(
            source.verify_unchanged(),
            Err(FileError::SourceChanged)
        ));
    }
}
