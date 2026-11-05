//! In-memory chat sessions over the trusted QUIC control stream.
//!
//! Chat content is deliberately memory-only. The runtime keeps a bounded
//! transcript for the lifetime of the process, while trust and identity stay in
//! the engine. Nothing in this module writes a message body to disk.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

/// How many times a dial is retried when the peer's address changes underneath
/// it. Two moves in a row are already unusual; a third means something is flapping.
const REDIAL_ATTEMPTS: u32 = 3;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fluxy_app::Engine;
use fluxy_core::{Identity, TrustEntry, TrustStore};
use fluxy_net::{
    ClipboardShareOffer, ControlSender, ControlSession, PeerSession, SessionError, SessionEvent,
    connect_trusted,
};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::clipboard::{
    ClipboardFollowUp, ClipboardState, ClipboardWatcher, SystemClipboard, TextClipboard,
};
use crate::contract::{
    AppEvent, ChatConversationView, ChatDirection, ChatEvent, ChatMessageState, ChatMessageView,
    ChatSnapshot, ClipboardEvent, ClipboardReceivePolicy, ClipboardShareView, ClipboardSnapshot,
    EventHub, IpcError, PeerSummary,
};
use crate::notify::Notifier;
use crate::transfer::{TransferRuntime, TransferSession};

const MAX_MESSAGES: usize = 1000;
const MAX_TEXT_BYTES: usize = 32 * 1024;
const MODEL_PREVIEW_CHARS: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OfferDecision {
    Ask,
    Auto,
    Reject {
        code: &'static str,
        message: &'static str,
    },
}

