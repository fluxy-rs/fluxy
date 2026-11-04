//! Recursive folder scanning for folder batches.
//!
//! The sender walks the folder once, without following symbolic links, and
//! produces the entry list the batch plan validates. Files keep their size and
//! modification time; directories appear only when their subtree holds no file,
//! because a file's own path already implies its parents. Nothing here reads
//! file contents, so a 10k-file folder does not block on a pre-hash.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::transfer::batch::{
    BatchEntry, BatchError, BatchPlan, DEFAULT_ROOT_NAME, MAX_BATCH_ENTRIES, sanitize_root_name,
};

/// Walk `root` and build a validated batch plan.
pub fn scan_folder(root: &Path, batch_id: Uuid) -> Result<BatchPlan, BatchError> {
    let metadata = std::fs::symlink_metadata(root).map_err(|error| BatchError::Unreadable {
        path: root.display().to_string(),
        message: error.to_string(),
    })?;
    if metadata.file_type().is_symlink() {
        return Err(BatchError::LinkNotAllowed(root.display().to_string()));
    }
    if !metadata.is_dir() {
        return Err(BatchError::RootNotDirectory(root.display().to_string()));
    }
    let root_name = sanitize_root_name(
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(DEFAULT_ROOT_NAME),
    );

    let mut files = Vec::new();
    let mut directories: Vec<String> = Vec::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = stack.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| BatchError::Unreadable {
            path: directory.display().to_string(),
            message: error.to_string(),
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| BatchError::Unreadable {
                path: directory.display().to_string(),
                message: error.to_string(),
            })?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| BatchError::Unreadable {
                path: entry.path().display().to_string(),
                message: "the name is not valid UTF-8".into(),
            })?;
            let relative = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let file_type = entry.file_type().map_err(|error| BatchError::Unreadable {
                path: entry.path().display().to_string(),
                message: error.to_string(),
            })?;
            if file_type.is_symlink() {
                return Err(BatchError::LinkNotAllowed(relative));
            }
            if files.len() + directories.len() >= MAX_BATCH_ENTRIES {
                return Err(BatchError::TooManyEntries(MAX_BATCH_ENTRIES + 1));
            }
            if file_type.is_dir() {
                directories.push(relative.clone());
                stack.push((entry.path(), relative));
            } else if file_type.is_file() {
                let metadata = entry.metadata().map_err(|error| BatchError::Unreadable {
                    path: entry.path().display().to_string(),
                    message: error.to_string(),
                })?;
                files.push(BatchEntry::file(
                    relative,
                    metadata.len(),
                    modified_ms(&metadata),
                ));
            } else {
                // A device, socket, or FIFO has no business in a folder send.
                return Err(BatchError::Unreadable {
                    path: entry.path().display().to_string(),
                    message: "not a regular file or directory".into(),
                });
            }
        }
    }

    // A directory is worth creating only when nothing under it holds a file;
    // keep the outermost such directory so the entry list stays small.
    let mut occupied: HashSet<String> = HashSet::new();
    for file in &files {
        let mut cursor = file.relative_path.as_str();
        while let Some((parent, _)) = cursor.rsplit_once('/') {
            if !occupied.insert(parent.to_string()) {
                break;
            }
            cursor = parent;
        }
    }
    let empty: Vec<String> = directories
        .into_iter()
        .filter(|directory| !occupied.contains(directory))
        .filter(|directory| {
            directory
                .rsplit_once('/')
                .map(|(parent, _)| occupied.contains(parent))
                .unwrap_or(true)
        })
        .collect();
    let mut entries = files;
    entries.extend(
        empty
            .into_iter()
            .map(|directory| BatchEntry::directory(directory, 0)),
    );
    BatchPlan::new(batch_id, root_name, entries)
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_nested_folder_produces_files_and_its_empty_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("holiday");
        write(&root.join("a.txt"), b"aaa");
        write(&root.join("photos/2026/one.jpg"), b"12345");
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::create_dir_all(root.join("photos/empty-nested")).unwrap();

        let plan = scan_folder(&root, Uuid::now_v7()).unwrap();

        assert_eq!(plan.root_name, "holiday");
        assert_eq!(plan.file_count(), 2);
        assert_eq!(plan.total_bytes(), 8);
        let empty: Vec<&str> = plan.empty_directories().collect();
        assert_eq!(empty, vec!["empty", "photos/empty-nested"]);
        assert!(plan.entry("photos/2026/one.jpg").is_some());
        assert!(plan.entry("photos/2026").is_none());
        assert!(plan.entry("photos").is_none());
    }

    #[test]
    fn unicode_names_survive_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("2026 假期");
        write(&root.join("文档/报告 一.txt"), b"x");

        let plan = scan_folder(&root, Uuid::now_v7()).unwrap();

        assert_eq!(plan.root_name, "2026 假期");
        assert_eq!(plan.file_count(), 1);
        assert!(plan.entry("文档/报告 一.txt").is_some());
    }

    #[test]
    fn a_file_root_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plain.txt");
        fs::write(&file, b"x").unwrap();
        assert!(matches!(
            scan_folder(&file, Uuid::now_v7()),
            Err(BatchError::RootNotDirectory(_))
        ));
    }

    #[test]
    fn a_symbolic_link_inside_the_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("tree");
        fs::create_dir_all(&root).unwrap();
        let target = dir.path().join("outside.txt");
        fs::write(&target, b"secret").unwrap();
        let link = root.join("link.txt");
        let created = {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &link).is_ok()
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(&target, &link).is_ok()
            }
            #[cfg(not(any(unix, windows)))]
            {
                false
            }
        };
        if !created {
            // Creating a link needs a privilege this account does not have.
            return;
        }
        let error = scan_folder(&root, Uuid::now_v7()).expect_err("a link must be refused");
        assert!(matches!(error, BatchError::LinkNotAllowed(_)));
    }

    #[test]
    fn an_empty_folder_is_a_batch_of_one_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("nothing");
        fs::create_dir_all(&root).unwrap();

        let plan = scan_folder(&root, Uuid::now_v7()).unwrap();

        assert_eq!(plan.file_count(), 0);
        assert_eq!(plan.total_bytes(), 0);
        // The root directory is created by the receiver itself, so an empty
        // folder needs no entry at all.
        assert!(plan.entries.is_empty());
        assert!(plan.empty_directories().next().is_none());
    }
}
