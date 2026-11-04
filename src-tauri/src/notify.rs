//! Desktop notifications and native shell labels.
//!
//! Notifications intentionally contain only the trusted peer's display name and
//! a message type. Message bodies and clipboard text never leave the WebView
//! through this module.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;
#[cfg(desktop)]
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

const MAX_LABEL_CHARS: usize = 64;
const MAX_BODY_CHARS: usize = 160;
const FALLBACK_NAME: &str = "Fluxy";
const FALLBACK_FILE: &str = "-";
/// A bulk send is one record per file, so a batch that finishes at once would
/// otherwise raise one notification per file. Per device, at most one transfer
/// outcome is dispatched inside this window.
const TRANSFER_NOTIFY_WINDOW: Duration = Duration::from_secs(2);
/// Transfer bodies carry both the file and the peer, so the fallback keeps both
/// placeholders instead of collapsing to a bare name.
const TRANSFER_FALLBACK: &str = "{file} · {name}";

/// Labels supplied by the trusted frontend i18n catalog. Native tray menus and
/// OS notifications cannot read the WebView locale, so the frontend pushes the
/// current strings whenever the locale changes.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShellLabels {
    pub open: String,
    pub quit: String,
    /// Tray item that mirrors the automatic clipboard-send switch.
    pub auto_clipboard: String,
    /// Tray item that mirrors the login item.
    pub autostart: String,
    /// Tray item that opens the firewall and help page.
    pub help: String,
    pub chat_title: String,
    pub chat_body: String,
    pub clipboard_title: String,
    pub clipboard_body: String,
    pub clipboard_applied_title: String,
    pub clipboard_applied_body: String,
    pub transfer_title: String,
    pub transfer_body: String,
    pub transfer_failed_title: String,
    pub transfer_failed_body: String,
}

impl Default for ShellLabels {
    fn default() -> Self {
        Self {
            open: "Fluxy".into(),
            quit: "Quit".into(),
            auto_clipboard: "Auto send clipboard".into(),
            autostart: "Start at login".into(),
            help: "Help".into(),
            chat_title: "Fluxy".into(),
            chat_body: "{name}".into(),
            clipboard_title: "Fluxy".into(),
            clipboard_body: "{name}".into(),
            clipboard_applied_title: "Fluxy".into(),
            clipboard_applied_body: "{name}".into(),
            transfer_title: "Fluxy".into(),
            transfer_body: "{file} · {name}".into(),
            transfer_failed_title: "Fluxy".into(),
            transfer_failed_body: "{file} · {name}".into(),
        }
    }
}

impl ShellLabels {
    fn sanitized(self) -> Self {
        Self {
            open: clean(&self.open, MAX_LABEL_CHARS, FALLBACK_NAME),
            quit: clean(&self.quit, MAX_LABEL_CHARS, FALLBACK_NAME),
            auto_clipboard: clean(&self.auto_clipboard, MAX_LABEL_CHARS, FALLBACK_NAME),
            autostart: clean(&self.autostart, MAX_LABEL_CHARS, FALLBACK_NAME),
            help: clean(&self.help, MAX_LABEL_CHARS, FALLBACK_NAME),
            chat_title: clean(&self.chat_title, MAX_LABEL_CHARS, FALLBACK_NAME),
            chat_body: clean(&self.chat_body, MAX_BODY_CHARS, "{name}"),
            clipboard_title: clean(&self.clipboard_title, MAX_LABEL_CHARS, FALLBACK_NAME),
            clipboard_body: clean(&self.clipboard_body, MAX_BODY_CHARS, "{name}"),
            clipboard_applied_title: clean(
                &self.clipboard_applied_title,
                MAX_LABEL_CHARS,
                FALLBACK_NAME,
            ),
            clipboard_applied_body: clean(&self.clipboard_applied_body, MAX_BODY_CHARS, "{name}"),
            transfer_title: clean(&self.transfer_title, MAX_LABEL_CHARS, FALLBACK_NAME),
            transfer_body: clean(&self.transfer_body, MAX_BODY_CHARS, TRANSFER_FALLBACK),
            transfer_failed_title: clean(
                &self.transfer_failed_title,
                MAX_LABEL_CHARS,
                FALLBACK_NAME,
            ),
            transfer_failed_body: clean(
                &self.transfer_failed_body,
                MAX_BODY_CHARS,
                TRANSFER_FALLBACK,
            ),
        }
    }
}