fn decide_offer(policy: ClipboardReceivePolicy, echo: bool, duplicate: bool) -> OfferDecision {
    if echo {
        OfferDecision::Reject {
            code: "echo_suppressed",
            message: "the offer originated on this device",
        }
    } else if duplicate {
        OfferDecision::Reject {
            code: "duplicate_content",
            message: "the same clipboard content was already applied",
        }
    } else {
        match policy {
            ClipboardReceivePolicy::Ask => OfferDecision::Ask,
            ClipboardReceivePolicy::Auto => OfferDecision::Auto,
            ClipboardReceivePolicy::Reject => OfferDecision::Reject {
                code: "policy_reject",
                message: "the receiver policy rejects clipboard offers from this device",
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatPeer {
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    pub addr: SocketAddr,
}

struct SessionEntry {
    generation: u64,
    connected: bool,
    sender: mpsc::Sender<ChatCommand>,
    peer: ChatPeer,
    control: Mutex<Option<ControlSender>>,
    connection: Mutex<Option<quinn::Connection>>,
    session_id: Mutex<Option<Uuid>>,
}

#[derive(Debug)]
enum ChatCommand {
    Send {
        message_id: Uuid,
        text: String,
    },
    ClipboardOffer {
        offer_id: Uuid,
        size_bytes: u64,
        digest: [u8; 32],
        preview: String,
        ttl_ms: u64,
        origin_id: Uuid,
        hop: u32,
    },
    ClipboardAck {
        offer_id: Uuid,
        accepted: bool,
        applied: bool,
        code: String,
        message: String,
    },
    Close,
}

pub struct ChatRuntime {
    engine: Arc<Mutex<Engine>>,
    hub: Arc<EventHub>,
    messages: Mutex<VecDeque<ChatMessageView>>,
    sessions: Mutex<HashMap<Uuid, SessionEntry>>,
    clipboard: ClipboardState,
    notifier: Arc<Notifier>,
    transfer: Mutex<Option<Arc<TransferRuntime>>>,
    /// Asks the discovery layer where a peer answers right now. A session entry
    /// outlives the connection it was made for, so without this a recreated
    /// session inherits the address it was first created with and dials a port
    /// the peer left long ago.
    address_book: Mutex<Option<PeerResolver>>,
    next_generation: AtomicU64,
}

/// Returns a peer discovery currently knows about.
///
/// It carries the name and platform as well as the address because a cold start
/// has no session to copy them from: sending right after a share used to fail
/// with `peer_offline` purely because the session table was still empty.
pub type PeerResolver = Arc<dyn Fn(Uuid) -> Option<ChatPeer> + Send + Sync>;

impl ChatRuntime {
    pub fn new(engine: Arc<Mutex<Engine>>, hub: Arc<EventHub>) -> Arc<Self> {
        Self::with_notifier(engine, hub, Arc::new(Notifier::default()))
    }

    pub fn with_notifier(
        engine: Arc<Mutex<Engine>>,
        hub: Arc<EventHub>,
        notifier: Arc<Notifier>,
    ) -> Arc<Self> {
        Self::with_clipboard(engine, hub, notifier, Arc::new(SystemClipboard::new()))
    }

    /// Build a runtime over an explicit clipboard backend. The desktop shell
    /// passes the system clipboard; tests pass a fake one so the automatic
    /// watcher can be driven without a real clipboard server.
    pub fn with_clipboard(
        engine: Arc<Mutex<Engine>>,
        hub: Arc<EventHub>,
        notifier: Arc<Notifier>,
        backend: Arc<dyn TextClipboard>,
    ) -> Arc<Self> {
        Arc::new(Self {
            engine,
            hub,
            messages: Mutex::new(VecDeque::new()),
            sessions: Mutex::new(HashMap::new()),
            clipboard: ClipboardState::new(backend),
            notifier,
            transfer: Mutex::new(None),
            address_book: Mutex::new(None),
            next_generation: AtomicU64::new(1),
        })
    }

    /// Install the discovery-backed address lookup. Called by the network runtime
    /// once both halves exist.
    pub fn set_address_book(&self, resolver: PeerResolver) {
        if let Ok(mut slot) = self.address_book.lock() {
            *slot = Some(resolver);
        }
    }

    /// The address discovery last reported for a peer, if any.
    pub fn current_addr(&self, device_id: &Uuid) -> Option<SocketAddr> {
        self.address_book
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|resolve| resolve(*device_id)))
            .map(|peer| peer.addr)
    }

    /// The address discovery last reported for a peer, or the caller's fallback.
    fn freshest(&self, device_id: &Uuid, fallback: SocketAddr) -> SocketAddr {
        self.current_addr(device_id).unwrap_or(fallback)
    }

    pub fn snapshot(&self, trusted: &[TrustEntry], peers: &[PeerSummary]) -> ChatSnapshot {
        let messages = self
            .messages
            .lock()
            .map(|messages| messages.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let sessions = self
            .sessions
            .lock()
            .map(|sessions| {
                sessions
                    .iter()
                    .map(|(id, entry)| (*id, entry.connected))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut conversations = trusted
            .iter()
            .map(|entry| {
                let id = entry.device_id;
                let summary = peers.iter().find(|peer| peer.device_id == id.to_string());
                let connected = sessions.get(&id).copied().unwrap_or(false);
                let session_state = if connected {
                    "connected"
                } else if sessions.contains_key(&id) {
                    "connecting"
                } else {
                    "idle"
                };
                let last = messages
                    .iter()
                    .rev()
                    .find(|message| message.device_id == id.to_string());
                ChatConversationView {
                    device_id: id.to_string(),
                    name: entry.name.clone(),
                    platform: entry.platform.clone(),
                    online: summary.map(|peer| peer.online).unwrap_or(false) || connected,
                    session_state: session_state.into(),
                    last_message: last.map(|message| preview(&message.text)),
                    last_at_ms: last.map(|message| message.created_at_ms).unwrap_or(0),
                }
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|a, b| {
            b.last_at_ms
                .cmp(&a.last_at_ms)
                .then_with(|| a.name.cmp(&b.name))
        });
        ChatSnapshot {
            conversations,
            messages,
        }
    }

    pub fn open(self: &Arc<Self>, peer: ChatPeer) -> Result<(), IpcError> {
        self.check_trusted(peer.device_id)?;
        self.ensure_session(peer)?;
        Ok(())
    }

    pub fn clipboard_snapshot(&self) -> ClipboardSnapshot {
        self.clipboard.snapshot()
    }

    pub async fn share_clipboard(
        self: &Arc<Self>,
        peer: ChatPeer,
    ) -> Result<ClipboardShareView, IpcError> {
        let text = self.clipboard.read_local_text()?;
        self.share_clipboard_text(peer, text).await
    }

    /// One tick of the automatic clipboard watcher. Returns the text to send
    /// when a fresh, stable, locally-authored value is on the clipboard.
    ///
    /// Content that arrived from a peer is filtered by the applied-digest ring,
    /// so it is never offered back. Together with the origin/hop fields on the
    /// wire that is what makes a loop between two watching devices impossible.
    pub fn clipboard_watch_tick(
        &self,
        watcher: &mut ClipboardWatcher,
        now: Instant,
    ) -> Option<String> {
        let text = self.clipboard.read_local_text().ok()?;
        watcher.poll(&text, now, &|digest| {
            self.clipboard.is_recent_applied(digest)
        })
    }

    /// Record what is already on the clipboard when the watcher is switched on,
    /// so enabling does not send content the user copied before deciding.
    pub fn prime_clipboard_watch(&self, watcher: &mut ClipboardWatcher) {
        if let Ok(text) = self.clipboard.read_local_text() {
            watcher.prime(&text);
        }
    }

    /// Offer an explicit text value to one peer. The automatic watcher uses
    /// this so the value it validated is the value that is offered.
    pub async fn share_clipboard_text(
        self: &Arc<Self>,
        peer: ChatPeer,
        text: String,
    ) -> Result<ClipboardShareView, IpcError> {
        self.check_trusted(peer.device_id)?;
        let origin_id = self
            .identity()
            .map_err(|error| IpcError::internal(format!("identity unavailable: {error}")))?
            .device_id();
        let draft = self.clipboard.begin_outgoing(peer.device_id, text)?;
        self.emit_clipboard(ClipboardEvent::Offered {
            offer_id: draft.offer_id.to_string(),
            device_id: peer.device_id.to_string(),
            preview: draft.preview.clone(),
            size_bytes: draft.size_bytes,
            expires_at_ms: draft.view.expires_at_ms,
        });
        let sender = match self.ensure_session(peer.clone()) {
            Ok(sender) => sender,
            Err(error) => {
                let event = self
                    .clipboard
                    .mark_failed(draft.offer_id, &error.code, &error.message);
                self.emit_clipboard(event);
                return Err(error);
            }
        };
        if sender
            .send(ChatCommand::ClipboardOffer {
                offer_id: draft.offer_id,
                size_bytes: draft.size_bytes,
                digest: draft.digest,
                preview: draft.preview,
                ttl_ms: draft.ttl_ms,
                origin_id,
                hop: 0,
            })
            .await
            .is_err()
        {
            let event = self.clipboard.mark_failed(
                draft.offer_id,
                "session_closed",
                "the chat session closed before the clipboard offer was queued",
            );
            self.emit_clipboard(event);
            return Err(IpcError {
                code: "session_closed".into(),
                code_number: 4206,
                message: "the chat session closed before the clipboard offer was queued".into(),
                retryable: true,
            });
        }
        Ok(draft.view)
    }

    pub async fn accept_clipboard(
        self: &Arc<Self>,
        peer: ChatPeer,
        offer_id: Uuid,
    ) -> Result<ClipboardShareView, IpcError> {
        self.check_trusted(peer.device_id)?;
        let (ack, event) = self.clipboard.accept(peer.device_id, offer_id)?;
        self.emit_clipboard(event);
        let sender = match self.ensure_session(peer.clone()) {
            Ok(sender) => sender,
            Err(error) => {
                let event = self
                    .clipboard
                    .mark_failed(offer_id, &error.code, &error.message);
                self.emit_clipboard(event);
                return Err(error);
            }
        };
        if sender
            .send(ChatCommand::ClipboardAck {
                offer_id: ack.offer_id,
                accepted: ack.accepted,
                applied: ack.applied,
                code: ack.code,
                message: ack.message,
            })
            .await
            .is_err()
        {
            let event = self.clipboard.mark_failed(
                offer_id,
                "session_closed",
                "the chat session closed before the clipboard decision was queued",
            );
            self.emit_clipboard(event);
            return Err(IpcError {
                code: "session_closed".into(),
                code_number: 4206,
                message: "the chat session closed before the clipboard decision was queued".into(),
                retryable: true,
            });
        }
        self.clipboard
            .share(offer_id)
            .ok_or_else(|| IpcError::internal("clipboard offer disappeared after it was accepted"))
    }

    pub async fn reject_clipboard(
        self: &Arc<Self>,
        peer: ChatPeer,
        offer_id: Uuid,
    ) -> Result<ClipboardShareView, IpcError> {
        self.check_trusted(peer.device_id)?;
        let (ack, event) = self.clipboard.reject(peer.device_id, offer_id)?;
        self.emit_clipboard(event);
        let sender = self.ensure_session(peer.clone())?;
        let _ = sender
            .send(ChatCommand::ClipboardAck {
                offer_id: ack.offer_id,
                accepted: ack.accepted,
                applied: ack.applied,
                code: ack.code,
                message: ack.message,
            })
            .await;
        self.clipboard
            .share(offer_id)
            .ok_or_else(|| IpcError::internal("clipboard offer disappeared after it was rejected"))
    }

    pub async fn send(
        self: &Arc<Self>,
        peer: ChatPeer,
        text: String,
    ) -> Result<ChatMessageView, IpcError> {
        self.check_trusted(peer.device_id)?;
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(chat_error(
                "empty_message",
                4203,
                "message text cannot be empty",
                false,
            ));
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(chat_error(
                "message_too_large",
                4203,
                "message exceeds the 32 KiB text limit",
                false,
            ));
        }
        let message_id = Uuid::now_v7();
        let message = ChatMessageView {
            message_id: message_id.to_string(),
            device_id: peer.device_id.to_string(),
            direction: ChatDirection::Sent,
            text: text.clone(),
            state: ChatMessageState::Sending,
            created_at_ms: now_ms(),
            sequence: 0,
            copy_hint: false,
            error_code: None,
            error_message: None,
            retryable: false,
        };
        self.push_message(message.clone());
        let sender = match self.ensure_session(peer.clone()) {
            Ok(sender) => sender,
            Err(error) => {
                self.mark_failed(message_id, &error.code, &error.message, error.retryable);
                return Err(error);
            }
        };
        if sender
            .send(ChatCommand::Send { message_id, text })
            .await
            .is_err()
        {
            self.mark_failed(
                message_id,
                "session_closed",
                "the chat session closed before the message was queued",
                true,
            );
        }
        Ok(self.message(message_id).unwrap_or(message))
    }

    pub async fn retry(
        self: &Arc<Self>,
        peer: ChatPeer,
        message_id: Uuid,
    ) -> Result<ChatMessageView, IpcError> {
        self.check_trusted(peer.device_id)?;
        let message = self.message(message_id).ok_or_else(|| {
            chat_error(
                "message_not_found",
                4204,
                "chat message is no longer in memory",
                false,
            )
        })?;
        if message.device_id != peer.device_id.to_string()
            || !matches!(message.state, ChatMessageState::Failed)
        {
            return Err(chat_error(
                "message_not_retryable",
                4205,
                "only a failed message can be retried",
                false,
            ));
        }
        self.update_state(message_id, ChatMessageState::Sending, None, None, false);
        self.hub.bump_snapshot();
        let sender = match self.ensure_session(peer.clone()) {
            Ok(sender) => sender,
            Err(error) => {
                self.mark_failed(message_id, &error.code, &error.message, error.retryable);
                return Err(error);
            }
        };
        let retry_text = message.text.clone();
        if sender
            .send(ChatCommand::Send {
                message_id,
                text: retry_text,
            })
            .await
            .is_err()
        {
            self.mark_failed(
                message_id,
                "session_closed",
                "the chat session closed before the retry was queued",
                true,
            );
        }
        Ok(self.message(message_id).unwrap_or(message))
    }

    /// Adopt a trusted incoming control connection. The caller has already
    /// completed the signed resume handshake.
    pub fn accept(self: &Arc<Self>, connection: quinn::Connection, peer: PeerSession) {
        // Every early return here closes the connection, and without a log the
        // peer only sees "control stream closed" with no way to tell why.
        tracing::info!(peer = %peer.device_id, "chat: accepting an inbound session");
        if self.check_trusted(peer.device_id).is_err() {
            tracing::warn!(peer = %peer.device_id, "chat: inbound rejected, peer is not trusted");
            connection.close(0u32.into(), b"untrusted");
            return;
        }
        let local_id = match self.identity() {
            Ok(identity) => identity.device_id(),
            Err(_) => {
                tracing::warn!("chat: inbound rejected, identity unavailable");
                connection.close(0u32.into(), b"identity-unavailable");
                return;
            }
        };
        let (sender, receiver) = mpsc::channel(32);
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        let replaced = {
            let mut sessions = match self.sessions.lock() {
                Ok(sessions) => sessions,
                Err(_) => {
                    connection.close(0u32.into(), b"chat-unavailable");
                    return;
                }
            };
            // Two desktops may open outbound sessions at the same time. The
            // lexicographically smaller device keeps its outbound session; the
            // larger one accepts the inbound connection and replaces its own
            // outbound attempt.
            // Only defer to an outbound session that is actually alive. A dead
            // one - its worker gone after a heartbeat failure - used to win this
            // check forever, so the smaller-id side kept rejecting the inbound
            // connection while holding a session that no longer existed, and the
            // two peers could never meet again.
            let incumbent_alive = sessions
                .get(&peer.device_id)
                .is_some_and(|entry| !entry.sender.is_closed());
            if local_id < peer.device_id && incumbent_alive {
                tracing::info!(
                    peer = %peer.device_id,
                    "chat: inbound deferred, this device keeps its outbound session"
                );
                drop(sessions);
                connection.close(0u32.into(), b"chat-session-active");
                return;
            }
            let replaced = sessions.remove(&peer.device_id);
            sessions.insert(
                peer.device_id,
                SessionEntry {
                    generation,
                    connected: false,
                    sender,
                    peer: ChatPeer {
                        device_id: peer.device_id,
                        name: peer.name.clone(),
                        platform: peer.platform.clone(),
                        // connection.remote_address() is the peer's *ephemeral*
                        // source port, not the port it listens on. Recording it
                        // meant the next outbound dial went to a dead socket:
                        // the session looked live and every message vanished.
                        // Discovery knows the real listening port, so prefer it.
                        addr: self.freshest(&peer.device_id, connection.remote_address()),
                    },
                    control: Mutex::new(None),
                    connection: Mutex::new(None),
                    session_id: Mutex::new(None),
                },
            );
            replaced
        };
        if let Some(old) = replaced {
            let sender = old.sender;
            tauri::async_runtime::spawn(async move {
                let _ = sender.send(ChatCommand::Close).await;
            });
            self.fail_pending(
                peer.device_id,
                "session_replaced",
                "the chat session was replaced by an inbound connection",
                true,
            );
            self.emit(ChatEvent::Disconnected {
                device_id: peer.device_id.to_string(),
                reason: "session replaced by inbound connection".into(),
                at_ms: now_ms(),
            });
        }
        self.emit(ChatEvent::Connecting {
            device_id: peer.device_id.to_string(),
        });
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            runtime
                .run_accepted(connection, peer, receiver, generation)
                .await;
        });
    }

    pub fn revoke(&self, device_id: Uuid) {
        let entry = self
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(&device_id));
        if let Some(entry) = entry {
            let sender = entry.sender;
            tauri::async_runtime::spawn(async move {
                let _ = sender.send(ChatCommand::Close).await;
            });
        }
        self.fail_pending(
            device_id,
            "trust_removed",
            "trust was removed for this device",
            false,
        );
        self.clipboard.revoke(device_id);
        self.emit(ChatEvent::Disconnected {
            device_id: device_id.to_string(),
            reason: "trust removed".into(),
            at_ms: now_ms(),
        });
    }

    pub fn stop(&self) {
        let entries = self
            .sessions
            .lock()
            .map(|mut sessions| sessions.drain().map(|(_, entry)| entry).collect::<Vec<_>>())
            .unwrap_or_default();
        for entry in entries {
            tauri::async_runtime::spawn(async move {
                let _ = entry.sender.send(ChatCommand::Close).await;
            });
        }
    }

    /// Stop every session and discard the memory-only transcript. Used when
    /// the network identity is rotated or the listener restarts: old trust no
    /// longer authorizes those peers.
    pub fn reset(&self) {
        self.stop();
        if let Ok(mut messages) = self.messages.lock() {
            messages.clear();
        }
        self.clipboard.reset();
        self.hub.bump_snapshot();
    }

    pub fn known_peer(&self, device_id: Uuid) -> Option<ChatPeer> {
        // Discovery first: a session stores the name and address it was opened
        // with, both of which go stale when the peer renames or restarts. Every
        // consumer of this - the transfer record's device name, the address a
        // dial uses - wants the current values, not the historical ones.
        let discovered = self
            .address_book
            .lock()
            .ok()
            .and_then(|book| book.as_ref().and_then(|resolve| resolve(device_id)));
        if discovered.is_some() {
            return discovered;
        }
        // Nothing in discovery (a share into a cold app, or a peer that stopped
        // advertising): fall back to whatever the session remembers.
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&device_id).map(|entry| entry.peer.clone()))
    }

    /// Keep a live session pointed at the address the peer currently answers on.
    /// A session outlives the connection it was opened with, and the other side
    /// rebinds its QUIC socket on every restart, so the stored address goes stale
    /// while discovery keeps learning the new one.
    pub fn update_peer_addr(&self, device_id: Uuid, addr: SocketAddr) {
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        let Some(entry) = sessions.get_mut(&device_id) else {
            return;
        };
        if entry.peer.addr == addr {
            return;
        }
        tracing::info!(%device_id, old = %entry.peer.addr, new = %addr, "peer moved; updating the chat session address");
        entry.peer.addr = addr;
        // The live connection is bound to the old socket; tear it down so the
        // next command dials the address discovery just learned. Without this the
        // session keeps writing to a port nobody owns and the peer never sees it.
        if let Ok(mut slot) = entry.connection.lock()
            && let Some(connection) = slot.take()
        {
            connection.close(0u32.into(), b"peer-moved");
        }
    }

    fn ensure_session(
        self: &Arc<Self>,
        peer: ChatPeer,
    ) -> Result<mpsc::Sender<ChatCommand>, IpcError> {
        // Discovery wins over whatever address the caller carried in.
        let peer = ChatPeer {
            addr: self.freshest(&peer.device_id, peer.addr),
            ..peer
        };
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| IpcError::internal("chat session registry unavailable"))?;
        // A session whose worker has exited keeps its entry until end_session
        // runs; reusing that dead sender silently drops every command, and the
        // caller then waits for a connection that will never be opened.
        let dead = sessions
            .get(&peer.device_id)
            .map(|entry| entry.sender.is_closed())
            .unwrap_or(false);
        if dead {
            sessions.remove(&peer.device_id);
        }
        if let Some(entry) = sessions.get(&peer.device_id) {
            return Ok(entry.sender.clone());
        }
        let (sender, receiver) = mpsc::channel(32);
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        sessions.insert(
            peer.device_id,
            SessionEntry {
                generation,
                connected: false,
                sender: sender.clone(),
                peer: peer.clone(),
                control: Mutex::new(None),
                connection: Mutex::new(None),
                session_id: Mutex::new(None),
            },
        );
        drop(sessions);
        self.emit(ChatEvent::Connecting {
            device_id: peer.device_id.to_string(),
        });
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            runtime.run_outbound(peer, receiver, generation).await;
        });
        Ok(sender)
    }

    async fn run_outbound(
        self: Arc<Self>,
        peer: ChatPeer,
        receiver: mpsc::Receiver<ChatCommand>,
        generation: u64,
    ) {
        let result = async {
            let (identity, tls, mut trust) = self.identity_and_trust()?;
            if !trust.is_trusted(&peer.device_id) {
                return Err("peer is no longer trusted".to_string());
            }
            for attempt in 0..REDIAL_ATTEMPTS {
                // Discovery may have learned a newer address between session
                // creation and this connect; the registry copy is the fresh one.
                let addr = self
                    .sessions
                    .lock()
                    .ok()
                    .and_then(|sessions| sessions.get(&peer.device_id).map(|entry| entry.peer.addr))
                    .unwrap_or(peer.addr);
                // Without this the whole connect path is silent: a refused dial,
                // a failed handshake and a healthy session all look the same from
                // the outside, and the command queue simply never gets read.
                tracing::info!(
                    device_id = %peer.device_id,
                    %addr,
                    attempt,
                    "chat session: dialling"
                );
                let session = connect_trusted(addr, peer.device_id, &identity, &tls, &mut trust)
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            device_id = %peer.device_id,
                            %addr,
                            attempt,
                            message = %error,
                            "chat session: connect failed"
                        );
                        error.to_string()
                    })?;
                tracing::info!(
                    device_id = %peer.device_id,
                    %addr,
                    attempt,
                    "chat session: connected"
                );
                // The peer can move again while the handshake is in flight. A
                // connection to the port it just left looks healthy and silently
                // swallows everything written to it, so check before trusting it.
                let live = self.sessions.lock().ok().and_then(|sessions| {
                    sessions.get(&peer.device_id).map(|entry| entry.peer.addr)
                });
                if live.is_none_or(|live| live == addr) {
                    return Ok(session);
                }
                tracing::info!(%addr, "the peer moved while dialling; redialling");
                session.connection().close(0u32.into(), b"peer-moved");
                if attempt + 1 == REDIAL_ATTEMPTS {
                    return Err("the peer kept moving while dialling".to_string());
                }
            }
            Err("unreachable".to_string())
        }
        .await;
        match result {
            Ok(session) => {
                self.run_connected(peer.device_id, session, receiver, generation)
                    .await;
            }
            Err(reason) => self.end_session(
                peer.device_id,
                generation,
                &format!("chat connect failed: {reason}"),
            ),
        }
    }

    async fn run_accepted(
        self: Arc<Self>,
        connection: quinn::Connection,
        peer: PeerSession,
        receiver: mpsc::Receiver<ChatCommand>,
        generation: u64,
    ) {
        let identity = match self.identity() {
            Ok(identity) => identity,
            Err(reason) => {
                tracing::warn!(reason = %reason, "chat: inbound identity unavailable");
                connection.close(0u32.into(), b"identity-unavailable");
                self.end_session(peer.device_id, generation, &reason);
                return;
            }
        };
        // Three statements sit between here and the first log inside
        // ControlSession::accept, so an entry/exit pair is enough to tell a
        // blocked accept_bi apart from a hang before it.
        tracing::info!(peer = %peer.device_id, "chat: waiting for the inbound control stream");
        match ControlSession::accept(&connection, &identity, &peer).await {
            Ok(session) => {
                tracing::info!(peer = %peer.device_id, "chat: inbound control stream accepted");
                self.run_connected(peer.device_id, session, receiver, generation)
                    .await;
            }
            Err(error) => {
                tracing::warn!(peer = %peer.device_id, message = %error, "chat: inbound accept failed");
                self.end_session(
                    peer.device_id,
                    generation,
                    &format!("chat accept failed: {error}"),
                );
            }
        }
    }

    async fn run_connected(
        self: Arc<Self>,
        device_id: Uuid,
        mut session: ControlSession,
        receiver: mpsc::Receiver<ChatCommand>,
        generation: u64,
    ) {
        let session_id = session.session_id().to_string();
        // The session carries the name the peer used when it was opened, which
        // is stale after a rename. Prefer what discovery reports now; the
        // session value is only a fallback for a peer that stopped advertising.
        let peer_name = self
            .known_peer(device_id)
            .map(|peer| peer.name)
            .unwrap_or_else(|| session.peer().name.clone());
        // Two peers dialling each other at the same time can end up each holding
        // a different connection; the id says which one this is.
        tracing::info!(
            %device_id,
            connection = session.connection().stable_id(),
            remote = %session.connection().remote_address(),
            "chat session live"
        );
        if !self.mark_connected(device_id, generation, &session_id, &session) {
            session.close().await;
            return;
        }
        let sender = session.sender();
        let command_runtime = self.clone();
        let command_task = tokio::spawn(async move {
            command_runtime.run_commands(sender, receiver).await;
        });

        let mut reason = "control session ended".to_string();
        while let Some(event) = session.next_event().await {
            match event {
                SessionEvent::Message {
                    message_id,
                    text,
                    sequence,
                    copy_hint,
                } => self.record_incoming(device_id, message_id, text, sequence, copy_hint),
                SessionEvent::Ack {
                    message_id,
                    delivered,
                } => self.record_ack(device_id, message_id, delivered),
                SessionEvent::ClipboardOffer {
                    offer_id,
                    size_bytes,
                    digest,
                    preview,
                    sequence: _,
                    ttl_ms,
                    origin_id,
                    hop: _hop,
                } => {
                    let local_id = self.identity().ok().map(|identity| identity.device_id());
                    let echo = origin_id.is_some() && origin_id == local_id;
                    let duplicate = self.clipboard.is_recent_applied(&digest);
                    let policy = self.clipboard_policy(device_id);
                    let events = self
                        .clipboard
                        .receive_offer(device_id, offer_id, size_bytes, digest, preview, ttl_ms);
                    let pending = !events.is_empty();
                    for event in events {
                        self.emit_clipboard(event);
                    }
                    if !pending {
                        continue;
                    }
                    let decision = decide_offer(policy, echo, duplicate);
                    if decision == OfferDecision::Ask {
                        self.notifier.notify_clipboard_offer(&peer_name);
                    }
                    let action = match decision {
                        OfferDecision::Ask => continue,
                        OfferDecision::Auto => self.clipboard.accept(device_id, offer_id),
                        OfferDecision::Reject { code, message } => self
                            .clipboard
                            .reject(device_id, offer_id)
                            .map(|(ack, event)| {
                                let ack = crate::clipboard::ClipboardAckDraft {
                                    code: code.into(),
                                    message: message.into(),
                                    ..ack
                                };
                                (ack, event)
                            }),
                    };
                    let (ack, event) = match action {
                        Ok(value) => value,
                        Err(error) => {
                            reason = format!("clipboard policy action failed: {}", error.message);
                            break;
                        }
                    };
                    self.emit_clipboard(event);
                    if let Err(error) = session
                        .sender()
                        .send_clipboard_ack(
                            ack.offer_id,
                            ack.accepted,
                            ack.applied,
                            ack.code,
                            ack.message,
                        )
                        .await
                    {
                        reason = format!("clipboard policy acknowledgement failed: {error}");
                        break;
                    }
                }
                SessionEvent::ClipboardText {
                    offer_id,
                    text,
                    digest,
                } => {
                    let (events, ack) =
                        self.clipboard.apply_text(device_id, offer_id, text, digest);
                    let applied = ack.applied;
                    for event in events {
                        self.emit_clipboard(event);
                    }
                    if let Err(error) = session
                        .sender()
                        .send_clipboard_ack(
                            ack.offer_id,
                            ack.accepted,
                            ack.applied,
                            ack.code,
                            ack.message,
                        )
                        .await
                    {
                        reason = format!("clipboard acknowledgement failed: {error}");
                        break;
                    }
                    if applied {
                        self.notifier.notify_clipboard_applied(&peer_name);
                    }
                }
                SessionEvent::ClipboardAck {
                    offer_id,
                    accepted,
                    applied,
                    code,
                    message,
                } => {
                    let (events, follow_up) = self
                        .clipboard
                        .receive_ack(device_id, offer_id, accepted, applied, code, message);
                    for event in events {
                        self.emit_clipboard(event);
                    }
                    match follow_up {
                        Some(ClipboardFollowUp::SendText {
                            offer_id,
                            text,
                            digest,
                        }) => {
                            if let Err(error) = session
                                .sender()
                                .send_clipboard_text(offer_id, text, digest)
                                .await
                            {
                                let event = self.clipboard.mark_failed(
                                    offer_id,
                                    session_error_code(&error),
                                    &error.to_string(),
                                );
                                self.emit_clipboard(event);
                                reason = format!("clipboard text send failed: {error}");
                                break;
                            }
                        }
                        Some(ClipboardFollowUp::SendAck(ack)) => {
                            let _ = session
                                .sender()
                                .send_clipboard_ack(
                                    ack.offer_id,
                                    ack.accepted,
                                    ack.applied,
                                    ack.code,
                                    ack.message,
                                )
                                .await;
                        }
                        None => {}
                    }
                }
                event @ (SessionEvent::TransferOffer { .. }
                | SessionEvent::TransferAccept { .. }
                | SessionEvent::TransferReject { .. }
                | SessionEvent::TransferProgress { .. }
                | SessionEvent::TransferComplete { .. }
                | SessionEvent::TransferDuplicate { .. }
                | SessionEvent::TransferError { .. }
                | SessionEvent::TransferPause { .. }
                | SessionEvent::TransferCancel { .. }
                | SessionEvent::TransferResume { .. }) => {
                    if let Some(transfer) = self.transfer()
                        && let Some(session) = self.transfer_session(device_id)
                    {
                        transfer.handle_peer_event(device_id, &peer_name, session, event);
                    }
                }
                SessionEvent::Disconnected { reason: detail } => {
                    reason = detail;
                    break;
                }
                SessionEvent::Reconnected { .. } => {}
            }
        }
        command_task.abort();
        session.close().await;
        self.end_session(device_id, generation, &reason);
    }

    async fn run_commands(
        self: Arc<Self>,
        sender: ControlSender,
        mut receiver: mpsc::Receiver<ChatCommand>,
    ) {
        while let Some(command) = receiver.recv().await {
            match command {
                ChatCommand::Close => {
                    let _ = sender.close().await;
                    break;
                }
                ChatCommand::Send { message_id, text } => {
                    if let Err(error) = sender.send_text_with_id(message_id, text).await {
                        let retryable = error.is_retryable();
                        self.mark_failed(
                            message_id,
                            session_error_code(&error),
                            &error.to_string(),
                            retryable,
                        );
                        // A failed write leaves the control stream in an
                        // unknown state; close it so the event loop reports a
                        // disconnect and pending messages fail consistently.
                        let _ = sender.close().await;
                        break;
                    }
                }
                ChatCommand::ClipboardOffer {
                    offer_id,
                    size_bytes,
                    digest,
                    preview,
                    ttl_ms,
                    origin_id,
                    hop,
                } => {
                    let offer = ClipboardShareOffer {
                        offer_id,
                        size_bytes,
                        digest,
                        preview,
                        ttl_ms,
                        origin_id,
                        hop,
                    };
                    // The command loop had no logging at all: a command that was
                    // never queued and one that was queued but whose send failed
                    // looked identical from the outside.
                    tracing::info!(%offer_id, bytes = size_bytes, "clipboard: sending an offer");
                    if let Err(error) = sender.send_clipboard_offer(offer).await {
                        tracing::warn!(%offer_id, message = %error, "clipboard: offer send failed");
                        let event = self.clipboard.mark_failed(
                            offer_id,
                            session_error_code(&error),
                            &error.to_string(),
                        );
                        self.emit_clipboard(event);
                        let _ = sender.close().await;
                        break;
                    }
                    tracing::info!(%offer_id, "clipboard: offer sent");
                }
                ChatCommand::ClipboardAck {
                    offer_id,
                    accepted,
                    applied,
                    code,
                    message,
                } => {
                    if let Err(error) = sender
                        .send_clipboard_ack(offer_id, accepted, applied, code, message)
                        .await
                    {
                        let event = self.clipboard.mark_failed(
                            offer_id,
                            session_error_code(&error),
                            &error.to_string(),
                        );
                        self.emit_clipboard(event);
                        let _ = sender.close().await;
                        break;
                    }
                }
            }
        }
    }

    fn mark_connected(
        &self,
        device_id: Uuid,
        generation: u64,
        session_id: &str,
        session: &ControlSession,
    ) -> bool {
        let connected = self
            .sessions
            .lock()
            .map(|mut sessions| {
                let Some(entry) = sessions.get_mut(&device_id) else {
                    return false;
                };
                if entry.generation != generation {
                    return false;
                }
                entry.connected = true;
                if let Ok(mut slot) = entry.control.lock() {
                    *slot = Some(session.sender());
                }
                if let Ok(mut slot) = entry.connection.lock() {
                    *slot = Some(session.connection());
                }
                if let Ok(mut slot) = entry.session_id.lock() {
                    *slot = Some(session.session_id());
                }
                true
            })
            .unwrap_or(false);
        if connected {
            self.emit(ChatEvent::Connected {
                device_id: device_id.to_string(),
                session_id: session_id.to_string(),
                connected_at_ms: now_ms(),
            });
        }
        connected
    }

    /// Connect the transfer runtime after both have been constructed.
    pub fn attach_transfer(&self, transfer: Arc<TransferRuntime>) {
        if let Ok(mut slot) = self.transfer.lock() {
            *slot = Some(transfer);
        }
    }

    fn transfer(&self) -> Option<Arc<TransferRuntime>> {
        self.transfer.lock().ok().and_then(|slot| slot.clone())
    }

    /// The control-stream handles for an established session, if any.
    pub fn transfer_session(&self, device_id: Uuid) -> Option<TransferSession> {
        let sessions = self.sessions.lock().ok()?;
        let entry = sessions.get(&device_id)?;
        if !entry.connected {
            return None;
        }
        let sender = entry.control.lock().ok()?.clone()?;
        let connection = entry.connection.lock().ok()?.clone()?;
        let session_id = entry.session_id.lock().ok().and_then(|slot| *slot)?;
        Some(TransferSession {
            sender,
            connection,
            session_id,
        })
    }

    fn end_session(&self, device_id: Uuid, generation: u64, reason: &str) {
        tracing::info!(%device_id, %reason, "chat session ended");
        let removed = self
            .sessions
            .lock()
            .map(|mut sessions| {
                if sessions
                    .get(&device_id)
                    .is_some_and(|entry| entry.generation == generation)
                {
                    sessions.remove(&device_id);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if !removed {
            return;
        }
        self.fail_pending(
            device_id,
            "connection_lost",
            "the message was not acknowledged before the session ended",
            true,
        );
        self.emit(ChatEvent::Disconnected {
            device_id: device_id.to_string(),
            reason: reason.into(),
            at_ms: now_ms(),
        });
    }

    fn peer_display_name(&self, device_id: Uuid) -> String {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions
                    .get(&device_id)
                    .map(|entry| entry.peer.name.clone())
            })
            .unwrap_or_default()
    }

    fn record_incoming(
        &self,
        device_id: Uuid,
        message_id: Uuid,
        text: String,
        sequence: u64,
        copy_hint: bool,
    ) {
        let id = message_id.to_string();
        let message = ChatMessageView {
            message_id: id.clone(),
            device_id: device_id.to_string(),
            direction: ChatDirection::Received,
            text: text.clone(),
            state: ChatMessageState::Received,
            created_at_ms: now_ms(),
            sequence,
            copy_hint,
            error_code: None,
            error_message: None,
            retryable: false,
        };
        let fresh = self
            .messages
            .lock()
            .map(|mut messages| {
                if messages.iter().any(|existing| existing.message_id == id) {
                    return false;
                }
                messages.push_back(message);
                while messages.len() > MAX_MESSAGES {
                    messages.pop_front();
                }
                true
            })
            .unwrap_or(false);
        if fresh {
            // A text message exists so the receiver can paste it. Copy it before
            // anything else looks at the message; a failure is logged but never
            // turns a delivered message into an error.
            match self.clipboard.write_local(&text) {
                Ok(()) => {
                    tracing::info!(bytes = text.len(), "received text copied to the clipboard")
                }
                Err(error) => tracing::warn!(
                    code = %error.code,
                    message = %error.message,
                    "the received text could not be copied to the clipboard"
                ),
            }
            self.notifier
                .notify_chat(&device_id.to_string(), &self.peer_display_name(device_id));
            self.emit(ChatEvent::Received {
                message_id: id,
                device_id: device_id.to_string(),
                text,
                copy_hint,
                received_at_ms: now_ms(),
            });
        }
    }

    fn record_ack(&self, device_id: Uuid, message_id: Uuid, delivered: bool) {
        if delivered {
            let changed = self
                .messages
                .lock()
                .map(|mut messages| {
                    if let Some(message) = messages
                        .iter_mut()
                        .find(|message| message.message_id == message_id.to_string())
                        && matches!(message.state, ChatMessageState::Sending)
                    {
                        message.state = ChatMessageState::Delivered;
                        message.error_code = None;
                        message.error_message = None;
                        message.retryable = false;
                        return true;
                    }
                    false
                })
                .unwrap_or(false);
            if changed {
                self.emit(ChatEvent::Delivered {
                    message_id: message_id.to_string(),
                    device_id: device_id.to_string(),
                    delivered_at_ms: now_ms(),
                });
            }
        } else {
            self.mark_failed(
                message_id,
                "not_delivered",
                "the peer did not accept the message",
                true,
            );
        }
    }

    fn mark_failed(&self, message_id: Uuid, code: &str, message: &str, retryable: bool) -> bool {
        let changed = self.update_state(
            message_id,
            ChatMessageState::Failed,
            Some(code.into()),
            Some(message.into()),
            retryable,
        );
        if changed {
            self.emit(ChatEvent::Failed {
                message_id: message_id.to_string(),
                device_id: self
                    .message(message_id)
                    .map(|message| message.device_id)
                    .unwrap_or_default(),
                code: code.into(),
                message: message.into(),
                retryable,
            });
        }
        changed
    }

    fn fail_pending(&self, device_id: Uuid, code: &str, message: &str, retryable: bool) {
        let failed = self
            .messages
            .lock()
            .map(|mut messages| {
                let mut failed = Vec::new();
                for item in messages.iter_mut() {
                    if item.device_id == device_id.to_string()
                        && matches!(item.state, ChatMessageState::Sending)
                    {
                        item.state = ChatMessageState::Failed;
                        item.error_code = Some(code.into());
                        item.error_message = Some(message.into());
                        item.retryable = retryable;
                        failed.push(item.message_id.clone());
                    }
                }
                failed
            })
            .unwrap_or_default();
        for message_id in failed {
            self.emit(ChatEvent::Failed {
                message_id,
                device_id: device_id.to_string(),
                code: code.into(),
                message: message.into(),
                retryable,
            });
        }
    }

    fn update_state(
        &self,
        message_id: Uuid,
        state: ChatMessageState,
        code: Option<String>,
        message: Option<String>,
        retryable: bool,
    ) -> bool {
        self.messages
            .lock()
            .map(|mut messages| {
                let Some(item) = messages
                    .iter_mut()
                    .find(|item| item.message_id == message_id.to_string())
                else {
                    return false;
                };
                item.state = state;
                item.error_code = code;
                item.error_message = message;
                item.retryable = retryable;
                true
            })
            .unwrap_or(false)
    }

    fn push_message(&self, message: ChatMessageView) {
        if let Ok(mut messages) = self.messages.lock() {
            messages.push_back(message);
            while messages.len() > MAX_MESSAGES {
                messages.pop_front();
            }
        }
        self.hub.bump_snapshot();
    }

    fn message(&self, message_id: Uuid) -> Option<ChatMessageView> {
        self.messages.lock().ok().and_then(|messages| {
            messages
                .iter()
                .find(|message| message.message_id == message_id.to_string())
                .cloned()
        })
    }

    fn clipboard_policy(&self, device_id: Uuid) -> ClipboardReceivePolicy {
        self.engine
            .lock()
            .ok()
            .and_then(|engine| engine.clipboard_policy(&device_id))
            .map(ClipboardReceivePolicy::from)
            .unwrap_or(ClipboardReceivePolicy::Reject)
    }

    fn check_trusted(&self, device_id: Uuid) -> Result<(), IpcError> {
        let trusted = self
            .engine
            .lock()
            .map_err(|_| IpcError::internal("engine state poisoned"))?
            .trusted_devices();
        if trusted.iter().any(|entry| entry.device_id == device_id) {
            Ok(())
        } else {
            Err(chat_error(
                "not_trusted",
                4202,
                "device is not trusted for chat",
                false,
            ))
        }
    }

    fn identity_and_trust(&self) -> Result<(Identity, fluxy_net::TlsIdentity, TrustStore), String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine state poisoned".to_string())?;
        Ok((
            engine.identity().clone(),
            engine.tls().clone(),
            engine.trust_snapshot(),
        ))
    }

    fn identity(&self) -> Result<Identity, String> {
        self.engine
            .lock()
            .map(|engine| engine.identity().clone())
            .map_err(|_| "engine state poisoned".to_string())
    }

    fn emit(&self, event: ChatEvent) {
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Chat { data: event });
    }

    fn emit_clipboard(&self, event: ClipboardEvent) {
        self.hub.bump_snapshot();
        self.hub.emit(AppEvent::Clipboard { data: event });
    }
}

