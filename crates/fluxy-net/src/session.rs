//! Multiplexed control session: chat, application ACKs, heartbeat, and
//! idle detection over one bidirectional QUIC stream.

use std::collections::{HashSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use getrandom::SysRng;
use quinn::{Connection, Endpoint, RecvStream, SendStream};
use rand_core::{Rng, UnwrapErr};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval_at, timeout};
use uuid::Uuid;

use fluxy_core::{Identity, TrustStore};
use fluxy_proto::{
    BatchEntry as WireBatchEntry, ChatText, ClipboardAck, ClipboardOffer, ClipboardText, Envelope,
    Error as WireError, MessageAck, MessageKind, ProtoError, TransferAccept, TransferCancel,
    TransferComplete, TransferDuplicate, TransferError, TransferOffer, TransferPause,
    TransferProgress, TransferReject, TransferResume, decode_payload, encode_payload,
};

use crate::handshake::{self, HandshakeError, PairingLimiter, PeerSession, make_envelope};
use crate::transport::{TransportError, client_endpoint, connect, read_envelope, write_envelope};

/// Default heartbeat interval from the design contract.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// Default half-open detection threshold.
// Two missed heartbeats, not one: a phone that pauses its timers for a moment
// (screen off, WebView throttled) should not lose the session over a single
// late ping.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum UTF-8 bytes in one ChatText message.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
/// Maximum UTF-8 bytes in one clipboard text message.
pub const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;
/// An offer may cross at most one forwarder; receivers never relay by default.
pub const MAX_CLIPBOARD_HOP: u32 = 1;
/// Scheduling priority for the session control stream. Bulk chunk data uses its
/// own uni streams at a lower priority, so chat and clipboard frames are not
/// stuck behind a transfer (acceptance T-06).
pub const CONTROL_STREAM_PRIORITY: i32 = 100;

/// Everything needed to send one clipboard offer. Keeping this in one struct
/// avoids an unwieldy argument list and makes the wire fields explicit.
#[derive(Debug, Clone)]
pub struct ClipboardShareOffer {
    pub offer_id: Uuid,
    pub size_bytes: u64,
    pub digest: [u8; 32],
    pub preview: String,
    pub ttl_ms: u64,
    pub origin_id: Uuid,
    pub hop: u32,
}

/// Everything needed to offer one file transfer. The manifest travels as
/// encoded bytes so the receiver can validate every block before accepting.
#[derive(Debug, Clone)]
pub struct TransferOfferDraft {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub block_size: u32,
    pub root_hash: [u8; 32],
    pub manifest_digest: [u8; 32],
    pub total_files: u32,
    pub total_bytes: u64,
    pub manifest: Vec<u8>,
    /// Set for a sub-transfer of a folder batch.
    pub batch_id: Option<Uuid>,
    /// Display name of the batch root, sent with the first offer only.
    pub root_name: Option<String>,
    /// This file's path inside the batch root, always '/'-separated.
    pub relative_path: Option<String>,
    /// The batch entry list; only the first offer of a batch carries it.
    pub entries: Vec<BatchEntryWire>,
}

/// One entry of a folder batch, as it travels on the control stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEntryWire {
    pub relative_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub is_dir: bool,
}

impl TransferOfferDraft {
    /// A single-file offer, which is what every pre-batch sender produced.
    pub fn file(
        transfer_id: Uuid,
        file_name: String,
        file_size: u64,
        block_size: u32,
        root_hash: [u8; 32],
        manifest_digest: [u8; 32],
        manifest: Vec<u8>,
    ) -> Self {
        Self {
            transfer_id,
            file_name,
            file_size,
            block_size,
            root_hash,
            manifest_digest,
            total_files: 1,
            total_bytes: file_size,
            manifest,
            batch_id: None,
            root_name: None,
            relative_path: None,
            entries: Vec::new(),
        }
    }
}