fn clean(value: &str, max: usize, fallback: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        fallback.into()
    } else {
        cleaned.into()
    }
}

fn peer_name(value: &str) -> String {
    clean(value, MAX_LABEL_CHARS, FALLBACK_NAME)
}

fn file_name(value: &str) -> String {
    clean(value, MAX_LABEL_CHARS, FALLBACK_FILE)
}

fn body(template: &str, name: &str) -> String {
    template.replace("{name}", name)
}

/// Transfer bodies name the file as well as the peer. The file name is bounded
/// and stripped of control characters, and the peer name is sanitized once.
fn transfer_body(template: &str, name: &str, file: &str) -> String {
    body(template, &peer_name(name)).replace("{file}", &file_name(file))
}

/// The tray menu items the shell owns. They live here so a locale change can
/// relabel them and so the two check items can follow the persisted switches.
#[derive(Clone)]
#[cfg(desktop)]
pub struct TrayMenu {
    pub open: MenuItem<Wry>,
    pub auto_clipboard: CheckMenuItem<Wry>,
    pub autostart: CheckMenuItem<Wry>,
    pub help: MenuItem<Wry>,
    pub quit: MenuItem<Wry>,
}

struct NotifierInner {
    app: Mutex<Option<AppHandle<Wry>>>,
    labels: RwLock<ShellLabels>,
    #[cfg(desktop)]
    menu: Mutex<Option<TrayMenu>>,
    next_id: AtomicI32,
    /// Desktop notifications are opt-out; the settings page owns the value.
    enabled: AtomicBool,
    /// Last transfer notification per device, used to coalesce a batch.
    transfers: Mutex<HashMap<String, Instant>>,
    /// Test-only record of every requested dispatch, captured before the mute
    /// and window-focus checks so a test can assert which transitions ask for a
    /// notification at all.
    #[cfg(test)]
    attempts: Mutex<Vec<&'static str>>,
}

impl Default for NotifierInner {
    fn default() -> Self {
        Self {
            app: Mutex::new(None),
            labels: RwLock::new(ShellLabels::default()),
            #[cfg(desktop)]
            menu: Mutex::new(None),
            next_id: AtomicI32::new(0),
            enabled: AtomicBool::new(true),
            transfers: Mutex::new(HashMap::new()),
            #[cfg(test)]
            attempts: Mutex::new(Vec::new()),
        }
    }
}

/// Cloneable bridge from the chat runtime to the Tauri shell. Before attach
/// runs (unit tests and early startup) every notification is a no-op.
#[derive(Clone, Default)]
pub struct Notifier {
    inner: Arc<NotifierInner>,
}

impl Notifier {
    pub fn labels(&self) -> ShellLabels {
        self.inner
            .labels
            .read()
            .map(|labels| labels.clone())
            .unwrap_or_default()
    }

    /// Attach the running application and the tray menu items. Called once from
    /// Tauri setup after the notification plugin is initialized.
    #[cfg(desktop)]
    pub fn attach(&self, app: AppHandle<Wry>, menu: TrayMenu) {
        {
            let mut slot = self
                .inner
                .app
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.is_some() {
                return;
            }
            *slot = Some(app.clone());
        }
        {
            let mut slot = self
                .inner
                .menu
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *slot = Some(menu);
        }
        let handle = app.clone();
        if let Err(error) = app.notification().on_action(move |_action| {
            focus_main(&handle);
        }) {
            tracing::warn!(%error, "could not register notification click handler");
        }
    }

