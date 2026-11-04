//! Login-item management for the desktop shell.
//!
//! M3-04 asks for an autostart switch that is off by default and that the user
//! can flip from settings or from the tray. The implementation deliberately
//! avoids a platform plugin: Windows writes the per-user Run key through
//! `reg.exe`, macOS writes a launchd property list, and Linux writes an XDG
//! desktop entry. All three sit behind one trait, so the switch is fully
//! testable without touching the machine's real login items.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Name of the login item on every platform. It is also the value name inside
/// the Windows Run key, so it stays short.
pub const LOGIN_ITEM_NAME: &str = "Fluxy";

/// Argument that starts the shell without showing its window, so a login start
/// lands in the tray and still receives.
pub const HIDDEN_FLAG: &str = "--hidden";

/// Manages one login item.
pub trait StartupBackend: Send + Sync {
    /// Whether the login item is currently registered.
    fn is_enabled(&self) -> Result<bool, String>;
    /// Register or remove the login item that starts `exe`.
    fn set_enabled(&self, enabled: bool, exe: &Path) -> Result<(), String>;
    /// Human-readable location, used by diagnostics and the help page.
    fn location(&self) -> String;
}

/// The command line a login item runs: this app, started hidden.
pub fn login_command(exe: &Path) -> String {
    format!("\"{}\" {}", exe.display(), HIDDEN_FLAG)
}

/// The login item the current platform uses.
pub fn platform_backend() -> Arc<dyn StartupBackend> {
    #[cfg(windows)]
    {
        Arc::new(WindowsStartup::system())
    }
    #[cfg(target_os = "macos")]
    {
        Arc::new(FileStartup::macos())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Arc::new(FileStartup::linux())
    }
    #[cfg(not(any(windows, unix)))]
    {
        Arc::new(FileStartup::new(
            home_dir().join("fluxy-autostart.conf"),
            FileStartupKind::Xdg,
        ))
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Escape the characters that would break an XML text node.
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape a value for an XDG desktop-entry `Exec` line: the value is quoted and
/// the shell-significant characters are backslash-escaped.
fn desktop_exec(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        if matches!(ch, '"' | '\\' | '`' | '$') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped.push('"');
    escaped
}

/// launchd property list used by macOS.
pub fn launchd_plist(exe: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>dev.fluxy.app</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>{exe}</string>\n\
    <string>{flag}</string>\n\
  </array>\n\
  <key>RunAtLoad</key>\n\
  <true/>\n\
</dict>\n\
</plist>\n",
        exe = xml_escape(&exe.display().to_string()),
        flag = HIDDEN_FLAG
    )
}

/// XDG desktop entry used by Linux.
pub fn xdg_desktop_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
Type=Application\n\
Name={LOGIN_ITEM_NAME}\n\
Comment=Fluxy clipboard and file sharing\n\
Exec={exec} {flag}\n\
X-GNOME-Autostart-enabled=true\n",
        exec = desktop_exec(&exe.display().to_string()),
        flag = HIDDEN_FLAG
    )
}

/// Which file format a file-based login item uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStartupKind {
    Launchd,
    Xdg,
}

/// File-based login item. The file is written when the switch is turned on and
/// removed when it is turned off; a missing file already means "off".
pub struct FileStartup {
    path: PathBuf,
    kind: FileStartupKind,
}

impl FileStartup {
    pub fn new(path: impl Into<PathBuf>, kind: FileStartupKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }

    pub fn macos() -> Self {
        Self::new(
            home_dir().join("Library/LaunchAgents/dev.fluxy.app.plist"),
            FileStartupKind::Launchd,
        )
    }

    pub fn linux() -> Self {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"));
        Self::new(base.join("autostart/fluxy.desktop"), FileStartupKind::Xdg)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn render(&self, exe: &Path) -> String {
        match self.kind {
            FileStartupKind::Launchd => launchd_plist(exe),
            FileStartupKind::Xdg => xdg_desktop_entry(exe),
        }
    }
}

impl StartupBackend for FileStartup {
    fn is_enabled(&self) -> Result<bool, String> {
        Ok(self.path.is_file())
    }

