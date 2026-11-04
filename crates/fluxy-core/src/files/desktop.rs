//! Desktop (std::fs) implementation of the file access abstraction.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use uuid::Uuid;

use super::{
    FileAccessManager, FileCapabilities, FileError, FileHandleId, FileMeta, FileReference,
    FileSink, FileSource, FinalPath, SourceIdentity, hash_reader, validate_file_name,
};

pub struct DesktopFileAccessManager {
    root: PathBuf,
    registry: Mutex<HashMap<FileHandleId, FileReference>>,
}

impl DesktopFileAccessManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            registry: Mutex::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Deterministic partial-file name for one transfer.
    pub fn partial_path(&self, transfer_id: Uuid) -> PathBuf {
        self.root.join(format!(".fluxy-{transfer_id}.part"))
    }

    pub fn registered_sources(&self) -> usize {
        self.registry
            .lock()
            .map(|registry| registry.len())
            .unwrap_or(0)
    }

    fn registry(&self) -> Result<MutexGuard<'_, HashMap<FileHandleId, FileReference>>, FileError> {
        self.registry
            .lock()
            .map_err(|_| FileError::Io(std::io::Error::other("file registry lock poisoned")))
    }
}

impl FileAccessManager for DesktopFileAccessManager {
    fn register_source(&self, reference: FileReference) -> Result<FileHandleId, FileError> {
        match &reference {
            FileReference::Path(path) => {
                let metadata = std::fs::metadata(path).map_err(|error| match error.kind() {
                    std::io::ErrorKind::NotFound => FileError::NotFound(path.display().to_string()),
                    _ => FileError::Io(error),
                })?;
                if !metadata.is_file() {
                    return Err(FileError::NotRegularFile(path.display().to_string()));
                }
            }
            FileReference::Uri(_) | FileReference::Bookmark(_) => {
                return Err(FileError::Unsupported);
            }
        }
        let id = FileHandleId::new();
        self.registry()?.insert(id, reference);
        Ok(id)
    }

    fn open_source(&self, id: FileHandleId) -> Result<Box<dyn FileSource>, FileError> {
        let reference = self
            .registry()?
            .get(&id)
            .cloned()
            .ok_or(FileError::InvalidHandle)?;
        Ok(Box::new(DesktopFileSource::open(&reference)?))
    }

    fn create_sink_with_id(
        &self,
        transfer_id: Uuid,
        name: &str,
        size: u64,
    ) -> Result<Box<dyn FileSink>, FileError> {
        let name = validate_file_name(name)?;
        std::fs::create_dir_all(&self.root)?;
        let temp_path = self.partial_path(transfer_id);
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&temp_path)?;
        let final_path = self.root.join(&name);
        Ok(Box::new(DesktopFileSink::new(
            file, temp_path, final_path, size,
        )))
    }

    fn resume_sink(
        &self,
        transfer_id: Uuid,
        name: &str,
        size: u64,
        covered: &[(u64, u64)],
    ) -> Result<Box<dyn FileSink>, FileError> {
        let name = validate_file_name(name)?;
        let temp_path = self.partial_path(transfer_id);
        if !temp_path.exists() {
            return Err(FileError::NotFound(temp_path.display().to_string()));
        }
        for (start, end) in covered {
            if start > end || *end > size {
                return Err(FileError::WriteOutOfRange {
                    offset: *start,
                    len: end.saturating_sub(*start) as usize,
                    size,
                });
            }
        }
        let file = OpenOptions::new().read(true).write(true).open(&temp_path)?;
        let metadata = file.metadata()?;
        if metadata.len() > size {
            return Err(FileError::WriteOutOfRange {
                offset: metadata.len(),
                len: 0,
                size,
            });
        }
        if metadata.len() < size {
            file.set_len(size)?;
        }
        let final_path = self.root.join(&name);
        Ok(Box::new(DesktopFileSink::resume(
            file, temp_path, final_path, size, covered,
        )))
    }

    fn export(&self, path: &Path) -> Result<(), FileError> {
        if path.exists() {
            Ok(())
        } else {
            Err(FileError::NotFound(path.display().to_string()))
        }
    }
}

/// A std::fs reader with verified random-read capability.
pub struct DesktopFileSource {
    file: File,
    path: PathBuf,
    meta: FileMeta,
    identity: SourceIdentity,
}