    /// Phone flavour of the same bridge: there is no tray and no window to
    /// focus, but the notifier still needs the handle or every send returns
    /// early and no notification is ever shown.
    #[cfg(not(desktop))]
    pub fn attach(&self, app: AppHandle<Wry>) {
        let mut slot = self
            .inner
            .app
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            return;
        }
        *slot = Some(app);
    }

    /// Update the native labels. The frontend sends already-localized strings;
    /// invalid or oversized values are bounded here as well.
    pub fn set_labels(&self, labels: ShellLabels) {
        let labels = labels.sanitized();
        if let Ok(mut current) = self.inner.labels.write() {
            *current = labels.clone();
        }
        // The tray menu only exists on desktop; the labels themselves are
        // stored either way because the notification payloads use them.
        #[cfg(desktop)]
        {
            let menu = self
                .inner
                .menu
                .lock()
                .map(|menu| menu.clone())
                .unwrap_or(None);
            if let Some(menu) = menu {
                let _ = menu.open.set_text(&labels.open);
                let _ = menu.quit.set_text(&labels.quit);
                let _ = menu.auto_clipboard.set_text(&labels.auto_clipboard);
                let _ = menu.autostart.set_text(&labels.autostart);
                let _ = menu.help.set_text(&labels.help);
            }
        }
        tracing::debug!("native shell labels updated");
    }

    /// Mirror the automatic clipboard-send switch onto its tray check item.
    /// Mobile builds have no tray, so the call is a no-op there.
    pub fn set_auto_clipboard_checked(&self, enabled: bool) {
        #[cfg(desktop)]
        self.with_menu(|menu| menu.auto_clipboard.set_checked(enabled));
        #[cfg(not(desktop))]
        let _ = enabled;
    }

    /// Mirror the login-item preference onto its tray check item.
    pub fn set_autostart_checked(&self, enabled: bool) {
        #[cfg(desktop)]
        self.with_menu(|menu| menu.autostart.set_checked(enabled));
        #[cfg(not(desktop))]
        let _ = enabled;
    }

    #[cfg(desktop)]
    fn with_menu(&self, update: impl FnOnce(&TrayMenu) -> tauri::Result<()>) {
        let menu = self
            .inner
            .menu
            .lock()
            .map(|menu| menu.clone())
            .unwrap_or(None);
        if let Some(menu) = menu
            && let Err(error) = update(&menu)
        {
            tracing::warn!(%error, "could not update a tray menu item");
        }
    }

    /// Mirror the persisted preference; a muted notifier dispatches nothing.
    pub fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn attempts(&self) -> Vec<&'static str> {
        self.inner
            .attempts
            .lock()
            .map(|attempts| attempts.clone())
            .unwrap_or_default()
    }

    /// `device_id` rides along as extra data: a tap on the notification has to
    /// know which conversation to open, and the OS only hands back what we put
    /// in.
    pub fn notify_chat(&self, device_id: &str, name: &str) {
        tracing::warn!(device_id, "notify_chat entered");
        let labels = self.labels();
        self.send_with_extra(
            "chat",
            &labels.chat_title,
            &body(&labels.chat_body, &peer_name(name)),
            ("device_id", device_id.to_string()),
        );
    }

    pub fn notify_clipboard_offer(&self, name: &str) {
        let labels = self.labels();
        self.send(
            "clipboard_offer",
            &labels.clipboard_title,
            &body(&labels.clipboard_body, &peer_name(name)),
        );
    }

    pub fn notify_clipboard_applied(&self, name: &str) {
        let labels = self.labels();
        self.send(
            "clipboard_applied",
            &labels.clipboard_applied_title,
            &body(&labels.clipboard_applied_body, &peer_name(name)),
        );
    }

    pub fn notify_transfer_completed(&self, device_id: &str, name: &str, file: &str) {
        if !self.transfer_window_open(device_id) {
            return;
        }
        let labels = self.labels();
        self.send(
            "transfer_completed",
            &labels.transfer_title,
            &transfer_body(&labels.transfer_body, name, file),
        );
    }

    pub fn notify_transfer_failed(&self, device_id: &str, name: &str, file: &str) {
        if !self.transfer_window_open(device_id) {
            return;
        }
        let labels = self.labels();
        self.send(
            "transfer_failed",
            &labels.transfer_failed_title,
            &transfer_body(&labels.transfer_failed_body, name, file),
        );
    }

    /// True when this device has not raised a transfer notification inside the
    /// coalescing window. Records the attempt so a burst stays quiet.
    fn transfer_window_open(&self, device_id: &str) -> bool {
        let now = Instant::now();
        let Ok(mut transfers) = self.inner.transfers.lock() else {
            return true;
        };
        match transfers.get(device_id) {
            Some(previous) if now.duration_since(*previous) < TRANSFER_NOTIFY_WINDOW => false,
            _ => {
                transfers.insert(device_id.to_string(), now);
                true
            }
        }
    }

    fn send(&self, kind: &'static str, title: &str, body: &str) {
        self.send_inner(kind, title, body, None);
    }

    fn send_with_extra(
        &self,
        kind: &'static str,
        title: &str,
        body: &str,
        extra: (&str, String),
    ) {
        self.send_inner(kind, title, body, Some(extra));
    }

    fn send_inner(
        &self,
        kind: &'static str,
        title: &str,
        body: &str,
        extra: Option<(&str, String)>,
    ) {
        #[cfg(test)]
        if let Ok(mut attempts) = self.inner.attempts.lock() {
            attempts.push(kind);
        }
        if !self.enabled() {
            tracing::warn!(kind, "notification skipped: disabled");
            return;
        }
        let app = self.inner.app.lock().map(|app| app.clone()).unwrap_or(None);
        let Some(app) = app else {
            tracing::warn!(kind, "notification skipped: no app handle attached");
            return;
        };
        if main_is_focused(&app) {
            tracing::warn!(kind, "notification skipped: window focused");
            return;
        }
        let id = self
            .inner
            .next_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(if current >= i32::MAX - 1 {
                    1
                } else {
                    current + 1
                })
            })
            .unwrap_or(1);
        // Android needs the tap to open the conversation, and the Tauri
        // notification plugin cannot carry extra data through its intent.
        // Our own plugin posts it with a fluxy://chat?device= deep link.
        #[cfg(target_os = "android")]
        {
            let _ = id;
            let device = extra
                .as_ref()
                .filter(|(key, _)| *key == "device_id")
                .map(|(_, value)| value.as_str())
                .unwrap_or("");
            let result = app
                .try_state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>()
                .map(|mobile| mobile.notify_chat(device, title, body))
                .unwrap_or_else(|| Err("the mobile bridge is not installed".to_string()));
            match result {
                Ok(()) => tracing::warn!(kind, "chat notification posted"),
                Err(error) => tracing::warn!(%error, "could not post the notification"),
            }
            return;
        }
        #[cfg(not(target_os = "android"))]
        {
            let mut builder = app.notification().builder().id(id).title(title).body(body);
            if let Some((key, value)) = extra {
                builder = builder.extra(key, value);
            }
            match builder.show() {
                Ok(()) => tracing::debug!(id, kind, "desktop notification dispatched"),
                Err(error) => tracing::warn!(%error, "could not show desktop notification"),
            }
        }
    }
}

