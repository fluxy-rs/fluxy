//! Folder batches: one batch id aggregating many single-file transfers.
//!
//! A folder is described by the sender as a list of entries relative to the
//! batch root, and the receiver rebuilds that structure under its own root.
//! Every relative path therefore has to be provably safe before anything is
//! created on disk: an absolute path, a parent segment, a NUL, a control
//! character, a Windows alternate data stream, a reserved device name, a
//! trailing dot or space, an empty or over-long segment, and a duplicate
//! (case-insensitively, because Windows is) are all refused here rather than
//! sanitised later. Each sub-transfer keeps its own manifest and checkpoint,
//! so a batch reuses the single-file resume path instead of inventing one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorCode};

/// Directory under the app data root that holds one JSON file per batch.
pub const BATCH_DIR: &str = "batches";
/// A batch may carry at most this many entries (DESIGN limits).
pub const MAX_BATCH_ENTRIES: usize = 10_000;
/// Longest accepted relative path, in bytes.
pub const MAX_RELATIVE_PATH_BYTES: usize = 4096;
/// Longest accepted single path segment, in bytes.
pub const MAX_SEGMENT_BYTES: usize = 255;
/// Root name used when a sender supplies an unusable one.
pub const DEFAULT_ROOT_NAME: &str = "folder";

/// Windows reserved device names, matched on the stem before the extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a batch or one of its paths was refused.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BatchError {
    #[error("the folder has {0} entries, more than the {MAX_BATCH_ENTRIES} allowed")]
    TooManyEntries(usize),
    #[error("relative path {path:?} is unsafe: {reason}")]
    InvalidPath { path: String, reason: String },
    #[error("relative path {0:?} appears more than once")]
    DuplicatePath(String),
    #[error("{path:?} is both a file and a directory in this batch")]
    PathKindConflict { path: String },
    #[error("the batch metadata is not valid: {0}")]
    Corrupt(String),
    #[error("{0:?} is not a directory")]
    RootNotDirectory(String),
    #[error("symbolic links are not transferable: {0:?}")]
    LinkNotAllowed(String),
    #[error("cannot read {path:?}: {message}")]
    Unreadable { path: String, message: String },
}

impl BatchError {
    fn invalid(path: &str, reason: impl Into<String>) -> Self {
        Self::InvalidPath {
            path: path.to_string(),
            reason: reason.into(),
        }
    }

    /// A refused batch is a file-access failure, which is what the receiver
    /// answers with so the sender sees a stable 3003.
    pub fn code(&self) -> ErrorCode {
        ErrorCode::FileAccess
    }
}

impl From<BatchError> for Error {
    fn from(value: BatchError) -> Self {
        Error::new(value.code(), value.to_string(), false)
    }
}

/// One entry of a batch: a file, or a directory that is worth creating even
/// though no file lives in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchEntry {
    /// Path relative to the batch root, always "/"-separated, no leading or
    /// trailing separator.
    pub relative_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub is_dir: bool,
}

impl BatchEntry {
    pub fn file(relative_path: impl Into<String>, size: u64, mtime_ms: i64) -> Self {
        Self {
            relative_path: relative_path.into(),
            size,
            mtime_ms,
            is_dir: false,
        }
    }

    pub fn directory(relative_path: impl Into<String>, mtime_ms: i64) -> Self {
        Self {
            relative_path: relative_path.into(),
            size: 0,
            mtime_ms,
            is_dir: true,
        }
    }
}

/// A whole folder, validated. Every sub-transfer is a single file that carries
/// the batch id and its own relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchPlan {
    pub schema: u32,
    pub batch_id: Uuid,
    pub root_name: String,
    pub entries: Vec<BatchEntry>,
}

impl BatchPlan {
    /// Schema version written by this build.
    pub const SCHEMA: u32 = 1;