fn chat_error(code: &str, code_number: u16, message: &str, retryable: bool) -> IpcError {
    IpcError {
        code: code.into(),
        code_number,
        message: message.into(),
        retryable,
    }
}

fn session_error_code(error: &SessionError) -> &'static str {
    match error {
        SessionError::TextTooLarge(_) => "message_too_large",
        SessionError::Closed => "session_closed",
        SessionError::Timeout => "send_timeout",
        SessionError::Transport(_) => "transport",
        SessionError::Handshake(_) => "handshake",
        _ => "send_failed",
    }
}

fn preview(text: &str) -> String {
    text.chars().take(MODEL_PREVIEW_CHARS).collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxy_app::AppPaths;
    use std::time::Duration;

    fn runtime() -> (tempfile::TempDir, Arc<ChatRuntime>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let engine = Engine::load(paths).unwrap();
        let hub = Arc::new(EventHub::new());
        let runtime = ChatRuntime::new(Arc::new(Mutex::new(engine)), hub);
        (dir, runtime)
    }

    /// A clipboard backend that answers with whatever the test stored.
    struct FakeClipboard {
        text: Mutex<String>,
    }

    impl FakeClipboard {
        fn new(text: &str) -> Arc<Self> {
            Arc::new(Self {
                text: Mutex::new(text.to_string()),
            })
        }

        fn set(&self, text: &str) {
            *self.text.lock().unwrap() = text.to_string();
        }
    }

    impl TextClipboard for FakeClipboard {
        fn read_text(&self) -> Result<String, String> {
            Ok(self.text.lock().unwrap().clone())
        }

        fn write_text(&self, text: &str) -> Result<(), String> {
            self.set(text);
            Ok(())
        }
    }

    fn watching_runtime(text: &str) -> (tempfile::TempDir, Arc<ChatRuntime>, Arc<FakeClipboard>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        paths.ensure().unwrap();
        let engine = Engine::load(paths).unwrap();
        let hub = Arc::new(EventHub::new());
        let backend = FakeClipboard::new(text);
        let runtime = ChatRuntime::with_clipboard(
            Arc::new(Mutex::new(engine)),
            hub,
            Arc::new(Notifier::default()),
            backend.clone(),
        );
        (dir, runtime, backend)
    }

    /// T-10: the watcher must never offer back content that arrived from a
    /// peer, however long it stays on the clipboard.
    #[test]
    fn the_watcher_never_sends_received_content_back() {
        let (_dir, runtime, backend) = watching_runtime("");
        let device = Uuid::now_v7();
        let offer_id = Uuid::now_v7();
        let text = "text from the peer";
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        runtime.clipboard.receive_offer(
            device,
            offer_id,
            text.len() as u64,
            digest,
            "text from the p".into(),
            120_000,
        );
        runtime.clipboard.accept(device, offer_id).unwrap();
        let (events, ack) =
            runtime
                .clipboard
                .apply_text(device, offer_id, text.to_string(), digest);
        assert!(ack.applied);
        assert!(!events.is_empty());
        assert_eq!(backend.read_text().unwrap(), text);

        // The value is now on the local clipboard, so a watching device would
        // see it - but it was received, so it must stay quiet.
        let mut watcher = ClipboardWatcher::new();
        let start = Instant::now();
        assert!(runtime.clipboard_watch_tick(&mut watcher, start).is_none());
        assert!(
            runtime
                .clipboard_watch_tick(&mut watcher, start + Duration::from_secs(5))
                .is_none()
        );

        // Content the user copied locally is still sent after the debounce.
        backend.set("typed on this device");
        assert!(runtime.clipboard_watch_tick(&mut watcher, start).is_none());
        assert_eq!(
            runtime
                .clipboard_watch_tick(&mut watcher, start + Duration::from_secs(1))
                .as_deref(),
            Some("typed on this device")
        );
    }

    fn entry(device_id: Uuid) -> TrustEntry {
        TrustEntry {
            device_id,
            name: "Peer".into(),
            platform: "test".into(),
            public_key: [1; 32],
            cert_fingerprint: [2; 32],
            paired_at_ms: 1,
            clipboard_policy: fluxy_core::ClipboardPolicy::Ask,
            auto_accept: fluxy_core::AutoAcceptPolicy::Inherit,
        }
    }

    fn outgoing(device_id: Uuid) -> ChatMessageView {
        ChatMessageView {
            message_id: Uuid::now_v7().to_string(),
            device_id: device_id.to_string(),
            direction: ChatDirection::Sent,
            text: "hello".into(),
            state: ChatMessageState::Sending,
            created_at_ms: now_ms(),
            sequence: 0,
            copy_hint: false,
            error_code: None,
            error_message: None,
            retryable: false,
        }
    }

    #[test]
    fn enabling_the_watcher_records_but_does_not_send_the_current_clipboard() {
        let (_dir, runtime, _backend) = watching_runtime("a password copied earlier");
        let mut watcher = ClipboardWatcher::new();
        let start = Instant::now();
        // The shell primes the watcher the first time it sees the switch on.
        runtime.prime_clipboard_watch(&mut watcher);
        assert!(runtime.clipboard_watch_tick(&mut watcher, start).is_none());
        assert!(
            runtime
                .clipboard_watch_tick(&mut watcher, start + Duration::from_secs(30))
                .is_none()
        );
    }

    #[test]
    fn incoming_message_is_deduplicated_and_visible_in_snapshot() {
        let (_dir, runtime) = runtime();
        let peer = Uuid::now_v7();
        let message_id = Uuid::now_v7();
        runtime.record_incoming(peer, message_id, "first".into(), 4, false);
        runtime.record_incoming(peer, message_id, "duplicate".into(), 5, true);

        let snapshot = runtime.snapshot(&[entry(peer)], &[]);
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.messages[0].text, "first");
        assert_eq!(snapshot.messages[0].state, ChatMessageState::Received);
        assert_eq!(
            snapshot.conversations[0].last_message.as_deref(),
            Some("first")
        );
    }

    #[test]
    fn ack_marks_only_sending_messages_delivered() {
        let (_dir, runtime) = runtime();
        let peer = Uuid::now_v7();
        let message = outgoing(peer);
        let message_id = Uuid::parse_str(&message.message_id).unwrap();
        runtime.push_message(message);
        runtime.record_ack(peer, message_id, true);

        let snapshot = runtime.snapshot(&[entry(peer)], &[]);
        assert_eq!(snapshot.messages[0].state, ChatMessageState::Delivered);
        assert!(!snapshot.messages[0].retryable);
    }

    #[test]
    fn pending_messages_fail_when_the_session_ends() {
        let (_dir, runtime) = runtime();
        let peer = Uuid::now_v7();
        let message = outgoing(peer);
        runtime.push_message(message);
        runtime.fail_pending(peer, "connection_lost", "lost", true);

        let snapshot = runtime.snapshot(&[entry(peer)], &[]);
        assert_eq!(snapshot.messages[0].state, ChatMessageState::Failed);
        assert_eq!(
            snapshot.messages[0].error_code.as_deref(),
            Some("connection_lost")
        );
        assert!(snapshot.messages[0].retryable);
    }

    #[test]
    fn reset_clears_the_memory_only_transcript() {
        let (_dir, runtime) = runtime();
        let peer = Uuid::now_v7();
        runtime.push_message(outgoing(peer));
        runtime.reset();
        let snapshot = runtime.snapshot(&[entry(peer)], &[]);
        assert!(snapshot.messages.is_empty());
    }

    #[test]
    fn transcript_is_bounded() {
        let (_dir, runtime) = runtime();
        let peer = Uuid::now_v7();
        for _ in 0..(MAX_MESSAGES + 3) {
            runtime.push_message(outgoing(peer));
        }
        let snapshot = runtime.snapshot(&[entry(peer)], &[]);
        assert_eq!(snapshot.messages.len(), MAX_MESSAGES);
    }

    #[test]
    fn clipboard_policy_decisions_are_explicit() {
        assert_eq!(
            decide_offer(ClipboardReceivePolicy::Ask, false, false),
            OfferDecision::Ask
        );
        assert_eq!(
            decide_offer(ClipboardReceivePolicy::Auto, false, false),
            OfferDecision::Auto
        );
        assert!(matches!(
            decide_offer(ClipboardReceivePolicy::Reject, false, false),
            OfferDecision::Reject {
                code: "policy_reject",
                ..
            }
        ));
        assert!(matches!(
            decide_offer(ClipboardReceivePolicy::Auto, true, false),
            OfferDecision::Reject {
                code: "echo_suppressed",
                ..
            }
        ));
        assert!(matches!(
            decide_offer(ClipboardReceivePolicy::Auto, false, true),
            OfferDecision::Reject {
                code: "duplicate_content",
                ..
            }
        ));
    }
}
