//! System clipboard access and the in-memory clipboard sharing state.
//!
//! Clipboard text is never persisted. An offer carries only a preview, size,
//! and BLAKE3 digest; the receiver must explicitly accept before the sender
//! sends the text. Incoming text is verified against the offer digest before
//! it can be written to the OS clipboard.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::contract::{
    ClipboardDirection, ClipboardEvent, ClipboardShareState, ClipboardShareView, ClipboardSnapshot,
    IpcError,
};

const MAX_SHARES: usize = 64;
const MAX_PENDING_OFFERS: usize = 32;
const OFFER_TTL_MS: i64 = 120_000;
const MIN_OFFER_TTL_MS: i64 = 5_000;
const MAX_OFFER_TTL_MS: i64 = 600_000;
const MAX_PREVIEW_CHARS: usize = 160;
const RECENT_DIGEST_CAP: usize = 16;
const RECENT_DIGEST_TTL_MS: i64 = 5 * 60 * 1000;
const CLIPBOARD_OPERATION_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy)]
struct RecentDigest {
    digest: [u8; 32],
    at_ms: i64,
}

/// Small abstraction so clipboard state can be unit-tested without an X11,
/// Wayland, Windows, or macOS clipboard server.
pub trait TextClipboard: Send + Sync {
    fn read_text(&self) -> Result<String, String>;
    fn write_text(&self, text: &str) -> Result<(), String>;
}

enum ClipboardRequest {
    Read(mpsc::Sender<Result<String, String>>),
    Write(String, mpsc::Sender<Result<(), String>>),
}

/// A dedicated thread owns arboard's platform clipboard. On Linux the
/// clipboard is served by the owning process, so dropping a short-lived
/// arboard handle immediately after writing would make the text disappear.
pub struct SystemClipboard {
    requests: mpsc::Sender<ClipboardRequest>,
}

/// Android reads and writes through the mobile plugin, which needs the app
/// handle. It arrives from the setup hook, after the runtime that owns this
/// clipboard already exists, so it is stored once for the whole process.
#[cfg(target_os = "android")]
static MOBILE_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_app_handle(app: tauri::AppHandle) {
    let _ = MOBILE_APP.set(app);
}

impl SystemClipboard {
    pub fn new() -> Self {
        let (requests, receiver) = mpsc::channel();
        #[cfg(not(target_os = "android"))]
        let _ = thread::Builder::new()
            .name("fluxy-clipboard".into())
            .spawn(move || run_system_clipboard(receiver));
        #[cfg(target_os = "android")]
        let _ = thread::Builder::new()
            .name("fluxy-clipboard".into())
            .spawn(move || run_mobile_clipboard(receiver));
        Self { requests }
    }

    fn request<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, String>>) -> ClipboardRequest,
    ) -> Result<T, String> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(build(reply))
            .map_err(|_| "clipboard thread is unavailable".to_string())?;
        result
            .recv_timeout(CLIPBOARD_OPERATION_TIMEOUT)
            .map_err(|_| "clipboard operation timed out".to_string())?
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl TextClipboard for SystemClipboard {
    fn read_text(&self) -> Result<String, String> {
        self.request(ClipboardRequest::Read)
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        self.request(|reply| ClipboardRequest::Write(text.to_string(), reply))
    }
}

/// Android has no desktop clipboard: the plugin owns it, and this forwards each
/// request there. Until the app handle is installed every request is refused
/// rather than silently succeeding.
#[cfg(target_os = "android")]
fn run_mobile_clipboard(receiver: mpsc::Receiver<ClipboardRequest>) {
    for request in receiver {
        let handle = MOBILE_APP.get().cloned();
        match request {
            ClipboardRequest::Read(reply) => {
                let outcome = match handle {
                    Some(app) => tauri::async_runtime::block_on(read_through_plugin(app)),
                    None => Err(CLIPBOARD_UNAVAILABLE.to_string()),
                };
                let _ = reply.send(outcome);
            }
            ClipboardRequest::Write(text, reply) => {
                let outcome = match handle {
                    Some(app) => tauri::async_runtime::block_on(write_through_plugin(app, text)),
                    None => Err(CLIPBOARD_UNAVAILABLE.to_string()),
                };
                let _ = reply.send(outcome);
            }
        }
    }
}

#[cfg(target_os = "android")]
async fn read_through_plugin(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
    handle.clipboard_read().await.map(|payload| payload.text)
}

#[cfg(target_os = "android")]
async fn write_through_plugin(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri::Manager;
    let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
    handle.clipboard_write(text).await
}

#[cfg(target_os = "android")]
const CLIPBOARD_UNAVAILABLE: &str = "the system clipboard is not available on this platform";

