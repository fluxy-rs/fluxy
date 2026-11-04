use std::path::{Path, PathBuf};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

use crate::Error;

/// Newest log file name; the rotating appender appends a date to it.
pub const LOG_PREFIX: &str = "fluxy";

/// Keeps the non-blocking writer's worker alive for the process lifetime.
///
/// Dropping the guard stops the writer thread and the last lines are lost, so it
/// is leaked deliberately: the log must outlive every runtime it describes.
static GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

/// Where the file appender writes; set once at startup.
static LOG_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(log_dir: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(log_dir)?;
    let _ = LOG_DIR.set(log_dir.to_path_buf());
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Always mirror to stderr (that is what `tauri dev` and logcat show), and
    // additionally keep a daily file so a release build - where nothing is
    // attached to the process - can still be inspected from the settings page.
    let file = tracing_appender::rolling::daily(log_dir, format!("{LOG_PREFIX}.log"));
    let (file, guard) = tracing_appender::non_blocking(file);
    let _ = GUARD.set(guard);

    // The same writer feeds stderr and the file, so ANSI colour has to go:
    // escape codes are noise in logcat and unreadable in the settings panel.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::io::stderr.and(file))
        .try_init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_dir = %log_dir.display(),
        "fluxy starting"
    );
    Ok(())
}

/// Newest log file in `log_dir`, if any.
pub fn newest_file(log_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(log_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(LOG_PREFIX))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
        })
}

/// Last `limit` lines of the log written by this process's appender.
pub fn tail_recent(limit: usize) -> Result<Vec<String>, Error> {
    match LOG_DIR.get() {
        Some(dir) => tail(dir, limit),
        None => Ok(Vec::new()),
    }
}

/// Last `limit` lines of the newest log file, oldest first.
pub fn tail(log_dir: &Path, limit: usize) -> Result<Vec<String>, Error> {
    let Some(path) = newest_file(log_dir) else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(&path)?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..]
        .iter()
        .map(|line| (*line).to_string())
        .collect())
}