    fn set_enabled(&self, enabled: bool, exe: &Path) -> Result<(), String> {
        if !enabled {
            return match std::fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            };
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&self.path, self.render(exe)).map_err(|error| error.to_string())
    }

    fn location(&self) -> String {
        self.path.display().to_string()
    }
}

#[cfg(windows)]
pub const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Runs an external program and reports whether it exited successfully.
#[cfg(windows)]
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[String]) -> Result<bool, String>;
}

#[cfg(windows)]
pub struct SystemCommandRunner;

#[cfg(windows)]
impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<bool, String> {
        let status = std::process::Command::new(program)
            .args(args)
            .status()
            .map_err(|error| error.to_string())?;
        Ok(status.success())
    }
}

/// `reg.exe` arguments that register the login item.
#[cfg(windows)]
pub fn windows_add_args(exe: &Path) -> Vec<String> {
    vec![
        "add".into(),
        WINDOWS_RUN_KEY.into(),
        "/v".into(),
        LOGIN_ITEM_NAME.into(),
        "/t".into(),
        "REG_SZ".into(),
        "/d".into(),
        login_command(exe),
        "/f".into(),
    ]
}

/// `reg.exe` arguments that read the login item.
#[cfg(windows)]
pub fn windows_query_args() -> Vec<String> {
    vec![
        "query".into(),
        WINDOWS_RUN_KEY.into(),
        "/v".into(),
        LOGIN_ITEM_NAME.into(),
    ]
}

/// `reg.exe` arguments that remove the login item.
#[cfg(windows)]
pub fn windows_delete_args() -> Vec<String> {
    vec![
        "delete".into(),
        WINDOWS_RUN_KEY.into(),
        "/v".into(),
        LOGIN_ITEM_NAME.into(),
        "/f".into(),
    ]
}

/// Per-user Windows Run key, written through `reg.exe` so the shell needs no
/// registry dependency of its own.
#[cfg(windows)]
pub struct WindowsStartup {
    runner: Arc<dyn CommandRunner>,
}

#[cfg(windows)]
impl WindowsStartup {
    pub fn system() -> Self {
        Self {
            runner: Arc::new(SystemCommandRunner),
        }
    }

    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }
}

#[cfg(windows)]
impl StartupBackend for WindowsStartup {
    fn is_enabled(&self) -> Result<bool, String> {
        self.runner.run("reg", &windows_query_args())
    }

    fn set_enabled(&self, enabled: bool, exe: &Path) -> Result<(), String> {
        let args = if enabled {
            windows_add_args(exe)
        } else {
            windows_delete_args()
        };
        let ok = self.runner.run("reg", &args)?;
        if ok || !enabled {
            // Removing an item that is not there exits non-zero, which is
            // already the state the caller asked for.
            Ok(())
        } else {
            Err("reg.exe rejected the login item".into())
        }
    }