/// One transfer control frame. Keeping every variant in one enum lets the
/// actor loop encode them in a single place.
#[derive(Debug, Clone)]
pub enum TransferMessage {
    Offer(Box<TransferOfferDraft>),
    Accept {
        transfer_id: Uuid,
    },
    Reject {
        transfer_id: Uuid,
        code: String,
        message: String,
    },
    Progress {
        transfer_id: Uuid,
        bytes_done: u64,
        bytes_total: u64,
    },
    /// The receiver already holds this content, keyed by the offer root hash.
    Duplicate {
        transfer_id: Uuid,
    },
    Complete {
        transfer_id: Uuid,
    },
    Error {
        transfer_id: Uuid,
        code: String,
        message: String,
        retryable: bool,
    },
    Pause {
        transfer_id: Uuid,
    },
    Cancel {
        transfer_id: Uuid,
    },
    Resume {
        transfer_id: Uuid,
    },
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Protocol(#[from] ProtoError),
    #[error(transparent)]
    Handshake(#[from] HandshakeError),
    #[error("session is closed")]
    Closed,
    #[error("text is larger than {0} bytes")]
    TextTooLarge(usize),
    #[error("clipboard text is larger than {0} bytes")]
    ClipboardTooLarge(usize),
    #[error("clipboard hop {0} exceeds the maximum")]
    ClipboardHop(u32),
    #[error("peer is not trusted: {0}")]
    Untrusted(String),
    #[error("session timed out")]
    Timeout,
}

impl SessionError {
    pub fn is_retryable(&self) -> bool {
        match self {
            SessionError::Transport(_) | SessionError::Timeout => true,
            SessionError::Handshake(error) => matches!(
                error,
                HandshakeError::Transport(_)
                    | HandshakeError::Timeout
                    | HandshakeError::RateLimited { .. }
            ),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum SessionEvent {
    Message {
        message_id: Uuid,
        text: String,
        sequence: u64,
        copy_hint: bool,
    },
    Ack {
        message_id: Uuid,
        delivered: bool,
    },
    ClipboardOffer {
        offer_id: Uuid,
        size_bytes: u64,
        digest: [u8; 32],
        preview: String,
        sequence: u64,
        ttl_ms: u64,
        origin_id: Option<Uuid>,
        hop: u32,
    },
    ClipboardText {
        offer_id: Uuid,
        text: String,
        digest: [u8; 32],
    },
    ClipboardAck {
        offer_id: Uuid,
        accepted: bool,
        applied: bool,
        code: String,
        message: String,
    },
    TransferOffer {
        transfer_id: Uuid,
        file_name: String,
        file_size: u64,
        block_size: u32,
        root_hash: [u8; 32],
        manifest_digest: [u8; 32],
        total_files: u32,
        total_bytes: u64,
        manifest: Vec<u8>,
        batch_id: Option<Uuid>,
        root_name: Option<String>,
        relative_path: Option<String>,
        entries: Vec<BatchEntryWire>,
    },
    TransferAccept {
        transfer_id: Uuid,
    },
    TransferReject {
        transfer_id: Uuid,
        code: String,
        message: String,
    },
    TransferProgress {
        transfer_id: Uuid,
        bytes_done: u64,
        bytes_total: u64,
    },
    TransferComplete {
        transfer_id: Uuid,
    },
    /// The peer already holds this content, so nothing was transferred.
    TransferDuplicate {
        transfer_id: Uuid,
    },
    TransferError {
        transfer_id: Uuid,
        code: String,
        message: String,
        retryable: bool,
    },
    TransferPause {
        transfer_id: Uuid,
    },
    TransferCancel {
        transfer_id: Uuid,
    },
    TransferResume {
        transfer_id: Uuid,
    },
    Disconnected {
        reason: String,
    },
    Reconnected {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub heartbeat_interval: Duration,
    pub idle_timeout: Duration,
    pub max_text_bytes: usize,
    pub max_clipboard_bytes: usize,
    pub seen_limit: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: HEARTBEAT_INTERVAL,
            idle_timeout: IDLE_TIMEOUT,
            max_text_bytes: MAX_TEXT_BYTES,
            max_clipboard_bytes: MAX_CLIPBOARD_BYTES,
            seen_limit: 1024,
        }
    }
}

enum Command {
    SendText {
        message_id: Uuid,
        text: String,
        reply: oneshot::Sender<Result<Uuid, SessionError>>,
    },
    SendClipboardOffer {
        offer: ClipboardShareOffer,
        reply: oneshot::Sender<Result<Uuid, SessionError>>,
    },
    SendClipboardText {
        offer_id: Uuid,
        text: String,
        digest: [u8; 32],
        reply: oneshot::Sender<Result<Uuid, SessionError>>,
    },
    SendClipboardAck {
        offer_id: Uuid,
        accepted: bool,
        applied: bool,
        code: String,
        message: String,
        reply: oneshot::Sender<Result<Uuid, SessionError>>,
    },
    SendTransfer {
        message: TransferMessage,
        reply: oneshot::Sender<Result<Uuid, SessionError>>,
    },
    Close,
}

pub struct ControlSession {
    peer: PeerSession,
    connection: Connection,
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<SessionEvent>,
    task: JoinHandle<()>,
    _endpoint: Option<Endpoint>,
}

/// A cloneable write handle for an open control stream. It lets a caller send
/// while another task owns `ControlSession::next_event`.
#[derive(Clone)]
pub struct ControlSender {
    commands: mpsc::Sender<Command>,
}

impl ControlSender {
    pub async fn send_text(&self, text: impl Into<String>) -> Result<Uuid, SessionError> {
        self.send_text_with_id(Uuid::now_v7(), text).await
    }

    pub async fn close(&self) -> Result<(), SessionError> {
        self.commands
            .send(Command::Close)
            .await
            .map_err(|_| SessionError::Closed)
    }

    /// Send one transfer control frame and return its message id.
    pub async fn send_transfer(&self, message: TransferMessage) -> Result<Uuid, SessionError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(Command::SendTransfer { message, reply })
            .await
            .map_err(|_| SessionError::Closed)?;
        receiver.await.map_err(|_| SessionError::Closed)?
    }

    pub async fn send_text_with_id(
        &self,
        message_id: Uuid,
        text: impl Into<String>,
    ) -> Result<Uuid, SessionError> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::SendText {
                message_id,
                text: text.into(),
                reply,
            })
            .await
            .map_err(|_| SessionError::Closed)?;
        timeout(Duration::from_secs(5), result)
            .await
            .map_err(|_| SessionError::Timeout)?
            .map_err(|_| SessionError::Closed)?
    }

    pub async fn send_clipboard_offer(
        &self,
        offer: ClipboardShareOffer,
    ) -> Result<Uuid, SessionError> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::SendClipboardOffer { offer, reply })
            .await
            .map_err(|_| SessionError::Closed)?;
        timeout(Duration::from_secs(5), result)
            .await
            .map_err(|_| SessionError::Timeout)?
            .map_err(|_| SessionError::Closed)?
    }

    pub async fn send_clipboard_text(
        &self,
        offer_id: Uuid,
        text: impl Into<String>,
        digest: [u8; 32],
    ) -> Result<Uuid, SessionError> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::SendClipboardText {
                offer_id,
                text: text.into(),
                digest,
                reply,
            })
            .await
            .map_err(|_| SessionError::Closed)?;
        timeout(Duration::from_secs(5), result)
            .await
            .map_err(|_| SessionError::Timeout)?
            .map_err(|_| SessionError::Closed)?
    }

    pub async fn send_clipboard_ack(
        &self,
        offer_id: Uuid,
        accepted: bool,
        applied: bool,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Uuid, SessionError> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::SendClipboardAck {
                offer_id,
                accepted,
                applied,
                code: code.into(),
                message: message.into(),
                reply,
            })
            .await
            .map_err(|_| SessionError::Closed)?;
        timeout(Duration::from_secs(5), result)
            .await
            .map_err(|_| SessionError::Timeout)?
            .map_err(|_| SessionError::Closed)?
    }
}

impl ControlSession {
    pub async fn open(
        connection: &Connection,
        identity: &Identity,
        peer: &PeerSession,
    ) -> Result<Self, SessionError> {
        Self::open_with_config(connection, identity, peer, SessionConfig::default(), None).await
    }

    pub async fn open_with_config(
        connection: &Connection,
        identity: &Identity,
        peer: &PeerSession,
        config: SessionConfig,
        endpoint: Option<Endpoint>,
    ) -> Result<Self, SessionError> {
        let (send, recv) = connection
            .open_bi()
            .await
            .map_err(|e| SessionError::Transport(TransportError::Stream(e.to_string())))?;
        Ok(Self::start(
            connection.clone(),
            identity,
            peer.clone(),
            send,
            recv,
            config,
            endpoint,
        ))
    }

    pub async fn accept(
        connection: &Connection,
        identity: &Identity,
        peer: &PeerSession,
    ) -> Result<Self, SessionError> {
        Self::accept_with_config(connection, identity, peer, SessionConfig::default(), None).await
    }

    pub async fn accept_with_config(
        connection: &Connection,
        identity: &Identity,
        peer: &PeerSession,
        config: SessionConfig,
        endpoint: Option<Endpoint>,
    ) -> Result<Self, SessionError> {
        tracing::info!(remote = %connection.remote_address(), "waiting for the peer control stream");
        let (send, recv) = match connection.accept_bi().await {
            Ok(pair) => {
                tracing::info!("peer control stream accepted");
                pair
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    reason = ?connection.close_reason(),
                    "accepting the peer control stream failed"
                );
                return Err(SessionError::Transport(TransportError::Stream(
                    error.to_string(),
                )));
            }
        };
        Ok(Self::start(
            connection.clone(),
            identity,
            peer.clone(),
            send,
            recv,
            config,
            endpoint,
        ))
    }

    pub fn peer(&self) -> &PeerSession {
        &self.peer
    }

    pub fn session_id(&self) -> Uuid {
        self.peer.session_id
    }

    pub async fn next_event(&mut self) -> Option<SessionEvent> {
        self.events.recv().await
    }

    /// The underlying QUIC connection. Transfer tasks open their own
    /// bidirectional data stream on it while this session keeps the control
    /// stream.
    pub fn connection(&self) -> Connection {
        self.connection.clone()
    }

    pub fn sender(&self) -> ControlSender {
        ControlSender {
            commands: self.commands.clone(),
        }
    }

    pub async fn send_text(&self, text: impl Into<String>) -> Result<Uuid, SessionError> {
        self.sender().send_text(text).await
    }

    /// Send with a caller-supplied id. Retrying an unacknowledged message must
    /// reuse its original id so the receiver can deduplicate it.
    pub async fn send_text_with_id(
        &self,
        message_id: Uuid,
        text: impl Into<String>,
    ) -> Result<Uuid, SessionError> {
        self.sender().send_text_with_id(message_id, text).await
    }

    pub async fn send_clipboard_offer(
        &self,
        offer: ClipboardShareOffer,
    ) -> Result<Uuid, SessionError> {
        self.sender().send_clipboard_offer(offer).await
    }

    pub async fn send_clipboard_text(
        &self,
        offer_id: Uuid,
        text: impl Into<String>,
        digest: [u8; 32],
    ) -> Result<Uuid, SessionError> {
        self.sender()
            .send_clipboard_text(offer_id, text, digest)
            .await
    }

    pub async fn send_clipboard_ack(
        &self,
        offer_id: Uuid,
        accepted: bool,
        applied: bool,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Uuid, SessionError> {
        self.sender()
            .send_clipboard_ack(offer_id, accepted, applied, code, message)
            .await
    }

    pub async fn close(self) {
        let _ = self.commands.send(Command::Close).await;
        self.task.abort();
    }

    fn start(
        connection: Connection,
        identity: &Identity,
        peer: PeerSession,
        send: SendStream,
        recv: RecvStream,
        config: SessionConfig,
        endpoint: Option<Endpoint>,
    ) -> Self {
        // Chat, clipboard and transfer negotiation all ride this stream. Bulk
        // chunk data uses its own uni streams, so scheduling control frames at
        // a higher priority keeps chat responsive while a transfer saturates
        // the link (acceptance T-06).
        let _ = send.set_priority(CONTROL_STREAM_PRIORITY);
        let (commands, command_rx) = mpsc::channel(16);
        let (events, event_rx) = mpsc::channel(64);
        let (incoming, incoming_rx) = mpsc::channel(32);

        let reader = tokio::spawn(read_loop(recv, incoming));
        let task = tokio::spawn(actor_loop(
            connection.clone(),
            identity.clone(),
            peer.clone(),
            send,
            incoming_rx,
            command_rx,
            events,
            config,
            reader,
        ));

        Self {
            peer,
            connection,
            commands,
            events: event_rx,
            task,
            _endpoint: endpoint,
        }
    }
}