/// Restore, unminimize and focus the main window. Used by notification clicks
/// and by the tray icon.
#[cfg(desktop)]
pub fn focus_main(app: &AppHandle<Wry>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn main_is_focused(app: &AppHandle<Wry>) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    if window.is_minimized().unwrap_or(false) {
        return false;
    }
    if window.is_focused().unwrap_or(false) {
        return true;
    }
    // Tauri's cached focus flag can lag after a Win32 foreground activation
    // (for example a test tool calling SetForegroundWindow). Fall back to the
    // actual foreground window on Windows.
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let Some(hwnd) = window.hwnd().ok().map(|handle| handle.0 as isize) else {
            return false;
        };
        let foreground = unsafe { GetForegroundWindow() };
        !foreground.is_null() && foreground as isize == hwnd
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_bounded_and_keep_the_name_placeholder() {
        let labels = ShellLabels {
            open: "  Open\nFluxy  ".into(),
            quit: "\u{0}".into(),
            auto_clipboard: "Auto\tclipboard".into(),
            autostart: "".into(),
            help: "  Help  ".into(),
            chat_title: "Title".into(),
            chat_body: "from {name}".into(),
            clipboard_title: "Clip".into(),
            clipboard_body: "offer from {name}".into(),
            clipboard_applied_title: "Applied".into(),
            clipboard_applied_body: "{name} applied".into(),
            transfer_title: "Transfer done".into(),
            transfer_body: "{file}\nfrom {name}".into(),
            transfer_failed_title: "\u{0}".into(),
            transfer_failed_body: "{file} failed for {name}".into(),
        }
        .sanitized();
        assert_eq!(labels.open, "OpenFluxy");
        assert_eq!(labels.quit, FALLBACK_NAME);
        // The tray items are sanitized like every other native string.
        assert_eq!(labels.auto_clipboard, "Autoclipboard");
        assert_eq!(labels.autostart, FALLBACK_NAME);
        assert_eq!(labels.help, "Help");
        // The tray items are sanitized like every other native string.
        assert_eq!(labels.auto_clipboard, "Autoclipboard");
        assert_eq!(labels.autostart, FALLBACK_NAME);
        assert_eq!(labels.help, "Help");
        assert_eq!(body(&labels.chat_body, "Peer"), "from Peer");
        assert_eq!(labels.transfer_failed_title, FALLBACK_NAME);
        assert_eq!(
            transfer_body(&labels.transfer_body, "Peer", "report.pdf"),
            "report.pdffrom Peer"
        );
    }

    #[test]
    fn transfer_bodies_name_the_file_and_bound_it() {
        let labels = ShellLabels::default().sanitized();
        assert_eq!(
            transfer_body(&labels.transfer_body, "  Desk\nTop ", "  a\rb  "),
            "ab · DeskTop"
        );
        assert_eq!(transfer_body(&labels.transfer_body, "", ""), "- · Fluxy");
        assert_eq!(file_name(&"f".repeat(100)).chars().count(), MAX_LABEL_CHARS);
    }

    #[test]
    fn notifications_are_on_until_the_settings_page_mutes_them() {
        let notifier = Notifier::default();
        assert!(notifier.enabled());
        notifier.set_enabled(false);
        assert!(!notifier.enabled());
        notifier.set_enabled(true);
        assert!(notifier.enabled());
    }

    #[test]
    fn a_muted_notifier_still_records_the_attempt_but_dispatches_nothing() {
        let notifier = Notifier::default();
        notifier.notify_chat("Peer");
        notifier.set_enabled(false);
        notifier.notify_transfer_failed("device-1", "Peer", "report.bin");
        assert_eq!(notifier.attempts(), vec!["chat", "transfer_failed"]);
        assert!(!notifier.enabled());
    }

    #[test]
    fn a_batch_burst_raises_one_transfer_notification_per_device() {
        let notifier = Notifier::default();
        notifier.notify_transfer_completed("device-1", "Peer", "one.bin");
        notifier.notify_transfer_completed("device-1", "Peer", "two.bin");
        notifier.notify_transfer_completed("device-2", "Other", "three.bin");
        // The failure is inside device-1's window as well, so a burst of one
        // device stays a single notification either way.
        notifier.notify_transfer_failed("device-1", "Peer", "four.bin");
        assert_eq!(
            notifier.attempts(),
            vec!["transfer_completed", "transfer_completed"]
        );
    }

    #[test]
    fn peer_names_are_single_line_and_bounded() {
        let name = peer_name("  A\nB\r\nC  ");
        assert_eq!(name, "ABC");
        assert_eq!(peer_name("   "), FALLBACK_NAME);
        assert_eq!(peer_name(&"x".repeat(100)).chars().count(), MAX_LABEL_CHARS);
    }
}