#[cfg(not(target_os = "android"))]
fn run_system_clipboard(receiver: mpsc::Receiver<ClipboardRequest>) {
    let mut clipboard: Option<arboard::Clipboard> = None;
    for request in receiver {
        match request {
            ClipboardRequest::Read(reply) => {
                let value = with_clipboard(&mut clipboard, |clipboard| {
                    clipboard.get_text().map_err(|error| error.to_string())
                });
                let _ = reply.send(value);
            }
            ClipboardRequest::Write(text, reply) => {
                let value = with_clipboard(&mut clipboard, |clipboard| {
                    clipboard.set_text(text).map_err(|error| error.to_string())
                });
                let _ = reply.send(value);
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
fn with_clipboard<T>(
    slot: &mut Option<arboard::Clipboard>,
    operation: impl FnOnce(&mut arboard::Clipboard) -> Result<T, String>,
) -> Result<T, String> {
    if slot.is_none() {
        *slot = Some(
            arboard::Clipboard::new()
                .map_err(|error| format!("clipboard is unavailable: {error}"))?,
        );
    }
    let clipboard = slot.as_mut().expect("clipboard initialized above");
    let result = operation(clipboard);
    if result.is_err() {
        // A failed platform context can remain wedged; recreate it on the next
        // request instead of failing every later clipboard operation.
        *slot = None;
    }
    result
}

#[derive(Debug, Clone)]
pub struct ClipboardOfferDraft {
    pub offer_id: Uuid,
    pub size_bytes: u64,
    pub digest: [u8; 32],
    pub preview: String,
    pub ttl_ms: u64,
    pub view: ClipboardShareView,
}

#[derive(Debug, Clone)]
pub struct ClipboardAckDraft {
    pub offer_id: Uuid,
    pub accepted: bool,
    pub applied: bool,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum ClipboardFollowUp {
    SendText {
        offer_id: Uuid,
        text: String,
        digest: [u8; 32],
    },
    SendAck(ClipboardAckDraft),
}

struct PendingIncoming {
    device_id: Uuid,
    digest: [u8; 32],
    size_bytes: u64,
    expires_at_ms: i64,
    accepted: bool,
}

struct PendingOutgoing {
    device_id: Uuid,
    digest: [u8; 32],
    text: String,
}

pub struct ClipboardState {
    backend: Arc<dyn TextClipboard>,
    shares: Mutex<VecDeque<ClipboardShareView>>,
    incoming: Mutex<HashMap<Uuid, PendingIncoming>>,
    outgoing: Mutex<HashMap<Uuid, PendingOutgoing>>,
    recent_applied: Mutex<VecDeque<RecentDigest>>,
}

impl ClipboardState {
    pub fn new(backend: Arc<dyn TextClipboard>) -> Self {
        Self {
            backend,
            shares: Mutex::new(VecDeque::new()),
            incoming: Mutex::new(HashMap::new()),
            outgoing: Mutex::new(HashMap::new()),
            recent_applied: Mutex::new(VecDeque::new()),
        }
    }

    /// Put text on the local clipboard. Used when a chat message arrives: the
    /// point of sending text between devices is to paste it on the far side.
    pub fn write_local(&self, text: &str) -> Result<(), IpcError> {
        self.backend.write_text(text).map_err(clipboard_error)
    }

    pub fn read_local_text(&self) -> Result<String, IpcError> {
        self.backend.read_text().map_err(clipboard_error)
    }

    pub fn share(&self, offer_id: Uuid) -> Option<ClipboardShareView> {
        self.find_share(offer_id)
    }

    /// Whether this text was already written to the local clipboard recently.
    /// Used to stop echoes and to prevent re-sending received content.
    pub fn is_recent_applied(&self, digest: &[u8; 32]) -> bool {
        let Ok(mut recent) = self.recent_applied.lock() else {
            return false;
        };
        prune_recent(&mut recent);
        recent.iter().any(|entry| entry.digest == *digest)
    }

    /// Reserve a digest before writing. This closes the check-then-write race
    /// between concurrent sessions: only one apply can claim a digest, and a
    /// failed write releases it for a later retry.
    fn claim_applied(&self, digest: [u8; 32]) -> bool {
        let Ok(mut recent) = self.recent_applied.lock() else {
            return false;
        };
        prune_recent(&mut recent);
        if recent.iter().any(|entry| entry.digest == digest) {
            return false;
        }
        recent.push_back(RecentDigest {
            digest,
            at_ms: now_ms(),
        });
        while recent.len() > RECENT_DIGEST_CAP {
            recent.pop_front();
        }
        true
    }

    fn release_applied(&self, digest: [u8; 32]) {
        if let Ok(mut recent) = self.recent_applied.lock() {
            recent.retain(|entry| entry.digest != digest);
        }
    }

    pub fn snapshot(&self) -> ClipboardSnapshot {
        self.expire_stale();
        ClipboardSnapshot {
            shares: self
                .shares
                .lock()
                .map(|shares| shares.iter().cloned().collect())
                .unwrap_or_default(),
        }
    }

    pub fn begin_outgoing(
        &self,
        device_id: Uuid,
        text: String,
    ) -> Result<ClipboardOfferDraft, IpcError> {
        if text.is_empty() {
            return Err(IpcError {
                code: "clipboard_empty".into(),
                code_number: 4302,
                message: "the system clipboard does not contain text".into(),
                retryable: false,
            });
        }
        if text.len() > fluxy_net::MAX_CLIPBOARD_BYTES {
            return Err(IpcError {
                code: "clipboard_too_large".into(),
                code_number: 4303,
                message: "clipboard text exceeds the 1 MiB limit".into(),
                retryable: false,
            });
        }
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        if self.is_recent_applied(&digest) {
            return Err(IpcError {
                code: "clipboard_duplicate".into(),
                code_number: 4305,
                message: "this clipboard content was already received recently".into(),
                retryable: false,
            });
        }
        let now = now_ms();
        let expires_at_ms = now + OFFER_TTL_MS;
        let offer_id = Uuid::now_v7();
        let preview = preview(&text);
        let size_bytes = text.len() as u64;
        let view = ClipboardShareView {
            offer_id: offer_id.to_string(),
            device_id: device_id.to_string(),
            direction: ClipboardDirection::Sent,
            state: ClipboardShareState::Offered,
            preview: preview.clone(),
            size_bytes,
            created_at_ms: now,
            expires_at_ms,
            error_code: None,
            error_message: None,
        };
        self.outgoing
            .lock()
            .map_err(|_| IpcError::internal("clipboard state unavailable"))?
            .insert(
                offer_id,
                PendingOutgoing {
                    device_id,
                    digest,
                    text,
                },
            );
        self.push_share(view.clone());
        self.trim_pending();
        Ok(ClipboardOfferDraft {
            offer_id,
            size_bytes,
            digest,
            preview,
            ttl_ms: OFFER_TTL_MS as u64,
            view,
        })
    }

    pub fn receive_offer(
        &self,
        device_id: Uuid,
        offer_id: Uuid,
        size_bytes: u64,
        digest: [u8; 32],
        preview: String,
        ttl_ms: u64,
    ) -> Vec<ClipboardEvent> {
        if self.shares_contains(offer_id) {
            return Vec::new();
        }
        if size_bytes > fluxy_net::MAX_CLIPBOARD_BYTES as u64 {
            return Vec::new();
        }
        let ttl_ms = (ttl_ms as i64).clamp(MIN_OFFER_TTL_MS, MAX_OFFER_TTL_MS);
        let now = now_ms();
        let expires_at_ms = now + ttl_ms;
        let preview = preview.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        if self
            .incoming
            .lock()
            .map(|incoming| incoming.len() >= MAX_PENDING_OFFERS)
            .unwrap_or(false)
        {
            return Vec::new();
        }
        self.incoming
            .lock()
            .expect("clipboard incoming state")
            .insert(
                offer_id,
                PendingIncoming {
                    device_id,
                    digest,
                    size_bytes,
                    expires_at_ms,
                    accepted: false,
                },
            );
        self.push_share(ClipboardShareView {
            offer_id: offer_id.to_string(),
            device_id: device_id.to_string(),
            direction: ClipboardDirection::Received,
            state: ClipboardShareState::Offered,
            preview: preview.clone(),
            size_bytes,
            created_at_ms: now,
            expires_at_ms,
            error_code: None,
            error_message: None,
        });
        vec![ClipboardEvent::OfferReceived {
            offer_id: offer_id.to_string(),
            device_id: device_id.to_string(),
            text_preview: preview,
            size_bytes,
            expires_at_ms,
        }]
    }

    pub fn accept(
        &self,
        device_id: Uuid,
        offer_id: Uuid,
    ) -> Result<(ClipboardAckDraft, ClipboardEvent), IpcError> {
        self.expire_stale();
        let mut incoming = self
            .incoming
            .lock()
            .map_err(|_| IpcError::internal("clipboard state unavailable"))?;
        let Some(pending) = incoming.get_mut(&offer_id) else {
            return Err(offer_expired());
        };
        if pending.device_id != device_id {
            return Err(IpcError::internal(
                "clipboard offer belongs to another device",
            ));
        }
        if pending.expires_at_ms <= now_ms() {
            return Err(offer_expired());
        }
        pending.accepted = true;
        drop(incoming);
        self.update_share(offer_id, |share| {
            share.state = ClipboardShareState::Accepted;
        });
        Ok((
            ClipboardAckDraft {
                offer_id,
                accepted: true,
                applied: false,
                code: String::new(),
                message: String::new(),
            },
            ClipboardEvent::Accepted {
                offer_id: offer_id.to_string(),
                device_id: device_id.to_string(),
            },
        ))
    }

    pub fn reject(
        &self,
        device_id: Uuid,
        offer_id: Uuid,
    ) -> Result<(ClipboardAckDraft, ClipboardEvent), IpcError> {
        let removed = self
            .incoming
            .lock()
            .map_err(|_| IpcError::internal("clipboard state unavailable"))?
            .remove(&offer_id);
        let Some(pending) = removed else {
            return Err(offer_expired());
        };
        if pending.device_id != device_id {
            return Err(IpcError::internal(
                "clipboard offer belongs to another device",
            ));
        }
        self.update_share(offer_id, |share| {
            share.state = ClipboardShareState::Rejected;
        });
        Ok((
            ClipboardAckDraft {
                offer_id,
                accepted: false,
                applied: false,
                code: "rejected".into(),
                message: "the receiver rejected the clipboard offer".into(),
            },
            ClipboardEvent::Rejected {
                offer_id: offer_id.to_string(),
                device_id: device_id.to_string(),
            },
        ))
    }

    pub fn apply_text(
        &self,
        device_id: Uuid,
        offer_id: Uuid,
        text: String,
        digest: [u8; 32],
    ) -> (Vec<ClipboardEvent>, ClipboardAckDraft) {
        self.expire_stale();
        let pending = self.incoming.lock().ok().and_then(|incoming| {
            incoming.get(&offer_id).map(|pending| {
                (
                    pending.device_id,
                    pending.digest,
                    pending.size_bytes,
                    pending.expires_at_ms,
                    pending.accepted,
                )
            })
        });
        let Some((pending_device, pending_digest, pending_size, expires_at, accepted)) = pending
        else {
            return (
                Vec::new(),
                failure_ack(
                    offer_id,
                    "offer_not_pending",
                    "clipboard offer is no longer pending",
                ),
            );
        };
        if pending_device != device_id || !accepted {
            return (
                Vec::new(),
                failure_ack(
                    offer_id,
                    "offer_not_accepted",
                    "clipboard offer was not accepted on this device",
                ),
            );
        }
        if expires_at <= now_ms() {
            if let Ok(mut incoming) = self.incoming.lock() {
                incoming.remove(&offer_id);
            }
            return (
                Vec::new(),
                failure_ack(offer_id, "offer_expired", "clipboard offer expired"),
            );
        }
        if let Ok(mut incoming) = self.incoming.lock() {
            incoming.remove(&offer_id);
        }
        if pending_size != text.len() as u64 {
            self.mark_share_failed(
                offer_id,
                "size_mismatch",
                "clipboard text size did not match",
            );
            return (
                vec![ClipboardEvent::ApplyFailed {
                    offer_id: offer_id.to_string(),
                    device_id: device_id.to_string(),
                    code: "size_mismatch".into(),
                    message: "clipboard text size did not match the offer".into(),
                }],
                failure_ack(
                    offer_id,
                    "size_mismatch",
                    "clipboard text size did not match",
                ),
            );
        }
        if *blake3::hash(text.as_bytes()).as_bytes() != digest || digest != pending_digest {
            self.mark_share_failed(
                offer_id,
                "digest_mismatch",
                "clipboard text digest did not match",
            );
            return (
                vec![ClipboardEvent::ApplyFailed {
                    offer_id: offer_id.to_string(),
                    device_id: device_id.to_string(),
                    code: "digest_mismatch".into(),
                    message: "clipboard text digest did not match the offer".into(),
                }],
                failure_ack(
                    offer_id,
                    "digest_mismatch",
                    "clipboard text digest did not match",
                ),
            );
        }
        if !self.claim_applied(digest) {
            self.mark_share_failed(
                offer_id,
                "duplicate_content",
                "this clipboard content was already applied recently",
            );
            return (
                vec![ClipboardEvent::ApplyFailed {
                    offer_id: offer_id.to_string(),
                    device_id: device_id.to_string(),
                    code: "duplicate_content".into(),
                    message: "this clipboard content was already applied recently".into(),
                }],
                failure_ack(
                    offer_id,
                    "duplicate_content",
                    "this clipboard content was already applied recently",
                ),
            );
        }
        match self.backend.write_text(&text) {
            Ok(()) => {
                self.update_share(offer_id, |share| {
                    share.state = ClipboardShareState::Applied;
                    share.error_code = None;
                    share.error_message = None;
                });
                (
                    vec![ClipboardEvent::Applied {
                        offer_id: offer_id.to_string(),
                        device_id: device_id.to_string(),
                    }],
                    ClipboardAckDraft {
                        offer_id,
                        accepted: true,
                        applied: true,
                        code: String::new(),
                        message: String::new(),
                    },
                )
            }
            Err(error) => {
                self.release_applied(digest);
                self.mark_share_failed(offer_id, "clipboard_write_failed", &error);
                (
                    vec![ClipboardEvent::ApplyFailed {
                        offer_id: offer_id.to_string(),
                        device_id: device_id.to_string(),
                        code: "clipboard_write_failed".into(),
                        message: error.clone(),
                    }],
                    failure_ack(offer_id, "clipboard_write_failed", &error),
                )
            }
        }
    }

    pub fn receive_ack(
        &self,
        device_id: Uuid,
        offer_id: Uuid,
        accepted: bool,
        applied: bool,
        code: String,
        message: String,
    ) -> (Vec<ClipboardEvent>, Option<ClipboardFollowUp>) {
        self.expire_stale();
        let pending_device = self
            .outgoing
            .lock()
            .ok()
            .and_then(|outgoing| outgoing.get(&offer_id).map(|pending| pending.device_id));
        let Some(pending_device) = pending_device else {
            return (Vec::new(), None);
        };
        if pending_device != device_id {
            return (Vec::new(), None);
        }
        if !accepted {
            self.remove_outgoing(offer_id);
            self.update_share(offer_id, |share| {
                share.state = ClipboardShareState::Rejected;
            });
            return (
                vec![ClipboardEvent::Rejected {
                    offer_id: offer_id.to_string(),
                    device_id: device_id.to_string(),
                }],
                None,
            );
        }
        if applied {
            self.remove_outgoing(offer_id);
            let failed = !code.is_empty();
            self.update_share(offer_id, |share| {
                share.state = if failed {
                    ClipboardShareState::Failed
                } else {
                    ClipboardShareState::Applied
                };
                share.error_code = failed.then(|| code.clone());
                share.error_message = failed.then(|| message.clone());
            });
            return (
                vec![if failed {
                    ClipboardEvent::ApplyFailed {
                        offer_id: offer_id.to_string(),
                        device_id: device_id.to_string(),
                        code,
                        message,
                    }
                } else {
                    ClipboardEvent::Applied {
                        offer_id: offer_id.to_string(),
                        device_id: device_id.to_string(),
                    }
                }],
                None,
            );
        }
        if code.is_empty() {
            self.update_share(offer_id, |share| {
                share.state = ClipboardShareState::Sending;
            });
            let text = self.outgoing.lock().ok().and_then(|outgoing| {
                outgoing
                    .get(&offer_id)
                    .map(|item| (item.text.clone(), item.digest))
            });
            return (
                vec![ClipboardEvent::Accepted {
                    offer_id: offer_id.to_string(),
                    device_id: device_id.to_string(),
                }],
                text.map(|(text, digest)| ClipboardFollowUp::SendText {
                    offer_id,
                    text,
                    digest,
                }),
            );
        }
        self.remove_outgoing(offer_id);
        self.mark_share_failed(offer_id, &code, &message);
        (
            vec![ClipboardEvent::ApplyFailed {
                offer_id: offer_id.to_string(),
                device_id: device_id.to_string(),
                code,
                message,
            }],
            None,
        )
    }

    pub fn mark_failed(&self, offer_id: Uuid, code: &str, message: &str) -> ClipboardEvent {
        self.remove_outgoing(offer_id);
        self.mark_share_failed(offer_id, code, message);
        ClipboardEvent::ApplyFailed {
            offer_id: offer_id.to_string(),
            device_id: self
                .find_share(offer_id)
                .map(|share| share.device_id)
                .unwrap_or_default(),
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn revoke(&self, device_id: Uuid) {
        if let Ok(mut shares) = self.shares.lock() {
            shares.retain(|share| share.device_id != device_id.to_string());
        }
        if let Ok(mut incoming) = self.incoming.lock() {
            incoming.retain(|_, pending| pending.device_id != device_id);
        }
        if let Ok(mut outgoing) = self.outgoing.lock() {
            outgoing.retain(|_, pending| pending.device_id != device_id);
        }
    }

    pub fn reset(&self) {
        if let Ok(mut shares) = self.shares.lock() {
            shares.clear();
        }
        if let Ok(mut incoming) = self.incoming.lock() {
            incoming.clear();
        }
        if let Ok(mut outgoing) = self.outgoing.lock() {
            outgoing.clear();
        }
        if let Ok(mut recent) = self.recent_applied.lock() {
            recent.clear();
        }
    }

    fn expire_stale(&self) {
        let now = now_ms();
        let expired = self
            .shares
            .lock()
            .map(|shares| {
                shares
                    .iter()
                    .filter(|share| {
                        share.expires_at_ms <= now
                            && matches!(
                                share.state,
                                ClipboardShareState::Offered
                                    | ClipboardShareState::Accepted
                                    | ClipboardShareState::Sending
                            )
                    })
                    .map(|share| share.offer_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for offer_id in expired {
            let Ok(id) = Uuid::parse_str(&offer_id) else {
                continue;
            };
            if let Ok(mut incoming) = self.incoming.lock() {
                incoming.remove(&id);
            }
            if let Ok(mut outgoing) = self.outgoing.lock() {
                outgoing.remove(&id);
            }
            self.update_share(id, |share| {
                share.state = ClipboardShareState::Expired;
            });
        }
    }

    fn shares_contains(&self, offer_id: Uuid) -> bool {
        self.shares
            .lock()
            .map(|shares| {
                shares
                    .iter()
                    .any(|share| share.offer_id == offer_id.to_string())
            })
            .unwrap_or(false)
    }

    fn find_share(&self, offer_id: Uuid) -> Option<ClipboardShareView> {
        self.shares.lock().ok().and_then(|shares| {
            shares
                .iter()
                .find(|share| share.offer_id == offer_id.to_string())
                .cloned()
        })
    }

    fn push_share(&self, share: ClipboardShareView) {
        if let Ok(mut shares) = self.shares.lock() {
            shares.push_back(share);
            while shares.len() > MAX_SHARES {
                shares.pop_front();
            }
        }
    }

    fn update_share(&self, offer_id: Uuid, update: impl FnOnce(&mut ClipboardShareView)) {
        if let Ok(mut shares) = self.shares.lock()
            && let Some(share) = shares
                .iter_mut()
                .find(|share| share.offer_id == offer_id.to_string())
        {
            update(share);
        }
    }

    fn remove_outgoing(&self, offer_id: Uuid) {
        if let Ok(mut outgoing) = self.outgoing.lock() {
            outgoing.remove(&offer_id);
        }
    }

    fn mark_share_failed(&self, offer_id: Uuid, code: &str, message: &str) {
        self.update_share(offer_id, |share| {
            share.state = ClipboardShareState::Failed;
            share.error_code = Some(code.into());
            share.error_message = Some(message.into());
        });
    }

    fn trim_pending(&self) {
        let removed = if let Ok(mut outgoing) = self.outgoing.lock()
            && outgoing.len() > MAX_PENDING_OFFERS
            && let Some(oldest) = outgoing.keys().min().copied()
        {
            outgoing.remove(&oldest);
            Some(oldest)
        } else {
            None
        };
        if let Some(offer_id) = removed {
            self.update_share(offer_id, |share| {
                share.state = ClipboardShareState::Expired;
            });
        }
    }
}

fn prune_recent(recent: &mut VecDeque<RecentDigest>) {
    let cutoff = now_ms() - RECENT_DIGEST_TTL_MS;
    while recent.front().is_some_and(|entry| entry.at_ms < cutoff) {
        recent.pop_front();
    }
}

fn failure_ack(offer_id: Uuid, code: &str, message: &str) -> ClipboardAckDraft {
    ClipboardAckDraft {
        offer_id,
        accepted: true,
        applied: false,
        code: code.into(),
        message: message.into(),
    }
}

/// How often the desktop watcher reads the system clipboard. The plan asks for
/// a 300 ms debounced read.
pub const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(300);
/// A clipboard value is only sent once it has looked the same for this long, so
/// a value that is still being written (or a burst of copies) is never sent
/// half-way.
pub const CLIPBOARD_DEBOUNCE: Duration = Duration::from_millis(300);

/// The automatic clipboard-send watcher. It owns no state that outlives the
/// process and never talks to the network: it answers one question per tick -
/// "should this text be sent now?" - which keeps the loop-prevention rules
/// testable without a live peer.
#[derive(Debug, Default)]
pub struct ClipboardWatcher {
    /// The digest seen last and when it first appeared.
    candidate: Option<([u8; 32], Instant)>,
    /// The digest that was already handed to the sender. The same content is
    /// never auto-sent twice.
    sent: Option<[u8; 32]>,
}

impl ClipboardWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the pending candidate. Used when the switch is turned off so a
    /// later enable starts from a fresh observation.
    pub fn reset(&mut self) {
        self.candidate = None;
    }

    /// Record the value that is already on the clipboard without sending it.
    ///
    /// Turning the switch on must not immediately broadcast whatever happens to
    /// be on the clipboard - it may be a password copied before the user made
    /// that decision - so enabling only syncs what is copied from now on. The
    /// manual share button still sends the current value on demand.
    pub fn prime(&mut self, text: &str) {
        self.candidate = None;
        if text.is_empty() || text.len() > fluxy_net::MAX_CLIPBOARD_BYTES {
            return;
        }
        self.sent = Some(*blake3::hash(text.as_bytes()).as_bytes());
    }

    /// Decide whether `text` should be auto-sent at `now`.
    ///
    /// `recent_applied` answers whether a digest was written to the local
    /// clipboard by an incoming share. Received content is never sent back,
    /// which is the rule that makes an A -> B -> A loop impossible even when
    /// both sides watch their clipboard.
    pub fn poll(
        &mut self,
        text: &str,
        now: Instant,
        recent_applied: &dyn Fn(&[u8; 32]) -> bool,
    ) -> Option<String> {
        if text.is_empty() || text.len() > fluxy_net::MAX_CLIPBOARD_BYTES {
            self.candidate = None;
            return None;
        }
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        if recent_applied(&digest) {
            // The user just received this text; sending it back would echo.
            self.candidate = None;
            return None;
        }
        if self.sent == Some(digest) {
            self.candidate = None;
            return None;
        }
        match self.candidate {
            Some((seen, first_at)) if seen == digest => {
                if now.duration_since(first_at) >= CLIPBOARD_DEBOUNCE {
                    self.sent = Some(digest);
                    self.candidate = None;
                    return Some(text.to_string());
                }
                None
            }
            _ => {
                self.candidate = Some((digest, now));
                None
            }
        }
    }
}

fn clipboard_error(message: String) -> IpcError {
    IpcError {
        code: "clipboard_unavailable".into(),
        code_number: 4301,
        message,
        retryable: true,
    }
}

fn offer_expired() -> IpcError {
    IpcError {
        code: "offer_expired".into(),
        code_number: 4304,
        message: "the clipboard offer is no longer pending".into(),
        retryable: false,
    }
}

fn preview(text: &str) -> String {
    let mut value: String = text.chars().take(MAX_PREVIEW_CHARS).collect();
    if text.chars().count() > MAX_PREVIEW_CHARS {
        value.push('…');
    }
    value
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeClipboard {
        text: Mutex<String>,
        writes: AtomicUsize,
    }

    impl FakeClipboard {
        fn new() -> Self {
            Self {
                text: Mutex::new(String::new()),
                writes: AtomicUsize::new(0),
            }
        }
    }

    impl TextClipboard for FakeClipboard {
        fn read_text(&self) -> Result<String, String> {
            Ok(self.text.lock().unwrap().clone())
        }

        fn write_text(&self, text: &str) -> Result<(), String> {
            *self.text.lock().unwrap() = text.to_string();
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn fixture() -> (Arc<FakeClipboard>, ClipboardState) {
        let clipboard = Arc::new(FakeClipboard::new());
        let state = ClipboardState::new(clipboard.clone());
        (clipboard, state)
    }

    #[test]
    fn the_watcher_sends_only_after_the_value_has_been_stable() {
        let mut watcher = ClipboardWatcher::new();
        let none = |_: &[u8; 32]| false;
        let start = Instant::now();
        // First sighting only records the candidate.
        assert!(watcher.poll("hello", start, &none).is_none());
        // A different value restarts the debounce window.
        assert!(
            watcher
                .poll("hello!", start + CLIPBOARD_DEBOUNCE, &none)
                .is_none()
        );
        assert!(
            watcher
                .poll("hello!", start + CLIPBOARD_DEBOUNCE * 2, &none)
                .is_some()
        );
    }

    #[test]
    fn the_watcher_never_sends_the_same_content_twice() {
        let mut watcher = ClipboardWatcher::new();
        let none = |_: &[u8; 32]| false;
        let start = Instant::now();
        assert!(watcher.poll("hello", start, &none).is_none());
        assert!(
            watcher
                .poll("hello", start + CLIPBOARD_DEBOUNCE, &none)
                .is_some()
        );
        // The same text stays quiet even after a long time.
        assert!(
            watcher
                .poll("hello", start + CLIPBOARD_DEBOUNCE * 10, &none)
                .is_none()
        );
        // New content is a new candidate.
        assert!(
            watcher
                .poll("world", start + CLIPBOARD_DEBOUNCE * 11, &none)
                .is_none()
        );
        assert!(
            watcher
                .poll("world", start + CLIPBOARD_DEBOUNCE * 12, &none)
                .is_some()
        );
    }

    #[test]
    fn received_content_is_never_sent_back() {
        let mut watcher = ClipboardWatcher::new();
        let applied = *blake3::hash(b"from the peer").as_bytes();
        let recent = move |digest: &[u8; 32]| *digest == applied;
        let start = Instant::now();
        // The text is stable for a long time, so only the echo rule can stop it.
        assert!(watcher.poll("from the peer", start, &recent).is_none());
        assert!(
            watcher
                .poll("from the peer", start + CLIPBOARD_DEBOUNCE, &recent)
                .is_none()
        );
        assert!(
            watcher
                .poll("from the peer", start + CLIPBOARD_DEBOUNCE * 5, &recent)
                .is_none()
        );
        // Local content is unaffected by the ring.
        assert!(watcher.poll("typed here", start, &recent).is_none());
        assert!(
            watcher
                .poll("typed here", start + CLIPBOARD_DEBOUNCE, &recent)
                .is_some()
        );
    }

    #[test]
    fn enabling_the_watcher_does_not_send_what_is_already_on_the_clipboard() {
        let mut watcher = ClipboardWatcher::new();
        let none = |_: &[u8; 32]| false;
        let start = Instant::now();
        // The switch is turned on while a password is on the clipboard.
        watcher.prime("correct horse battery staple");
        assert!(
            watcher
                .poll("correct horse battery staple", start, &none)
                .is_none()
        );
        assert!(
            watcher
                .poll(
                    "correct horse battery staple",
                    start + CLIPBOARD_DEBOUNCE * 10,
                    &none
                )
                .is_none()
        );
        // What the user copies next is still sent.
        assert!(watcher.poll("next", start, &none).is_none());
        assert!(
            watcher
                .poll("next", start + CLIPBOARD_DEBOUNCE, &none)
                .is_some()
        );
    }

    #[test]
    fn the_watcher_ignores_empty_and_oversized_content() {
        let mut watcher = ClipboardWatcher::new();
        let none = |_: &[u8; 32]| false;
        let start = Instant::now();
        assert!(watcher.poll("", start, &none).is_none());
        assert!(
            watcher
                .poll("", start + CLIPBOARD_DEBOUNCE, &none)
                .is_none()
        );
        let huge = "x".repeat(fluxy_net::MAX_CLIPBOARD_BYTES + 1);
        assert!(watcher.poll(&huge, start, &none).is_none());
        assert!(
            watcher
                .poll(&huge, start + CLIPBOARD_DEBOUNCE, &none)
                .is_none()
        );
        // Turning the switch off and on again starts from a fresh observation.
        assert!(watcher.poll("fresh", start, &none).is_none());
        watcher.reset();
        assert!(
            watcher
                .poll("fresh", start + CLIPBOARD_DEBOUNCE, &none)
                .is_none()
        );
        assert!(
            watcher
                .poll("fresh", start + CLIPBOARD_DEBOUNCE * 2, &none)
                .is_some()
        );
    }

    #[test]
    fn outgoing_offer_waits_for_acceptance_before_text() {
        let (_, state) = fixture();
        let device = Uuid::now_v7();
        let draft = state.begin_outgoing(device, "hello".into()).unwrap();
        assert_eq!(
            state.share(draft.offer_id).unwrap().state,
            ClipboardShareState::Offered
        );

        let (events, follow_up) = state.receive_ack(
            device,
            draft.offer_id,
            true,
            false,
            String::new(),
            String::new(),
        );
        assert!(matches!(
            events.as_slice(),
            [ClipboardEvent::Accepted { .. }]
        ));
        match follow_up {
            Some(ClipboardFollowUp::SendText { text, digest, .. }) => {
                assert_eq!(text, "hello");
                assert_eq!(digest, draft.digest);
            }
            other => panic!("expected text follow up, got {other:?}"),
        }

        let (events, follow_up) = state.receive_ack(
            device,
            draft.offer_id,
            true,
            true,
            String::new(),
            String::new(),
        );
        assert!(matches!(
            events.as_slice(),
            [ClipboardEvent::Applied { .. }]
        ));
        assert!(follow_up.is_none());
        assert_eq!(
            state.share(draft.offer_id).unwrap().state,
            ClipboardShareState::Applied
        );
    }

    #[test]
    fn incoming_text_is_not_written_before_acceptance() {
        let (clipboard, state) = fixture();
        let device = Uuid::now_v7();
        let digest = *blake3::hash(b"secret").as_bytes();
        let offers =
            state.receive_offer(device, Uuid::now_v7(), 6, digest, "secret".into(), 120_000);
        let offer_id = match offers.as_slice() {
            [ClipboardEvent::OfferReceived { offer_id, .. }] => Uuid::parse_str(offer_id).unwrap(),
            other => panic!("expected offer event, got {other:?}"),
        };
        let (_, ack) = state.apply_text(device, offer_id, "secret".into(), digest);
        assert_eq!(ack.code, "offer_not_accepted");
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);
        assert!(clipboard.read_text().unwrap().is_empty());
    }

    #[test]
    fn accepted_text_is_verified_and_written_exactly_once() {
        let (clipboard, state) = fixture();
        let device = Uuid::now_v7();
        let text = "line one\nline two";
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        let offer_id = Uuid::now_v7();
        state.receive_offer(
            device,
            offer_id,
            text.len() as u64,
            digest,
            text.into(),
            120_000,
        );
        let (ack, _) = state.accept(device, offer_id).unwrap();
        assert!(ack.accepted);

        let (events, ack) = state.apply_text(device, offer_id, text.into(), digest);
        assert!(ack.applied);
        assert!(matches!(
            events.as_slice(),
            [ClipboardEvent::Applied { .. }]
        ));
        assert_eq!(clipboard.read_text().unwrap(), text);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.share(offer_id).unwrap().state,
            ClipboardShareState::Applied
        );
    }

    #[test]
    fn applied_content_is_not_written_twice_or_sent_back() {
        let (clipboard, state) = fixture();
        let device = Uuid::now_v7();
        let text = "loop guard";
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        let first = Uuid::now_v7();
        state.receive_offer(
            device,
            first,
            text.len() as u64,
            digest,
            text.into(),
            120_000,
        );
        state.accept(device, first).unwrap();
        let (_, ack) = state.apply_text(device, first, text.into(), digest);
        assert!(ack.applied);
        assert!(state.is_recent_applied(&digest));

        let second = Uuid::now_v7();
        state.receive_offer(
            device,
            second,
            text.len() as u64,
            digest,
            text.into(),
            120_000,
        );
        state.accept(device, second).unwrap();
        let (events, ack) = state.apply_text(device, second, text.into(), digest);
        assert!(!ack.applied);
        assert_eq!(ack.code, "duplicate_content");
        assert!(matches!(
            events.as_slice(),
            [ClipboardEvent::ApplyFailed { .. }]
        ));
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 1);

        let error = state.begin_outgoing(device, text.into()).unwrap_err();
        assert_eq!(error.code, "clipboard_duplicate");
        assert!(state.begin_outgoing(device, "fresh text".into()).is_ok());
    }

    #[test]
    fn digest_mismatch_never_writes() {
        let (clipboard, state) = fixture();
        let device = Uuid::now_v7();
        let text = "expected";
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        let offer_id = Uuid::now_v7();
        state.receive_offer(
            device,
            offer_id,
            text.len() as u64,
            digest,
            "expected".into(),
            120_000,
        );
        state.accept(device, offer_id).unwrap();

        let (events, ack) = state.apply_text(device, offer_id, "tampered".into(), digest);
        assert!(!ack.applied);
        assert_eq!(ack.code, "digest_mismatch");
        assert!(matches!(
            events.as_slice(),
            [ClipboardEvent::ApplyFailed { .. }]
        ));
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn rejection_sends_decline_and_keeps_clipboard_untouched() {
        let (clipboard, state) = fixture();
        let device = Uuid::now_v7();
        let offer_id = Uuid::now_v7();
        state.receive_offer(
            device,
            offer_id,
            4,
            *blake3::hash(b"nope").as_bytes(),
            "nope".into(),
            120_000,
        );
        let (ack, event) = state.reject(device, offer_id).unwrap();
        assert!(!ack.accepted);
        assert!(matches!(event, ClipboardEvent::Rejected { .. }));
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);
        assert_eq!(
            state.share(offer_id).unwrap().state,
            ClipboardShareState::Rejected
        );
    }

    #[test]
    fn oversized_offer_is_ignored() {
        let (_, state) = fixture();
        let events = state.receive_offer(
            Uuid::now_v7(),
            Uuid::now_v7(),
            (fluxy_net::MAX_CLIPBOARD_BYTES + 1) as u64,
            [0u8; 32],
            "too big".into(),
            120_000,
        );
        assert!(events.is_empty());
        assert!(state.snapshot().shares.is_empty());
    }

    #[test]
    fn revoke_drops_the_peer_share() {
        let (_, state) = fixture();
        let device = Uuid::now_v7();
        let offer_id = Uuid::now_v7();
        state.receive_offer(
            device,
            offer_id,
            2,
            *blake3::hash(b"hi").as_bytes(),
            "hi".into(),
            120_000,
        );
        state.revoke(device);
        assert!(state.share(offer_id).is_none());
    }
}