impl Drop for ControlSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_loop(mut recv: RecvStream, incoming: mpsc::Sender<Envelope>) {
    while let Ok(envelope) = read_envelope(&mut recv).await {
        if incoming.send(envelope).await.is_err() {
            break;
        }
    }
}

/// Encode one transfer control frame. Payloads stay small because chunk data
/// never travels on the control stream.
fn encode_transfer(message: &TransferMessage) -> (MessageKind, Vec<u8>) {
    match message {
        TransferMessage::Offer(offer) => (
            MessageKind::TransferOffer,
            encode_payload(&TransferOffer {
                transfer_id: offer.transfer_id.as_bytes().to_vec(),
                file_name: offer.file_name.clone(),
                file_size: offer.file_size,
                block_size: offer.block_size,
                root_hash: offer.root_hash.to_vec(),
                manifest_digest: offer.manifest_digest.to_vec(),
                total_files: offer.total_files,
                total_bytes: offer.total_bytes,
                manifest: offer.manifest.clone(),
                batch_id: offer
                    .batch_id
                    .map(|id| id.as_bytes().to_vec())
                    .unwrap_or_default(),
                root_name: offer.root_name.clone().unwrap_or_default(),
                relative_path: offer.relative_path.clone().unwrap_or_default(),
                entries: offer
                    .entries
                    .iter()
                    .map(|entry| WireBatchEntry {
                        relative_path: entry.relative_path.clone(),
                        size: entry.size,
                        mtime_ms: entry.mtime_ms,
                        is_dir: entry.is_dir,
                    })
                    .collect(),
            }),
        ),
        TransferMessage::Accept { transfer_id } => (
            MessageKind::TransferAccept,
            encode_payload(&TransferAccept {
                transfer_id: transfer_id.as_bytes().to_vec(),
            }),
        ),
        TransferMessage::Reject {
            transfer_id,
            code,
            message,
        } => (
            MessageKind::TransferReject,
            encode_payload(&TransferReject {
                transfer_id: transfer_id.as_bytes().to_vec(),
                code: code.clone(),
                message: message.clone(),
            }),
        ),
        TransferMessage::Progress {
            transfer_id,
            bytes_done,
            bytes_total,
        } => (
            MessageKind::TransferProgress,
            encode_payload(&TransferProgress {
                transfer_id: transfer_id.as_bytes().to_vec(),
                bytes_done: *bytes_done,
                bytes_total: *bytes_total,
            }),
        ),
        TransferMessage::Complete { transfer_id } => (
            MessageKind::TransferComplete,
            encode_payload(&TransferComplete {
                transfer_id: transfer_id.as_bytes().to_vec(),
            }),
        ),
        TransferMessage::Duplicate { transfer_id } => (
            MessageKind::TransferDuplicate,
            encode_payload(&TransferDuplicate {
                transfer_id: transfer_id.as_bytes().to_vec(),
            }),
        ),
        TransferMessage::Error {
            transfer_id,
            code,
            message,
            retryable,
        } => (
            MessageKind::TransferError,
            encode_payload(&TransferError {
                transfer_id: transfer_id.as_bytes().to_vec(),
                code: code.clone(),
                message: message.clone(),
                retryable: *retryable,
            }),
        ),
        TransferMessage::Pause { transfer_id } => (
            MessageKind::TransferPause,
            encode_payload(&TransferPause {
                transfer_id: transfer_id.as_bytes().to_vec(),
            }),
        ),
        TransferMessage::Cancel { transfer_id } => (
            MessageKind::TransferCancel,
            encode_payload(&TransferCancel {
                transfer_id: transfer_id.as_bytes().to_vec(),
            }),
        ),
        TransferMessage::Resume { transfer_id } => (
            MessageKind::TransferResume,
            encode_payload(&TransferResume {
                transfer_id: transfer_id.as_bytes().to_vec(),
            }),
        ),
    }
}

fn uuid_from(bytes: &[u8]) -> Option<Uuid> {
    Uuid::from_slice(bytes).ok()
}

fn fixed_32(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.try_into().ok()
}