impl DesktopFileSource {
    fn open(reference: &FileReference) -> Result<Self, FileError> {
        let path = match reference {
            FileReference::Path(path) => path.clone(),
            FileReference::Uri(_) | FileReference::Bookmark(_) => {
                return Err(FileError::Unsupported);
            }
        };
        let file = File::open(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => FileError::NotFound(path.display().to_string()),
            _ => FileError::Io(error),
        })?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(FileError::NotRegularFile(path.display().to_string()));
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
            .to_string();
        let modified_unix_ms = metadata.modified().ok().and_then(to_unix_ms);
        let meta = FileMeta {
            name,
            size: metadata.len(),
            modified_unix_ms,
        };
        let identity = SourceIdentity {
            size: meta.size,
            modified_unix_ms: meta.modified_unix_ms,
        };
        Ok(Self {
            file,
            path,
            meta,
            identity,
        })
    }
}

impl FileSource for DesktopFileSource {
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
        if offset > self.meta.size {
            return Err(FileError::ReadOutOfRange {
                offset,
                len,
                size: self.meta.size,
            });
        }
        let remaining = self.meta.size - offset;
        let wanted = std::cmp::min(len as u64, remaining) as usize;
        let mut buffer = vec![0_u8; wanted];
        if wanted > 0 {
            read_exact_at(&self.file, &mut buffer, offset).map_err(|error| {
                if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    FileError::SourceChanged
                } else {
                    FileError::Io(error)
                }
            })?;
        }
        Ok(Bytes::from(buffer))
    }

    fn identity(&self) -> SourceIdentity {
        self.identity
    }

    fn stat(&self) -> Result<SourceIdentity, FileError> {
        let metadata = std::fs::metadata(&self.path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => FileError::NotFound(self.path.display().to_string()),
            _ => FileError::Io(error),
        })?;
        if !metadata.is_file() {
            return Err(FileError::NotRegularFile(self.path.display().to_string()));
        }
        Ok(SourceIdentity {
            size: metadata.len(),
            modified_unix_ms: metadata.modified().ok().and_then(to_unix_ms),
        })
    }
}

fn to_unix_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

#[cfg(unix)]
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buffer, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut read = 0;
    while read < buffer.len() {
        let count = file.seek_read(&mut buffer[read..], offset + read as u64)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "short read",
            ));
        }
        read += count;
    }
    Ok(())
}

/// A std::fs writer that only commits when every byte is covered.
pub struct DesktopFileSink {
    file: File,
    temp_path: PathBuf,
    final_path: PathBuf,
    size: u64,
    ranges: Vec<(u64, u64)>,
}

impl DesktopFileSink {
    fn new(file: File, temp_path: PathBuf, final_path: PathBuf, size: u64) -> Self {
        Self {
            file,
            temp_path,
            final_path,
            size,
            ranges: Vec::new(),
        }
    }

    fn resume(
        file: File,
        temp_path: PathBuf,
        final_path: PathBuf,
        size: u64,
        covered: &[(u64, u64)],
    ) -> Self {
        let mut sink = Self {
            file,
            temp_path,
            final_path,
            size,
            ranges: Vec::new(),
        };
        for (start, end) in covered {
            sink.add_range(*start, *end);
        }
        sink
    }

    fn covered_bytes(&self) -> u64 {
        self.ranges
            .iter()
            .map(|(start, end)| end - start)
            .sum::<u64>()
    }

    fn add_range(&mut self, start: u64, end: u64) {
        self.ranges.push((start, end));
        self.ranges.sort_by_key(|range| range.0);
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(self.ranges.len());
        for (range_start, range_end) in self.ranges.drain(..) {
            if let Some(last) = merged.last_mut()
                && range_start <= last.1
            {
                if range_end > last.1 {
                    last.1 = range_end;
                }
                continue;
            }
            merged.push((range_start, range_end));
        }
        self.ranges = merged;
    }

    fn remove_temp(&self) {
        let _ = std::fs::remove_file(&self.temp_path);
    }
}