    /// Validate a sender's entry list. Entries are sorted by path so a batch
    /// that arrives out of order still produces a stable plan.
    pub fn new(
        batch_id: Uuid,
        root_name: impl Into<String>,
        mut entries: Vec<BatchEntry>,
    ) -> Result<Self, BatchError> {
        if entries.len() > MAX_BATCH_ENTRIES {
            return Err(BatchError::TooManyEntries(entries.len()));
        }
        let mut seen: HashSet<String> = HashSet::with_capacity(entries.len());
        let mut kinds: HashMap<String, bool> = HashMap::with_capacity(entries.len());
        for entry in &entries {
            validate_relative_path(&entry.relative_path)?;
            if entry.is_dir && entry.size != 0 {
                return Err(BatchError::invalid(
                    &entry.relative_path,
                    "a directory entry carries no content",
                ));
            }
            let key = entry.relative_path.to_lowercase();
            if let Some(existing_dir) = kinds.get(&key)
                && *existing_dir != entry.is_dir
            {
                return Err(BatchError::PathKindConflict {
                    path: entry.relative_path.clone(),
                });
            }
            if !seen.insert(key.clone()) {
                return Err(BatchError::DuplicatePath(entry.relative_path.clone()));
            }
            kinds.insert(key, entry.is_dir);
        }
        // A file cannot also be a directory that holds another entry.
        for entry in &entries {
            let mut cursor = entry.relative_path.as_str();
            while let Some((parent, _)) = cursor.rsplit_once('/') {
                if kinds.get(&parent.to_lowercase()) == Some(&false) {
                    return Err(BatchError::PathKindConflict {
                        path: parent.to_string(),
                    });
                }
                cursor = parent;
            }
        }
        entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(Self {
            schema: Self::SCHEMA,
            batch_id,
            root_name: sanitize_root_name(&root_name.into()),
            entries,
        })
    }

    pub fn total_bytes(&self) -> u64 {
        self.files().map(|entry| entry.size).sum()
    }

    pub fn file_count(&self) -> usize {
        self.files().count()
    }

    pub fn directory_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_dir).count()
    }

    pub fn files(&self) -> impl Iterator<Item = &BatchEntry> {
        self.entries.iter().filter(|entry| !entry.is_dir)
    }

    /// Directories the receiver has to create before any file lands.
    pub fn empty_directories(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(|entry| entry.is_dir)
            .map(|entry| entry.relative_path.as_str())
    }

    pub fn entry(&self, relative_path: &str) -> Option<&BatchEntry> {
        self.entries
            .iter()
            .find(|entry| entry.relative_path == relative_path)
    }

    /// Resolve a relative path under a receiver root, refusing anything that
    /// would escape it even if a segment was somehow accepted earlier.
    pub fn resolve(root: &Path, relative_path: &str) -> Result<PathBuf, BatchError> {
        validate_relative_path(relative_path)?;
        let mut path = root.to_path_buf();
        for segment in relative_path.split('/') {
            path.push(segment);
        }
        if !path.starts_with(root) {
            return Err(BatchError::invalid(
                relative_path,
                "escapes the receive root",
            ));
        }
        Ok(path)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, BatchError> {
        serde_json::to_vec(self).map_err(|error| BatchError::Corrupt(error.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, BatchError> {
        let plan: Self = serde_json::from_slice(bytes)
            .map_err(|error| BatchError::Corrupt(error.to_string()))?;
        if plan.schema != Self::SCHEMA {
            return Err(BatchError::Corrupt(format!(
                "unsupported batch schema {}",
                plan.schema
            )));
        }
        if plan.entries.len() > MAX_BATCH_ENTRIES {
            return Err(BatchError::Corrupt("entry count out of range".into()));
        }
        Ok(plan)
    }
}

/// Validate one "/"-separated relative path.
pub fn validate_relative_path(path: &str) -> Result<(), BatchError> {
    if path.is_empty() {
        return Err(BatchError::invalid(path, "the path is empty"));
    }
    if path.len() > MAX_RELATIVE_PATH_BYTES {
        return Err(BatchError::invalid(path, "the path is too long"));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(BatchError::invalid(path, "absolute paths are refused"));
    }
    if path.ends_with('/') || path.ends_with('\\') {
        return Err(BatchError::invalid(path, "the path ends with a separator"));
    }
    if path.contains('\\') {
        return Err(BatchError::invalid(path, "use \"/\" as the only separator"));
    }
    if path.contains('\0') {
        return Err(BatchError::invalid(path, "the path contains a NUL"));
    }
    if path.contains(':') {
        return Err(BatchError::invalid(
            path,
            "the path contains a colon (drive or stream)",
        ));
    }
    if path.chars().any(is_unrenderable) {
        return Err(BatchError::invalid(
            path,
            "the path contains a control or bidi character",
        ));
    }
    for segment in path.split('/') {
        validate_segment(path, segment)?;
    }
    Ok(())
}

/// Control characters and the bidirectional overrides that make a name render
/// as something other than its bytes.
fn is_unrenderable(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

fn validate_segment(path: &str, segment: &str) -> Result<(), BatchError> {
    if segment.is_empty() {
        return Err(BatchError::invalid(path, "the path has an empty segment"));
    }
    if segment == "." || segment == ".." {
        return Err(BatchError::invalid(
            path,
            "the path has a traversal segment",
        ));
    }
    if segment.len() > MAX_SEGMENT_BYTES {
        return Err(BatchError::invalid(path, "a segment is too long"));
    }
    if segment.ends_with('.') || segment.ends_with(' ') {
        return Err(BatchError::invalid(
            path,
            "a segment ends with a dot or space",
        ));
    }
    let stem = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(BatchError::invalid(
            path,
            "a segment is a reserved device name",
        ));
    }
    Ok(())
}

/// Turn a sender-supplied root name into a single safe directory name.
pub fn sanitize_root_name(name: &str) -> String {
    let mut cleaned = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_control() || matches!(character, '/' | '\\' | ':' | '\0') {
            continue;
        }
        cleaned.push(character);
    }
    let cleaned = cleaned
        .trim()
        .trim_end_matches('.')
        .trim_start_matches('.')
        .trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return DEFAULT_ROOT_NAME.to_string();
    }
    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return DEFAULT_ROOT_NAME.to_string();
    }
    truncate_bytes(cleaned, MAX_SEGMENT_BYTES)
}