/// Parse one transfer frame. A malformed frame is ignored rather than treated
/// as a session failure, matching the compatibility rule for unknown kinds.
async fn handle_transfer_envelope(
    kind: MessageKind,
    envelope: &Envelope,
    events: &mpsc::Sender<SessionEvent>,
) -> bool {
    let event = match kind {
        MessageKind::TransferOffer => {
            let Ok(offer) = decode_payload::<TransferOffer>(&envelope.payload) else {
                return true;
            };
            let (Some(transfer_id), Some(root_hash), Some(manifest_digest)) = (
                uuid_from(&offer.transfer_id),
                fixed_32(&offer.root_hash),
                fixed_32(&offer.manifest_digest),
            ) else {
                return true;
            };
            SessionEvent::TransferOffer {
                transfer_id,
                file_name: offer.file_name,
                file_size: offer.file_size,
                block_size: offer.block_size,
                root_hash,
                manifest_digest,
                total_files: offer.total_files,
                total_bytes: offer.total_bytes,
                manifest: offer.manifest,
                batch_id: uuid_from(&offer.batch_id),
                root_name: (!offer.root_name.is_empty()).then_some(offer.root_name),
                relative_path: (!offer.relative_path.is_empty()).then_some(offer.relative_path),
                entries: offer
                    .entries
                    .into_iter()
                    .map(|entry| BatchEntryWire {
                        relative_path: entry.relative_path,
                        size: entry.size,
                        mtime_ms: entry.mtime_ms,
                        is_dir: entry.is_dir,
                    })
                    .collect(),
            }
        }
        MessageKind::TransferAccept => {
            let Ok(accept) = decode_payload::<TransferAccept>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&accept.transfer_id) else {
                return true;
            };
            SessionEvent::TransferAccept { transfer_id }
        }
        MessageKind::TransferReject => {
            let Ok(reject) = decode_payload::<TransferReject>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&reject.transfer_id) else {
                return true;
            };
            SessionEvent::TransferReject {
                transfer_id,
                code: reject.code,
                message: reject.message,
            }
        }
        MessageKind::TransferProgress => {
            let Ok(progress) = decode_payload::<TransferProgress>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&progress.transfer_id) else {
                return true;
            };
            SessionEvent::TransferProgress {
                transfer_id,
                bytes_done: progress.bytes_done,
                bytes_total: progress.bytes_total,
            }
        }
        MessageKind::TransferComplete => {
            let Ok(complete) = decode_payload::<TransferComplete>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&complete.transfer_id) else {
                return true;
            };
            SessionEvent::TransferComplete { transfer_id }
        }
        MessageKind::TransferDuplicate => {
            let Ok(duplicate) = decode_payload::<TransferDuplicate>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&duplicate.transfer_id) else {
                return true;
            };
            SessionEvent::TransferDuplicate { transfer_id }
        }
        MessageKind::TransferError => {
            let Ok(error) = decode_payload::<TransferError>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&error.transfer_id) else {
                return true;
            };
            SessionEvent::TransferError {
                transfer_id,
                code: error.code,
                message: error.message,
                retryable: error.retryable,
            }
        }
        MessageKind::TransferPause => {
            let Ok(pause) = decode_payload::<TransferPause>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&pause.transfer_id) else {
                return true;
            };
            SessionEvent::TransferPause { transfer_id }
        }
        MessageKind::TransferCancel => {
            let Ok(cancel) = decode_payload::<TransferCancel>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&cancel.transfer_id) else {
                return true;
            };
            SessionEvent::TransferCancel { transfer_id }
        }
        MessageKind::TransferResume => {
            let Ok(resume) = decode_payload::<TransferResume>(&envelope.payload) else {
                return true;
            };
            let Some(transfer_id) = uuid_from(&resume.transfer_id) else {
                return true;
            };
            SessionEvent::TransferResume { transfer_id }
        }
        _ => return true,
    };
    events.send(event).await.is_ok()
}

#[allow(clippy::too_many_arguments)]
async fn actor_loop(
    connection: Connection,
    identity: Identity,
    peer: PeerSession,
    mut send: SendStream,
    mut incoming: mpsc::Receiver<Envelope>,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<SessionEvent>,
    config: SessionConfig,
    reader: JoinHandle<()>,
) {
    let mut sequence = 1u64;
    let mut seen = VecDeque::new();
    let mut seen_set = HashSet::new();
    let mut heartbeat = interval_at(
        tokio::time::Instant::now() + config.heartbeat_interval,
        config.heartbeat_interval,
    );
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let idle = tokio::time::sleep(config.idle_timeout);
    tokio::pin!(idle);

    loop {
        tokio::select! {
            maybe = incoming.recv() => {
                match maybe {
                    Some(envelope) => {
                        let now = Instant::now();
                        idle.as_mut().reset((now + config.idle_timeout).into());
                        if !handle_envelope(
                            &mut send,
                            &identity,
                            &peer,
                            envelope,
                            &mut seen,
                            &mut seen_set,
                            config.seen_limit,
                            config.max_text_bytes,
                            config.max_clipboard_bytes,
                            &events,
                        ).await {
                            disconnect(&connection, &events, "control stream closed").await;
                            break;
                        }
                    }
                    None => {
                        disconnect(&connection, &events, "control stream closed").await;
                        break;
                    }
                }
            }
            command = commands.recv() => {
                match command {
                    Some(Command::SendText { message_id, text, reply }) => {
                        if text.len() > config.max_text_bytes {
                            let _ = reply.send(Err(SessionError::TextTooLarge(config.max_text_bytes)));
                            continue;
                        }
                        let payload = ChatText {
                            text,
                            copy_hint: false,
                            sequence,
                        };
                        sequence = sequence.saturating_add(1);
                        let mut envelope = make_envelope(
                            MessageKind::ChatText,
                            &identity,
                            Some(peer.session_id),
                            encode_payload(&payload),
                        );
                        envelope.message_id = message_id.as_bytes().to_vec();
                        if let Err(error) = write_envelope(&mut send, &envelope).await {
                            let _ = reply.send(Err(SessionError::Transport(error)));
                            disconnect(&connection, &events, "write failed").await;
                            break;
                        }
                        let _ = reply.send(Ok(message_id));
                    }
                    Some(Command::SendClipboardOffer { offer, reply }) => {
                        if offer.hop > MAX_CLIPBOARD_HOP {
                            let _ = reply.send(Err(SessionError::ClipboardHop(offer.hop)));
                            continue;
                        }
                        let payload = ClipboardOffer {
                            size_bytes: offer.size_bytes,
                            digest: offer.digest.to_vec(),
                            preview: offer.preview,
                            sequence,
                            ttl_ms: offer.ttl_ms,
                            origin_id: offer.origin_id.as_bytes().to_vec(),
                            hop: offer.hop,
                        };
                        sequence = sequence.saturating_add(1);
                        let mut envelope = make_envelope(
                            MessageKind::ClipboardOffer,
                            &identity,
                            Some(peer.session_id),
                            encode_payload(&payload),
                        );
                        envelope.message_id = offer.offer_id.as_bytes().to_vec();
                        if let Err(error) = write_envelope(&mut send, &envelope).await {
                            let _ = reply.send(Err(SessionError::Transport(error)));
                            disconnect(&connection, &events, "write failed").await;
                            break;
                        }
                        let _ = reply.send(Ok(offer.offer_id));
                    }
                    Some(Command::SendClipboardText {
                        offer_id,
                        text,
                        digest,
                        reply,
                    }) => {
                        if text.len() > config.max_clipboard_bytes {
                            let _ = reply.send(Err(SessionError::ClipboardTooLarge(
                                config.max_clipboard_bytes,
                            )));
                            continue;
                        }
                        let payload = ClipboardText {
                            offer_id: offer_id.as_bytes().to_vec(),
                            text,
                            digest: digest.to_vec(),
                        };
                        let envelope = make_envelope(
                            MessageKind::ClipboardText,
                            &identity,
                            Some(peer.session_id),
                            encode_payload(&payload),
                        );
                        let message_id =
                            Uuid::from_slice(&envelope.message_id).unwrap_or(Uuid::nil());
                        if let Err(error) = write_envelope(&mut send, &envelope).await {
                            let _ = reply.send(Err(SessionError::Transport(error)));
                            disconnect(&connection, &events, "write failed").await;
                            break;
                        }
                        let _ = reply.send(Ok(message_id));
                    }
                    Some(Command::SendClipboardAck {
                        offer_id,
                        accepted,
                        applied,
                        code,
                        message,
                        reply,
                    }) => {
                        let payload = ClipboardAck {
                            offer_id: offer_id.as_bytes().to_vec(),
                            accepted,
                            applied,
                            code,
                            message,
                        };
                        let envelope = make_envelope(
                            MessageKind::ClipboardAck,
                            &identity,
                            Some(peer.session_id),
                            encode_payload(&payload),
                        );
                        let message_id =
                            Uuid::from_slice(&envelope.message_id).unwrap_or(Uuid::nil());
                        if let Err(error) = write_envelope(&mut send, &envelope).await {
                            let _ = reply.send(Err(SessionError::Transport(error)));
                            disconnect(&connection, &events, "write failed").await;
                            break;
                        }
                        let _ = reply.send(Ok(message_id));
                    }
                    Some(Command::SendTransfer { message, reply }) => {
                        let (kind, payload) = encode_transfer(&message);
                        let envelope =
                            make_envelope(kind, &identity, Some(peer.session_id), payload);
                        let message_id =
                            Uuid::from_slice(&envelope.message_id).unwrap_or(Uuid::nil());
                        if let Err(error) = write_envelope(&mut send, &envelope).await {
                            let _ = reply.send(Err(SessionError::Transport(error)));
                            disconnect(&connection, &events, "write failed").await;
                            break;
                        }
                        let _ = reply.send(Ok(message_id));
                    }
                    Some(Command::Close) | None => {
                        connection.close(0u32.into(), b"closed");
                        break;
                    }
                }
            }
            _ = &mut idle => {
                disconnect(&connection, &events, "heartbeat timeout").await;
                break;
            }
            _ = heartbeat.tick() => {
                let ping = make_envelope(
                    MessageKind::Ping,
                    &identity,
                    Some(peer.session_id),
                    Vec::new(),
                );
                if write_envelope(&mut send, &ping).await.is_err() {
                    disconnect(&connection, &events, "ping failed").await;
                    break;
                }
            }
        }
    }

    reader.abort();
    connection.close(0u32.into(), b"closed");
}