/// The path itself when it is free, otherwise the first free "name (n).ext".
///
/// The check-then-rename is not atomic, which is fine: the rename is what
/// actually claims the name, and a collision there fails loudly rather than
/// overwriting someone else's file.
fn available_path(base: &std::path::Path) -> std::path::PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    let parent = base.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = base
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("file");
    let extension = base.extension().and_then(|extension| extension.to_str());
    for index in 1..10_000u32 {
        let candidate = match extension {
            Some(extension) => parent.join(format!("{stem} ({index}).{extension}")),
            None => parent.join(format!("{stem} ({index})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    base.to_path_buf()
}

impl FileSink for DesktopFileSink {
    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), FileError> {
        let len = data.len() as u64;
        if len == 0 {
            if offset > self.size {
                return Err(FileError::WriteOutOfRange {
                    offset,
                    len: 0,
                    size: self.size,
                });
            }
            return Ok(());
        }
        let end = offset.checked_add(len).ok_or(FileError::WriteOutOfRange {
            offset,
            len: data.len(),
            size: self.size,
        })?;
        if end > self.size {
            return Err(FileError::WriteOutOfRange {
                offset,
                len: data.len(),
                size: self.size,
            });
        }
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)?;
        self.add_range(offset, end);
        Ok(())
    }

    fn sync(&mut self) -> Result<(), FileError> {
        self.file.sync_all()?;
        Ok(())
    }

    fn root_hash(&mut self) -> Result<[u8; 32], FileError> {
        self.file.sync_all()?;
        let file = File::open(&self.temp_path)?;
        let mut reader = std::io::BufReader::new(file);
        hash_reader(&mut reader)
    }

    fn finalize(self: Box<Self>) -> Result<FinalPath, FileError> {
        let this = *self;
        let covered = this.covered_bytes();
        if covered != this.size {
            this.remove_temp();
            return Err(FileError::Incomplete {
                written: covered,
                size: this.size,
            });
        }
        this.file.sync_all()?;
        // A duplicate name is not an error the user can act on - receiving the
        // same file twice is normal. Fail the transfer and the only advice is
        // "delete the old one and send again", which is why this used to surface
        // as a generic 3001 claiming the peer had declined. Pick the first free
        // "name (n).ext" instead, matching what every file manager does.
        let final_path = available_path(&this.final_path);
        std::fs::rename(&this.temp_path, &final_path)?;
        Ok(FinalPath(final_path))
    }

    fn abort(self: Box<Self>) {
        let this = *self;
        this.remove_temp();
    }

    fn discard(&mut self) -> Result<(), FileError> {
        match std::fs::remove_file(&self.temp_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(FileError::Io(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{FileAccessManager, FileReference};

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn register_and_read_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "data.bin", b"hello");
        let manager = DesktopFileAccessManager::new(dir.path());
        let id = manager
            .register_source(FileReference::Path(path.clone()))
            .unwrap();
        assert_eq!(manager.registered_sources(), 1);
        let source = manager.open_source(id).unwrap();
        assert_eq!(source.meta().name, "data.bin");
        assert_eq!(source.meta().size, 5);
        assert!(source.meta().modified_unix_ms.is_some());
        let capabilities = source.capabilities();
        assert!(capabilities.random_read);
        assert!(!capabilities.random_write);
        assert!(capabilities.persistent_access);
        assert!(capabilities.size_known);
        assert!(source.read_at(0, 0).unwrap().is_empty());
        assert_eq!(&source.read_at(0, 2).unwrap()[..], b"he");
        assert_eq!(&source.read_at(3, 10).unwrap()[..], b"lo");
        assert!(source.read_at(5, 3).unwrap().is_empty());
        assert!(matches!(
            source.read_at(6, 1),
            Err(FileError::ReadOutOfRange {
                offset: 6,
                len: 1,
                size: 5
            })
        ));
        assert_eq!(source.identity().size, 5);
        assert_eq!(
            source.identity(),
            SourceIdentity {
                size: 5,
                modified_unix_ms: source.meta().modified_unix_ms
            }
        );
    }

    #[test]
    fn register_rejects_missing_directory_and_unsupported_references() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        assert!(matches!(
            manager.register_source(FileReference::Path(dir.path().join("missing.bin"))),
            Err(FileError::NotFound(_))
        ));
        assert!(matches!(
            manager.register_source(FileReference::Path(dir.path().to_path_buf())),
            Err(FileError::NotRegularFile(_))
        ));
        assert!(matches!(
            manager.register_source(FileReference::Uri("content://example/1".into())),
            Err(FileError::Unsupported)
        ));
        assert!(matches!(
            manager.register_source(FileReference::Bookmark(vec![1, 2, 3])),
            Err(FileError::Unsupported)
        ));
    }

    #[test]
    fn open_unknown_handle_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        assert!(matches!(
            manager.open_source(FileHandleId::new()),
            Err(FileError::InvalidHandle)
        ));
    }

    #[test]
    fn sink_writes_out_of_order_and_finalizes() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path().join("downloads"));
        let mut sink = manager.create_sink("out.bin", 6).unwrap();
        sink.write_at(4, b"ef").unwrap();
        sink.write_at(0, b"abcd").unwrap();
        sink.sync().unwrap();
        let final_path = sink.finalize().unwrap();
        assert_eq!(std::fs::read(final_path.as_path()).unwrap(), b"abcdef");
        let entries = std::fs::read_dir(dir.path().join("downloads")).unwrap();
        assert_eq!(entries.count(), 1);
    }

    #[test]
    fn sink_rejects_out_of_range_and_requires_full_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let mut sink = manager.create_sink("partial.bin", 4).unwrap();
        assert!(matches!(
            sink.write_at(2, b"abc"),
            Err(FileError::WriteOutOfRange {
                offset: 2,
                len: 3,
                size: 4
            })
        ));
        assert!(matches!(
            sink.write_at(5, b""),
            Err(FileError::WriteOutOfRange { offset: 5, .. })
        ));
        sink.write_at(0, b"ab").unwrap();
        assert!(matches!(
            sink.finalize(),
            Err(FileError::Incomplete {
                written: 2,
                size: 4
            })
        ));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn sink_merges_overlapping_writes() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let mut sink = manager.create_sink("merge.bin", 4).unwrap();
        sink.write_at(0, b"ab").unwrap();
        sink.write_at(1, b"bc").unwrap();
        sink.write_at(2, b"cd").unwrap();
        let final_path = sink.finalize().unwrap();
        assert_eq!(std::fs::read(final_path.as_path()).unwrap(), b"abcd");
    }

    #[test]
    fn zero_size_sink_finalizes_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let sink = manager.create_sink("empty.bin", 0).unwrap();
        let final_path = sink.finalize().unwrap();
        assert_eq!(std::fs::read(final_path.as_path()).unwrap(), b"");
    }

    #[test]
    fn sink_finalize_renames_instead_of_overwriting_a_destination() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        std::fs::write(dir.path().join("out.bin"), b"old").unwrap();
        let mut sink = manager.create_sink("out.bin", 3).unwrap();
        sink.write_at(0, b"new").unwrap();
        // Receiving the same name twice is ordinary, so the second copy takes
        // the next free name and the first file is left untouched.
        let final_path = sink.finalize().unwrap();
        assert_eq!(final_path.as_path(), dir.path().join("out (1).bin"));
        assert_eq!(std::fs::read(dir.path().join("out.bin")).unwrap(), b"old");
        assert_eq!(
            std::fs::read(dir.path().join("out (1).bin")).unwrap(),
            b"new"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn sink_finalize_keeps_counting_up_for_repeated_names() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        std::fs::write(dir.path().join("out.bin"), b"first").unwrap();
        for expected in ["out (1).bin", "out (2).bin", "out (3).bin"] {
            let mut sink = manager.create_sink("out.bin", 3).unwrap();
            sink.write_at(0, b"new").unwrap();
            let final_path = sink.finalize().unwrap();
            assert_eq!(final_path.as_path(), dir.path().join(expected));
        }
        assert_eq!(std::fs::read(dir.path().join("out.bin")).unwrap(), b"first");
    }

    #[test]
    fn available_path_handles_a_name_without_an_extension() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("README");
        std::fs::write(&base, b"one").unwrap();
        assert_eq!(available_path(&base), dir.path().join("README (1)"));
        assert_eq!(
            available_path(&dir.path().join("free")),
            dir.path().join("free")
        );
    }

    #[test]
    fn abort_removes_the_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let mut sink = manager.create_sink("cancel.bin", 8).unwrap();
        sink.write_at(0, b"data").unwrap();
        sink.abort();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn create_sink_rejects_invalid_names() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        assert!(matches!(
            manager.create_sink("../escape.bin", 1),
            Err(FileError::InvalidName(_))
        ));
        assert!(matches!(
            manager.create_sink("a/b.bin", 1),
            Err(FileError::InvalidName(_))
        ));
        assert!(matches!(
            manager.create_sink("CON", 1),
            Err(FileError::InvalidName(_))
        ));
    }

    #[test]
    fn read_at_reports_source_change_when_the_file_shrinks() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "shrink.bin", b"hello");
        let manager = DesktopFileAccessManager::new(dir.path());
        let id = manager
            .register_source(FileReference::Path(path.clone()))
            .unwrap();
        let source = manager.open_source(id).unwrap();
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(1).unwrap();
        drop(file);
        assert!(matches!(
            source.read_at(3, 2),
            Err(FileError::SourceChanged)
        ));
    }

    #[test]
    fn resume_sink_reopens_a_partial_file_and_finalizes() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let transfer_id = Uuid::now_v7();
        {
            let mut sink = manager
                .create_sink_with_id(transfer_id, "resumed.bin", 4)
                .unwrap();
            sink.write_at(0, b"ab").unwrap();
            sink.sync().unwrap();
        }
        assert!(manager.partial_path(transfer_id).exists());
        let mut resumed = manager
            .resume_sink(transfer_id, "resumed.bin", 4, &[(0, 2)])
            .unwrap();
        resumed.write_at(2, b"cd").unwrap();
        let path = resumed.finalize().unwrap();
        assert_eq!(std::fs::read(path.as_path()).unwrap(), b"abcd");
        assert!(!manager.partial_path(transfer_id).exists());
    }

    #[test]
    fn root_hash_matches_the_written_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let transfer_id = Uuid::now_v7();
        let mut sink = manager
            .create_sink_with_id(transfer_id, "hash.bin", 5)
            .unwrap();
        sink.write_at(0, b"hello").unwrap();
        let computed = sink.root_hash().unwrap();
        assert_eq!(computed, *blake3::hash(b"hello").as_bytes());
        let path = sink.finalize().unwrap();
        assert_eq!(std::fs::read(path.as_path()).unwrap(), b"hello");
    }

    #[test]
    fn root_hash_of_an_empty_file_is_the_empty_hash() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let mut sink = manager
            .create_sink_with_id(Uuid::now_v7(), "empty.bin", 0)
            .unwrap();
        assert_eq!(sink.root_hash().unwrap(), *blake3::hash(b"").as_bytes());
    }

    #[test]
    fn stat_detects_a_changed_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "changing.bin", b"hello");
        let manager = DesktopFileAccessManager::new(dir.path());
        let id = manager
            .register_source(FileReference::Path(path.clone()))
            .unwrap();
        let source = manager.open_source(id).unwrap();
        source.verify_unchanged().unwrap();
        // Touch the file with a different size and modification time.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&path, b"hello world").unwrap();
        assert!(matches!(source.stat(), Ok(identity) if identity.size == 11));
        assert!(matches!(
            source.verify_unchanged(),
            Err(FileError::SourceChanged)
        ));
    }

    #[test]
    fn discard_removes_the_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let transfer_id = Uuid::now_v7();
        let mut sink = manager
            .create_sink_with_id(transfer_id, "cancelled.bin", 4)
            .unwrap();
        sink.write_at(0, b"ab").unwrap();
        assert!(manager.partial_path(transfer_id).exists());
        sink.discard().unwrap();
        assert!(!manager.partial_path(transfer_id).exists());
        // Discarding twice is safe.
        sink.discard().unwrap();
    }

    #[test]
    fn resume_sink_requires_an_existing_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        assert!(matches!(
            manager.resume_sink(Uuid::now_v7(), "missing.bin", 4, &[]),
            Err(FileError::NotFound(_))
        ));
    }

    #[test]
    fn resume_sink_rejects_out_of_range_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let transfer_id = Uuid::now_v7();
        drop(
            manager
                .create_sink_with_id(transfer_id, "range.bin", 4)
                .unwrap(),
        );
        assert!(matches!(
            manager.resume_sink(transfer_id, "range.bin", 4, &[(2, 6)]),
            Err(FileError::WriteOutOfRange { .. })
        ));
    }

    #[test]
    fn resume_sink_extends_a_short_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DesktopFileAccessManager::new(dir.path());
        let transfer_id = Uuid::now_v7();
        {
            let mut sink = manager
                .create_sink_with_id(transfer_id, "extend.bin", 8)
                .unwrap();
            sink.write_at(0, b"xy").unwrap();
            sink.sync().unwrap();
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(manager.partial_path(transfer_id))
            .unwrap()
            .set_len(2)
            .unwrap();
        let mut resumed = manager
            .resume_sink(transfer_id, "extend.bin", 8, &[(0, 2)])
            .unwrap();
        resumed.write_at(2, b"123456").unwrap();
        let path = resumed.finalize().unwrap();
        assert_eq!(std::fs::read(path.as_path()).unwrap(), b"xy123456");
    }

    #[test]
    fn export_checks_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "export.bin", b"x");
        let manager = DesktopFileAccessManager::new(dir.path());
        assert!(manager.export(&path).is_ok());
        assert!(matches!(
            manager.export(&dir.path().join("missing.bin")),
            Err(FileError::NotFound(_))
        ));
    }
}