/// Cut a string at a character boundary so it fits a byte budget.
fn truncate_bytes(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(entries: Vec<BatchEntry>) -> Result<BatchPlan, BatchError> {
        BatchPlan::new(Uuid::now_v7(), "holiday", entries)
    }

    #[test]
    fn accepted_paths_are_portable_and_nested() {
        for path in ["a.txt", "dir/a.txt", "a/b/c/d/e/file.bin", "photo 1.png"] {
            validate_relative_path(path)
                .unwrap_or_else(|error| panic!("expected {path:?} to be accepted, got {error}"));
        }
    }

    #[test]
    fn unicode_paths_survive_byte_and_segment_limits() {
        let path = "文档/图片/报告 2026.txt";
        assert_eq!(validate_relative_path(path), Ok(()));
        // A segment is measured in bytes, so 86 three-byte characters do not
        // fit in the 255-byte budget while 84 still do.
        let long = "文".repeat(86);
        assert!(validate_relative_path(&long).is_err());
        let fits = "文".repeat(84);
        assert_eq!(validate_relative_path(&fits), Ok(()));
    }

    #[test]
    fn absolute_and_traversal_paths_are_refused() {
        for path in [
            "/etc/passwd",
            "\\server\\share\\x",
            "C:/Windows/x",
            "..",
            "../escape",
            "a/../b",
            "a/./b",
            "./a",
        ] {
            assert!(
                validate_relative_path(path).is_err(),
                "expected {path:?} to be refused"
            );
        }
    }

    #[test]
    fn platform_hostile_paths_are_refused() {
        for path in [
            "",
            "a//b",
            "a/",
            "a\\b",
            "a\u{0}b",
            "a\u{1}b",
            "a\u{202e}b",
            "stream:ads",
            "CON",
            "con.txt",
            "a/NUL/b.txt",
            "a/trail./b",
            "a/trail /b",
        ] {
            assert!(
                validate_relative_path(path).is_err(),
                "expected {path:?} to be refused"
            );
        }
        let deep = "a/".repeat(MAX_RELATIVE_PATH_BYTES);
        assert!(validate_relative_path(&deep).is_err());
    }

    #[test]
    fn a_plan_refuses_duplicates_ignoring_case() {
        let duplicate = plan(vec![
            BatchEntry::file("Photo.JPG", 10, 0),
            BatchEntry::file("photo.jpg", 10, 0),
        ]);
        assert!(matches!(duplicate, Err(BatchError::DuplicatePath(_))));
    }

    #[test]
    fn a_plan_refuses_a_path_that_is_both_file_and_directory() {
        let conflict = plan(vec![
            BatchEntry::file("docs", 3, 0),
            BatchEntry::file("docs/a.txt", 3, 0),
        ]);
        assert!(matches!(conflict, Err(BatchError::PathKindConflict { .. })));

        let same = plan(vec![
            BatchEntry::directory("docs", 0),
            BatchEntry::file("docs", 3, 0),
        ]);
        assert!(matches!(same, Err(BatchError::PathKindConflict { .. })));
    }

    #[test]
    fn an_empty_plan_is_valid_but_an_oversized_one_is_not() {
        // An empty folder still produces a batch: the receiver creates the root
        // directory even though no file travels.
        let empty = plan(Vec::new()).unwrap();
        assert_eq!(empty.file_count(), 0);
        assert_eq!(empty.total_bytes(), 0);
        let many = (0..=MAX_BATCH_ENTRIES)
            .map(|index| BatchEntry::file(format!("f{index}.bin"), 1, 0))
            .collect::<Vec<_>>();
        assert!(matches!(
            plan(many),
            Err(BatchError::TooManyEntries(count)) if count == MAX_BATCH_ENTRIES + 1
        ));
    }

    #[test]
    fn a_plan_sorts_entries_and_reports_its_totals() {
        let plan = plan(vec![
            BatchEntry::file("z/last.bin", 30, 0),
            BatchEntry::directory("empty", 0),
            BatchEntry::file("a/first.bin", 12, 0),
        ])
        .unwrap();
        assert_eq!(
            plan.entries
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["a/first.bin", "empty", "z/last.bin"]
        );
        assert_eq!(plan.total_bytes(), 42);
        assert_eq!(plan.file_count(), 2);
        assert_eq!(plan.directory_count(), 1);
        assert_eq!(plan.empty_directories().collect::<Vec<_>>(), vec!["empty"]);
        assert!(plan.entry("a/first.bin").is_some());
        assert!(plan.entry("missing.bin").is_none());
    }

    #[test]
    fn batch_metadata_round_trips_and_rejects_a_foreign_schema() {
        let plan = plan(vec![BatchEntry::file("a/b.bin", 7, 1_700_000_000_000)]).unwrap();
        let bytes = plan.to_json().unwrap();
        assert_eq!(BatchPlan::from_json(&bytes).unwrap(), plan);

        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["schema"] = serde_json::json!(99);
        let foreign = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            BatchPlan::from_json(&foreign),
            Err(BatchError::Corrupt(_))
        ));
        assert!(matches!(
            BatchPlan::from_json(b"not json"),
            Err(BatchError::Corrupt(_))
        ));
    }

    #[test]
    fn resolving_stays_under_the_receive_root() {
        let root = Path::new("received");
        let path = BatchPlan::resolve(root, "a/b.bin").unwrap();
        assert!(path.starts_with(root));
        assert!(path.ends_with("b.bin"));
        assert!(BatchPlan::resolve(root, "../escape.bin").is_err());
        assert!(BatchPlan::resolve(root, "/etc/passwd").is_err());
    }

    #[test]
    fn root_names_are_reduced_to_one_safe_segment() {
        assert_eq!(sanitize_root_name("holiday"), "holiday");
        assert_eq!(sanitize_root_name("2026 假期"), "2026 假期");
        assert_eq!(sanitize_root_name("../etc"), "etc");
        assert_eq!(sanitize_root_name("a/b\\c:d"), "abcd");
        assert_eq!(sanitize_root_name(""), DEFAULT_ROOT_NAME);
        assert_eq!(sanitize_root_name(".."), DEFAULT_ROOT_NAME);
        assert_eq!(sanitize_root_name("CON"), DEFAULT_ROOT_NAME);
        assert_eq!(sanitize_root_name("trailing."), "trailing");
        let long = "x".repeat(400);
        assert_eq!(sanitize_root_name(&long).len(), MAX_SEGMENT_BYTES);
        let unicode = "文".repeat(120);
        let cleaned = sanitize_root_name(&unicode);
        assert!(cleaned.len() <= MAX_SEGMENT_BYTES);
        assert!(unicode.starts_with(&cleaned));
    }
}