#[allow(clippy::too_many_arguments)]
async fn handle_envelope(
    send: &mut SendStream,
    identity: &Identity,
    peer: &PeerSession,
    envelope: Envelope,
    seen: &mut VecDeque<Uuid>,
    seen_set: &mut HashSet<Uuid>,
    seen_limit: usize,
    max_text_bytes: usize,
    max_clipboard_bytes: usize,
    events: &mpsc::Sender<SessionEvent>,
) -> bool {
    if envelope.session_id.as_slice() != peer.session_id.as_bytes()
        || envelope.source_device_id.as_slice() != peer.device_id.as_bytes()
    {
        // Closing the control stream over a mismatched envelope is correct, but
        // without this line the reason is invisible: the caller only logs
        // "control stream closed", which says nothing about why.
        tracing::warn!(
            envelope_session = %Uuid::from_slice(&envelope.session_id).unwrap_or(Uuid::nil()),
            expected_session = %peer.session_id,
            envelope_device = %Uuid::from_slice(&envelope.source_device_id).unwrap_or(Uuid::nil()),
            expected_device = %peer.device_id,
            kind = envelope.kind,
            "control envelope rejected: session or device mismatch"
        );
        return false;
    }
    match MessageKind::try_from(envelope.kind) {
        Ok(MessageKind::Ping) => {
            let pong = make_envelope(
                MessageKind::Pong,
                identity,
                Some(peer.session_id),
                Vec::new(),
            );
            write_envelope(send, &pong).await.is_ok()
        }
        Ok(MessageKind::Pong) => true,
        Ok(MessageKind::ChatText) => {
            let Ok(chat) = decode_payload::<ChatText>(&envelope.payload) else {
                return true;
            };
            if chat.text.len() > max_text_bytes {
                return false;
            }
            let message_id = Uuid::from_slice(&envelope.message_id).unwrap_or(Uuid::nil());
            let fresh = seen_set.insert(message_id);
            if fresh {
                seen.push_back(message_id);
                if seen.len() > seen_limit
                    && let Some(old) = seen.pop_front()
                {
                    seen_set.remove(&old);
                }
            }
            if fresh
                && events
                    .send(SessionEvent::Message {
                        message_id,
                        text: chat.text,
                        sequence: chat.sequence,
                        copy_hint: chat.copy_hint,
                    })
                    .await
                    .is_err()
            {
                return false;
            }
            let ack = MessageAck {
                message_id: envelope.message_id,
                delivered: true,
            };
            let envelope = make_envelope(
                MessageKind::MessageAck,
                identity,
                Some(peer.session_id),
                encode_payload(&ack),
            );
            write_envelope(send, &envelope).await.is_ok()
        }
        Ok(MessageKind::MessageAck) => {
            if let Ok(ack) = decode_payload::<MessageAck>(&envelope.payload) {
                let id = Uuid::from_slice(&ack.message_id).unwrap_or(Uuid::nil());
                let _ = events
                    .send(SessionEvent::Ack {
                        message_id: id,
                        delivered: ack.delivered,
                    })
                    .await;
            }
            true
        }
        Ok(MessageKind::ClipboardOffer) => {
            let Ok(offer) = decode_payload::<ClipboardOffer>(&envelope.payload) else {
                return true;
            };
            let Ok(offer_id) = Uuid::from_slice(&envelope.message_id) else {
                return true;
            };
            let Ok(digest) = <[u8; 32]>::try_from(offer.digest.as_slice()) else {
                return true;
            };
            if offer.preview.len() > 4096 || offer.ttl_ms == 0 || offer.hop > MAX_CLIPBOARD_HOP {
                return true;
            }
            let origin_id = if offer.origin_id.is_empty() {
                None
            } else {
                let Ok(origin) = Uuid::from_slice(&offer.origin_id) else {
                    return true;
                };
                Some(origin)
            };
            let _ = events
                .send(SessionEvent::ClipboardOffer {
                    offer_id,
                    size_bytes: offer.size_bytes,
                    digest,
                    preview: offer.preview,
                    sequence: offer.sequence,
                    ttl_ms: offer.ttl_ms,
                    origin_id,
                    hop: offer.hop,
                })
                .await;
            true
        }
        Ok(MessageKind::ClipboardText) => {
            let Ok(clipboard) = decode_payload::<ClipboardText>(&envelope.payload) else {
                return true;
            };
            let Ok(offer_id) = Uuid::from_slice(&clipboard.offer_id) else {
                return true;
            };
            let Ok(digest) = <[u8; 32]>::try_from(clipboard.digest.as_slice()) else {
                return true;
            };
            if clipboard.text.len() > max_clipboard_bytes {
                return false;
            }
            let _ = events
                .send(SessionEvent::ClipboardText {
                    offer_id,
                    text: clipboard.text,
                    digest,
                })
                .await;
            true
        }
        Ok(MessageKind::ClipboardAck) => {
            let Ok(ack) = decode_payload::<ClipboardAck>(&envelope.payload) else {
                return true;
            };
            let Ok(offer_id) = Uuid::from_slice(&ack.offer_id) else {
                return true;
            };
            if ack.code.len() > 128 || ack.message.len() > 1024 {
                return true;
            }
            let _ = events
                .send(SessionEvent::ClipboardAck {
                    offer_id,
                    accepted: ack.accepted,
                    applied: ack.applied,
                    code: ack.code,
                    message: ack.message,
                })
                .await;
            true
        }
        Ok(
            kind @ (MessageKind::TransferOffer
            | MessageKind::TransferAccept
            | MessageKind::TransferReject
            | MessageKind::TransferProgress
            | MessageKind::TransferComplete
            | MessageKind::TransferDuplicate
            | MessageKind::TransferError
            | MessageKind::TransferPause
            | MessageKind::TransferCancel
            | MessageKind::TransferResume),
        ) => handle_transfer_envelope(kind, &envelope, events).await,
        Ok(MessageKind::Error) => {
            if let Ok(error) = decode_payload::<WireError>(&envelope.payload) {
                let _ = events
                    .send(SessionEvent::Disconnected {
                        reason: format!("peer error {}: {}", error.code, error.message),
                    })
                    .await;
            }
            false
        }
        _ => true,
    }
}

async fn disconnect(connection: &Connection, events: &mpsc::Sender<SessionEvent>, reason: &str) {
    let _ = events
        .send(SessionEvent::Disconnected {
            reason: reason.to_string(),
        })
        .await;
    connection.close(0u32.into(), b"disconnected");
}