    fn location(&self) -> String {
        format!("{WINDOWS_RUN_KEY}\\{LOGIN_ITEM_NAME}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\Fluxy\fluxy.exe")
        } else {
            PathBuf::from("/opt/fluxy/bin/fluxy")
        }
    }

    #[test]
    fn the_login_command_starts_the_shell_hidden() {
        let command = login_command(Path::new("/opt/fluxy/fluxy"));
        assert!(command.starts_with('"'));
        assert!(command.ends_with(HIDDEN_FLAG));
        assert!(command.contains("/opt/fluxy/fluxy"));
    }

    #[test]
    fn the_file_formats_escape_the_executable_path() {
        let odd = Path::new("/opt/we&ird/<fluxy>/fluxy");
        let plist = launchd_plist(odd);
        assert!(plist.contains("we&amp;ird"));
        assert!(plist.contains("&lt;fluxy&gt;"));
        assert!(plist.contains("<string>--hidden</string>"));

        let entry = xdg_desktop_entry(Path::new("/opt/we ird/\"fluxy\"/fluxy"));
        assert!(entry.contains("Exec=\"/opt/we ird/\\\"fluxy\\\"/fluxy\" --hidden"));
        assert!(entry.starts_with("[Desktop Entry]"));
    }

    #[test]
    fn the_file_backend_writes_and_removes_the_login_item() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileStartup::new(
            dir.path().join("nested/fluxy.desktop"),
            FileStartupKind::Xdg,
        );
        assert!(!backend.is_enabled().unwrap());

        backend.set_enabled(true, &exe()).unwrap();
        assert!(backend.is_enabled().unwrap());
        let written = std::fs::read_to_string(backend.path()).unwrap();
        assert!(written.contains("--hidden"));
        assert_eq!(backend.location(), backend.path().display().to_string());

        backend.set_enabled(false, &exe()).unwrap();
        assert!(!backend.is_enabled().unwrap());
        // Removing it twice is not an error.
        backend.set_enabled(false, &exe()).unwrap();
    }

    #[cfg(windows)]
    mod windows {
        use super::*;
        // Only this module needs it; importing it one level up made Linux and
        // macOS clippy fail with "unused import".
        use std::sync::Mutex;

        #[derive(Default)]
        struct FakeRunner {
            calls: Mutex<Vec<Vec<String>>>,
            query_succeeds: bool,
            add_succeeds: bool,
        }

        impl CommandRunner for FakeRunner {
            fn run(&self, program: &str, args: &[String]) -> Result<bool, String> {
                assert_eq!(program, "reg");
                self.calls.lock().unwrap().push(args.to_vec());
                if args[0] == "query" {
                    Ok(self.query_succeeds)
                } else {
                    Ok(self.add_succeeds)
                }
            }
        }

        fn runner(query: bool, add: bool) -> (Arc<FakeRunner>, WindowsStartup) {
            let fake = Arc::new(FakeRunner {
                calls: Mutex::new(Vec::new()),
                query_succeeds: query,
                add_succeeds: add,
            });
            (fake.clone(), WindowsStartup::with_runner(fake))
        }

        #[test]
        fn the_run_key_value_hides_the_window_and_is_per_user() {
            let args = windows_add_args(&exe());
            assert_eq!(args[0], "add");
            assert_eq!(args[1], WINDOWS_RUN_KEY);
            assert!(WINDOWS_RUN_KEY.starts_with("HKCU"));
            let value = &args[args.len() - 2];
            assert!(value.ends_with(HIDDEN_FLAG));
            assert!(value.contains("fluxy.exe"));
            assert_eq!(args[args.len() - 1], "/f");
        }

        #[test]
        fn the_windows_backend_reports_and_writes_the_login_item() {
            let (fake, backend) = runner(true, true);
            assert!(backend.is_enabled().unwrap());
            backend.set_enabled(true, &exe()).unwrap();
            backend.set_enabled(false, &exe()).unwrap();
            let calls = fake.calls.lock().unwrap().clone();
            assert_eq!(calls.len(), 3);
            assert_eq!(calls[0][0], "query");
            assert_eq!(calls[1][0], "add");
            assert_eq!(calls[2][0], "delete");
            assert!(backend.location().contains(LOGIN_ITEM_NAME));

            let (_, missing) = runner(false, true);
            assert!(!missing.is_enabled().unwrap());
        }

        #[test]
        fn a_failed_registration_is_reported_but_a_missing_removal_is_not() {
            let (_, failing) = runner(true, false);
            assert!(failing.set_enabled(true, &exe()).is_err());
            // Deleting something that is not there is already the wanted state.
            assert!(failing.set_enabled(false, &exe()).is_ok());
        }
    }
}