/// Reconnect policy matching the design: base delays of 1s, 2s, 4s, 8s,
/// 15s, then 30s, with up to 20% jitter and a hard 30s cap.
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: usize,
    jitter: bool,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            attempt: 0,
            jitter: true,
        }
    }

    /// Deterministic variant used by tests and scripts that need exact delays.
    pub fn deterministic() -> Self {
        Self {
            attempt: 0,
            jitter: false,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let seconds = match self.attempt {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            4 => 15,
            _ => 30,
        };
        self.attempt = self.attempt.saturating_add(1);
        if !self.jitter {
            return Duration::from_secs(seconds);
        }

        let base_ms = seconds * 1000;
        let span = (base_ms / 5) as u32;
        let mut rng = UnwrapErr(SysRng);
        let offset = i64::from(rng.next_u32() % (span * 2 + 1)) - i64::from(span);
        let millis = (base_ms as i64 + offset).clamp(1, 30_000) as u64;
        Duration::from_millis(millis)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

/// Connect to an already-paired server, resume the authenticated session, and
/// open its multiplexed control stream.
pub async fn connect_trusted(
    addr: SocketAddr,
    server_device_id: Uuid,
    identity: &Identity,
    tls: &crate::tls::TlsIdentity,
    trust: &mut TrustStore,
) -> Result<ControlSession, SessionError> {
    let fingerprint = trust
        .get(&server_device_id)
        .ok_or_else(|| SessionError::Untrusted(format!("{server_device_id} is not trusted")))?
        .cert_fingerprint;
    let endpoint = client_endpoint(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        tls,
        fingerprint,
    )
    .await?;
    tracing::info!(%addr, "dialling the peer");
    let connection = connect(&endpoint, addr).await?;
    tracing::info!(%addr, "quic handshake completed");
    let peer = handshake::client_resume(&connection, identity, tls, trust).await?;
    tracing::info!(%addr, "resume handshake completed");
    let session = ControlSession::open_with_config(
        &connection,
        identity,
        &peer,
        SessionConfig::default(),
        Some(endpoint),
    )
    .await;
    match &session {
        Ok(_) => tracing::info!(%addr, "control stream open"),
        Err(error) => tracing::warn!(%addr, %error, "control stream failed"),
    }
    session
}

/// Connect with bounded exponential backoff. The trust store is re-read on
/// every attempt, so a revocation during the retry window is respected.
pub async fn connect_trusted_with_retry(
    addr: SocketAddr,
    server_device_id: Uuid,
    identity: &Identity,
    tls: &crate::tls::TlsIdentity,
    trust: &mut TrustStore,
    backoff: &mut Backoff,
    max_attempts: u32,
) -> Result<ControlSession, SessionError> {
    let mut attempts = max_attempts.max(1);
    loop {
        match connect_trusted(addr, server_device_id, identity, tls, trust).await {
            Ok(session) => {
                backoff.reset();
                return Ok(session);
            }
            Err(error) => {
                attempts -= 1;
                if attempts == 0 || !error.is_retryable() {
                    return Err(error);
                }
                tokio::time::sleep(backoff.next_delay()).await;
            }
        }
    }
}

/// A control session that transparently resumes after a disconnect. Callers
/// must keep reading next_event; a successful reconnect emits
/// SessionEvent::Reconnected before new messages arrive.
pub struct PersistentSession {
    addr: SocketAddr,
    server_id: Uuid,
    identity: Identity,
    tls: crate::tls::TlsIdentity,
    trust: TrustStore,
    backoff: Backoff,
    max_attempts: u32,
    session: Option<ControlSession>,
    seen: VecDeque<Uuid>,
    seen_set: HashSet<Uuid>,
}

impl PersistentSession {
    pub async fn connect(
        addr: SocketAddr,
        server_id: Uuid,
        identity: Identity,
        tls: crate::tls::TlsIdentity,
        trust: TrustStore,
    ) -> Result<Self, SessionError> {
        let mut persistent = Self {
            addr,
            server_id,
            identity,
            tls,
            trust,
            backoff: Backoff::new(),
            max_attempts: 8,
            session: None,
            seen: VecDeque::new(),
            seen_set: HashSet::new(),
        };
        let session = connect_trusted_with_retry(
            addr,
            server_id,
            &persistent.identity,
            &persistent.tls,
            &mut persistent.trust,
            &mut persistent.backoff,
            persistent.max_attempts,
        )
        .await?;
        persistent.session = Some(session);
        Ok(persistent)
    }

    pub fn peer(&self) -> Option<&PeerSession> {
        self.session.as_ref().map(ControlSession::peer)
    }

    pub fn trust(&self) -> &TrustStore {
        &self.trust
    }

    pub async fn send_text(&self, text: impl Into<String>) -> Result<Uuid, SessionError> {
        self.session
            .as_ref()
            .ok_or(SessionError::Closed)?
            .send_text(text)
            .await
    }

    pub async fn next_event(&mut self) -> Option<SessionEvent> {
        loop {
            let event = match self.session.as_mut() {
                Some(session) => session.next_event().await,
                None => None,
            };
            match event {
                Some(SessionEvent::Message { message_id, .. }) if !self.record_seen(message_id) => {
                    continue;
                }
                Some(SessionEvent::Disconnected { reason }) => {
                    if let Some(session) = self.session.take() {
                        session.close().await;
                    }
                    return self.reconnect(reason).await;
                }
                Some(event) => return Some(event),
                None => {
                    if let Some(session) = self.session.take() {
                        session.close().await;
                    }
                    return self.reconnect("control session ended".to_string()).await;
                }
            }
        }
    }

    fn record_seen(&mut self, message_id: Uuid) -> bool {
        if !self.seen_set.insert(message_id) {
            return false;
        }
        self.seen.push_back(message_id);
        const LIMIT: usize = 1024;
        if self.seen.len() > LIMIT
            && let Some(old) = self.seen.pop_front()
        {
            self.seen_set.remove(&old);
        }
        true
    }

    async fn reconnect(&mut self, reason: String) -> Option<SessionEvent> {
        match connect_trusted_with_retry(
            self.addr,
            self.server_id,
            &self.identity,
            &self.tls,
            &mut self.trust,
            &mut self.backoff,
            self.max_attempts,
        )
        .await
        {
            Ok(session) => {
                self.session = Some(session);
                Some(SessionEvent::Reconnected { reason })
            }
            Err(error) => Some(SessionEvent::Disconnected {
                reason: format!("reconnect failed: {error}"),
            }),
        }
    }
}

/// Server-side helper for one authenticated control session.
pub async fn accept_session<F>(
    connection: &Connection,
    identity: &Identity,
    tls: &crate::tls::TlsIdentity,
    trust: &mut TrustStore,
    limiter: &mut PairingLimiter,
    confirm: F,
) -> Result<ControlSession, SessionError>
where
    F: Fn(&str) -> bool + Send + Sync,
{
    let peer = handshake::server_pair(connection, identity, tls, trust, limiter, confirm).await?;
    ControlSession::accept(connection, identity, &peer).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    use fluxy_core::{Identity, TrustStore};

    use crate::handshake::client_pair;
    use crate::transport::{accept, client_endpoint, connect, server_endpoint};

    fn loopback() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    fn test_config() -> SessionConfig {
        SessionConfig {
            heartbeat_interval: Duration::from_millis(50),
            idle_timeout: Duration::from_millis(500),
            max_text_bytes: 32 * 1024,
            max_clipboard_bytes: MAX_CLIPBOARD_BYTES,
            seen_limit: 32,
        }
    }

    #[test]
    fn backoff_matches_design_sequence() {
        let mut backoff = Backoff::deterministic();
        let delays: Vec<u64> = (0..7).map(|_| backoff.next_delay().as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 15, 30, 30]);
        backoff.reset();
        assert_eq!(backoff.next_delay().as_secs(), 1);

        let mut jittered = Backoff::new();
        for (low, high) in [
            (800, 1200),
            (1600, 2400),
            (3200, 4800),
            (6400, 9600),
            (12_000, 18_000),
            (24_000, 30_000),
        ] {
            let millis = jittered.next_delay().as_millis() as u64;
            assert!(
                (low..=high).contains(&millis),
                "{millis} outside {low}..={high}"
            );
        }
    }

    #[tokio::test]
    async fn chat_is_acked() {
        let server_identity = Identity::generate();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let peer = crate::handshake::server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &connection,
                &server_identity,
                &peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            let event = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            control.close().await;
            event
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| true,
        )
        .await
        .unwrap();
        let mut control = ControlSession::open_with_config(
            &connection,
            &client_identity,
            &peer,
            test_config(),
            None,
        )
        .await
        .unwrap();
        let message_id = control.send_text("hello over control").await.unwrap();
        let ack = timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap();
        match ack {
            SessionEvent::Ack {
                message_id: ack_id,
                delivered,
            } => {
                assert_eq!(ack_id, message_id);
                assert!(delivered);
            }
            other => panic!("expected ACK, got {other:?}"),
        }

        match server_task.await.unwrap() {
            SessionEvent::Message { text, sequence, .. } => {
                assert_eq!(text, "hello over control");
                assert_eq!(sequence, 1);
            }
            other => panic!("expected message, got {other:?}"),
        }
        control.close().await;
        connection.close(0u32.into(), b"done");
        endpoint.wait_idle().await;
    }

    #[tokio::test]
    async fn a_transfer_duplicate_reaches_the_event_stream() {
        let server_identity = Identity::generate();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let peer = crate::handshake::server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &connection,
                &server_identity,
                &peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            let event = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            control.close().await;
            event
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| true,
        )
        .await
        .unwrap();
        let control = ControlSession::open_with_config(
            &connection,
            &client_identity,
            &peer,
            test_config(),
            None,
        )
        .await
        .unwrap();
        let transfer_id = Uuid::now_v7();
        control
            .sender()
            .send_transfer(TransferMessage::Duplicate { transfer_id })
            .await
            .unwrap();

        match server_task.await.unwrap() {
            SessionEvent::TransferDuplicate { transfer_id: seen } => {
                assert_eq!(seen, transfer_id);
            }
            other => panic!("expected a duplicate event, got {other:?}"),
        }
        control.close().await;
        connection.close(0u32.into(), b"done");
        endpoint.wait_idle().await;
    }

    #[tokio::test]
    async fn clipboard_offer_text_and_ack_round_trip() {
        let server_identity = Identity::generate();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let text = "clipboard text over mTLS";
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        let offer_id = Uuid::now_v7();

        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let peer = crate::handshake::server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &connection,
                &server_identity,
                &peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            let offer = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            match offer {
                SessionEvent::ClipboardOffer {
                    offer_id: id,
                    size_bytes,
                    digest: received_digest,
                    preview,
                    ttl_ms,
                    origin_id: received_origin,
                    hop,
                    ..
                } => {
                    assert_eq!(id, offer_id);
                    assert_eq!(size_bytes, text.len() as u64);
                    assert_eq!(received_digest, digest);
                    assert_eq!(preview, text);
                    assert!(ttl_ms > 0);
                    assert!(received_origin.is_some());
                    assert_eq!(hop, 0);
                }
                other => panic!("expected clipboard offer, got {other:?}"),
            }
            control
                .send_clipboard_ack(offer_id, true, false, "", "")
                .await
                .unwrap();
            let body = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            match body {
                SessionEvent::ClipboardText {
                    offer_id: id,
                    text: received,
                    digest: received_digest,
                } => {
                    assert_eq!(id, offer_id);
                    assert_eq!(received, text);
                    assert_eq!(received_digest, digest);
                }
                other => panic!("expected clipboard text, got {other:?}"),
            }
            control
                .send_clipboard_ack(offer_id, true, true, "", "")
                .await
                .unwrap();
            control.close().await;
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| true,
        )
        .await
        .unwrap();
        let mut control = ControlSession::open_with_config(
            &connection,
            &client_identity,
            &peer,
            test_config(),
            None,
        )
        .await
        .unwrap();

        let origin_id = client_identity.device_id();
        control
            .send_clipboard_offer(ClipboardShareOffer {
                offer_id,
                size_bytes: text.len() as u64,
                digest,
                preview: text.to_owned(),
                ttl_ms: 120_000,
                origin_id,
                hop: 0,
            })
            .await
            .unwrap();
        match timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap()
        {
            SessionEvent::ClipboardAck {
                offer_id: id,
                accepted,
                applied,
                ..
            } => {
                assert_eq!(id, offer_id);
                assert!(accepted);
                assert!(!applied);
            }
            other => panic!("expected clipboard accept ack, got {other:?}"),
        }

        control
            .send_clipboard_text(offer_id, text, digest)
            .await
            .unwrap();
        match timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap()
        {
            SessionEvent::ClipboardAck {
                offer_id: id,
                accepted,
                applied,
                ..
            } => {
                assert_eq!(id, offer_id);
                assert!(accepted);
                assert!(applied);
            }
            other => panic!("expected clipboard applied ack, got {other:?}"),
        }

        control.close().await;
        server_task.await.unwrap();
        connection.close(0u32.into(), b"done");
        endpoint.wait_idle().await;
    }

    #[tokio::test]
    async fn duplicate_send_id_is_acked_but_delivered_once() {
        let server_identity = Identity::generate();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let peer = crate::handshake::server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &connection,
                &server_identity,
                &peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            let event = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            // Stay open long enough for both ACKs to be written.
            timeout(Duration::from_secs(2), control.next_event())
                .await
                .ok();
            control.close().await;
            event
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| true,
        )
        .await
        .unwrap();
        let mut control = ControlSession::open_with_config(
            &connection,
            &client_identity,
            &peer,
            test_config(),
            None,
        )
        .await
        .unwrap();
        let message_id = Uuid::now_v7();
        let first = control
            .send_text_with_id(message_id, "retry me")
            .await
            .unwrap();
        assert_eq!(first, message_id);
        let first_ack = timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            first_ack,
            SessionEvent::Ack {
                message_id: ack_id,
                delivered: true,
            } if ack_id == message_id
        ));
        let second = control
            .send_text_with_id(message_id, "retry me")
            .await
            .unwrap();
        assert_eq!(second, message_id);
        let second_ack = timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            second_ack,
            SessionEvent::Ack {
                message_id: ack_id,
                delivered: true,
            } if ack_id == message_id
        ));

        match server_task.await.unwrap() {
            SessionEvent::Message {
                message_id: id,
                text,
                ..
            } => {
                assert_eq!(id, message_id);
                assert_eq!(text, "retry me");
            }
            other => panic!("expected one message, got {other:?}"),
        }
        control.close().await;
        connection.close(0u32.into(), b"done");
        endpoint.wait_idle().await;
    }

    #[tokio::test]
    async fn resume_rejects_changed_client_certificate() {
        let server_identity = Identity::generate();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut trust = TrustStore::default();
            let mut limiter = PairingLimiter::default();
            let first = accept(&server).await.unwrap();
            let _ = crate::handshake::server_pair(
                &first,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await
            .unwrap();
            let second = accept(&server).await.unwrap();
            let result = crate::handshake::server_pair(
                &second,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await;
            first.close(0u32.into(), b"done");
            result
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let mut trust = TrustStore::default();
        let first_endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let first = connect(&first_endpoint, addr).await.unwrap();
        let _ = client_pair(&first, &client_identity, &client_tls, &mut trust, |_| true)
            .await
            .unwrap();
        first.close(0u32.into(), b"reconnect");
        first_endpoint.wait_idle().await;

        let changed_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let second_endpoint = client_endpoint(loopback(), &changed_tls, server_fingerprint)
            .await
            .unwrap();
        let second = connect(&second_endpoint, addr).await.unwrap();
        let result =
            crate::handshake::client_resume(&second, &client_identity, &changed_tls, &mut trust)
                .await;
        assert!(result.is_err());

        let server_result = server_task.await.unwrap();
        assert!(matches!(server_result, Err(HandshakeError::Untrusted(_))));
        second.close(0u32.into(), b"done");
        second_endpoint.wait_idle().await;
    }

    #[tokio::test]
    async fn persistent_session_reconnects_after_drop() {
        let server_identity = Identity::generate();
        let server_id = server_identity.device_id();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut trust = TrustStore::default();
            let mut limiter = PairingLimiter::default();

            let first = accept(&server).await.unwrap();
            let _first_peer = crate::handshake::server_pair(
                &first,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await
            .unwrap();

            let second = accept(&server).await.unwrap();
            let second_peer = crate::handshake::server_pair(
                &second,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &second,
                &server_identity,
                &second_peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            let before = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            control.close().await;
            second.close(0u32.into(), b"drop");

            let third = accept(&server).await.unwrap();
            let third_peer = crate::handshake::server_pair(
                &third,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &third,
                &server_identity,
                &third_peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            let after = timeout(Duration::from_secs(5), control.next_event())
                .await
                .unwrap()
                .unwrap();
            control.close().await;
            first.close(0u32.into(), b"done");
            (before, after, second_peer.resumed, third_peer.resumed)
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let mut trust = TrustStore::default();
        let first_endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let first = connect(&first_endpoint, addr).await.unwrap();
        let _peer = client_pair(&first, &client_identity, &client_tls, &mut trust, |_| true)
            .await
            .unwrap();
        first.close(0u32.into(), b"reconnect");
        first_endpoint.wait_idle().await;

        let mut persistent =
            PersistentSession::connect(addr, server_id, client_identity, client_tls, trust)
                .await
                .unwrap();
        persistent.send_text("before drop").await.unwrap();

        let mut reconnected = false;
        for _ in 0..6 {
            match timeout(Duration::from_secs(6), persistent.next_event())
                .await
                .unwrap()
                .unwrap()
            {
                SessionEvent::Reconnected { .. } => {
                    reconnected = true;
                    break;
                }
                SessionEvent::Ack { .. } => continue,
                other => panic!("unexpected event before reconnect: {other:?}"),
            }
        }
        assert!(reconnected);
        persistent.send_text("after reconnect").await.unwrap();
        let ack = timeout(Duration::from_secs(3), persistent.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            ack,
            SessionEvent::Ack {
                delivered: true,
                ..
            }
        ));

        let (before, after, second_resumed, third_resumed) = server_task.await.unwrap();
        assert!(second_resumed);
        assert!(third_resumed);
        match before {
            SessionEvent::Message { text, .. } => assert_eq!(text, "before drop"),
            other => panic!("expected first message, got {other:?}"),
        }
        match after {
            SessionEvent::Message { text, .. } => assert_eq!(text, "after reconnect"),
            other => panic!("expected second message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn heartbeat_prevents_idle_disconnect() {
        let server_identity = Identity::generate();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let peer = crate::handshake::server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &connection,
                &server_identity,
                &peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            timeout(Duration::from_secs(3), control.next_event())
                .await
                .unwrap()
                .unwrap()
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&endpoint, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| true,
        )
        .await
        .unwrap();
        let mut control = ControlSession::open_with_config(
            &connection,
            &client_identity,
            &peer,
            test_config(),
            None,
        )
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(700)).await;
        control.send_text("still alive").await.unwrap();
        let ack = timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            ack,
            SessionEvent::Ack {
                delivered: true,
                ..
            }
        ));

        match server_task.await.unwrap() {
            SessionEvent::Message { text, .. } => assert_eq!(text, "still alive"),
            other => panic!("expected message, got {other:?}"),
        }
        control.close().await;
        connection.close(0u32.into(), b"done");
        endpoint.wait_idle().await;
    }

    #[tokio::test]
    async fn trusted_resume_skips_sas_and_opens_control() {
        let server_identity = Identity::generate();
        let server_id = server_identity.device_id();
        let server_tls = crate::tls::TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut trust = TrustStore::default();
            let mut limiter = PairingLimiter::default();

            let first = accept(&server).await.unwrap();
            let first_peer = crate::handshake::server_pair(
                &first,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await
            .unwrap();

            let second = accept(&server).await.unwrap();
            let second_peer = crate::handshake::server_pair(
                &second,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut limiter,
                |_| true,
            )
            .await
            .unwrap();
            let mut control = ControlSession::accept_with_config(
                &second,
                &server_identity,
                &second_peer,
                test_config(),
                None,
            )
            .await
            .unwrap();
            first.close(0u32.into(), b"reconnect");
            let event = timeout(Duration::from_secs(2), control.next_event())
                .await
                .unwrap()
                .unwrap();
            control.close().await;
            (first_peer.resumed, second_peer.resumed, event)
        });

        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let mut trust = TrustStore::default();

        let first_endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let first = connect(&first_endpoint, addr).await.unwrap();
        let first_peer = client_pair(&first, &client_identity, &client_tls, &mut trust, |_| true)
            .await
            .unwrap();
        assert!(!first_peer.resumed);
        first.close(0u32.into(), b"reconnect");
        first_endpoint.wait_idle().await;

        let mut control =
            connect_trusted(addr, server_id, &client_identity, &client_tls, &mut trust)
                .await
                .unwrap();
        assert!(control.peer().resumed);
        control.send_text("after resume").await.unwrap();
        let ack = timeout(Duration::from_secs(2), control.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            ack,
            SessionEvent::Ack {
                delivered: true,
                ..
            }
        ));

        let (first_resumed, second_resumed, event) = server_task.await.unwrap();
        assert!(!first_resumed);
        assert!(second_resumed);
        match event {
            SessionEvent::Message { text, .. } => assert_eq!(text, "after resume"),
            other => panic!("expected message, got {other:?}"),
        }

        control.close().await;
    }

    #[tokio::test]
    async fn untrusted_server_is_rejected_before_connecting() {
        let client_identity = Identity::generate();
        let client_tls = crate::tls::TlsIdentity::generate("client").unwrap();
        let mut trust = TrustStore::default();
        let error = match connect_trusted(
            SocketAddr::from(([127, 0, 0, 1], 9)),
            Uuid::now_v7(),
            &client_identity,
            &client_tls,
            &mut trust,
        )
        .await
        {
            Ok(_) => panic!("untrusted server unexpectedly connected"),
            Err(error) => error,
        };
        assert!(matches!(error, SessionError::Untrusted(_)));
        assert!(trust.is_empty());
    }
}
