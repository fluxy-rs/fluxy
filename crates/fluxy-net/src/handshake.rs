use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use quinn::{Connection, SendStream};
use thiserror::Error;
use tokio::time::timeout;
use uuid::Uuid;

use fluxy_core::{Error, ErrorCode, Identity, SasInput, TrustEntry, TrustStore, compute_sas};
use fluxy_proto::{
    Capabilities, Envelope, Error as WireError, Hello, HelloAck, MessageKind, PROTOCOL_VERSION,
    PairConfirm, PairReply, PairRequest, Ready, decode_payload, encode_payload,
};

use crate::tls::TlsIdentity;
use crate::transport::{TransportError, read_envelope, write_envelope};

// A pairing handshake includes a human SAS comparison on both devices. The
// limit is bounded but long enough for that comparison; network steps are
// individually bounded by QUIC and the transport timeouts.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("protocol error: {0}")]
    Protocol(#[from] fluxy_proto::ProtoError),
    #[error("core error: {0}")]
    Core(#[from] Error),
    #[error("identity error: {0}")]
    Identity(String),
    #[error("untrusted peer: {0}")]
    Untrusted(String),
    #[error("pairing rejected: {0}")]
    Pairing(String),
    #[error("SAS mismatch")]
    SasMismatch,
    #[error("pairing temporarily blocked; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("handshake timed out")]
    Timeout,
    #[error("peer did not present a TLS certificate")]
    NoPeerCertificate,
    #[error("random source error: {0}")]
    Random(#[from] getrandom::Error),
}

impl From<HandshakeError> for Error {
    fn from(value: HandshakeError) -> Self {
        let (code, retryable) = match &value {
            HandshakeError::Untrusted(_) | HandshakeError::Identity(_) => (ErrorCode::Trust, false),
            HandshakeError::RateLimited { .. } => (ErrorCode::Trust, true),
            HandshakeError::Random(_) => (ErrorCode::Internal, false),
            HandshakeError::Timeout => (ErrorCode::Timeout, true),
            _ => (ErrorCode::Protocol, false),
        };
        Error::new(code, value.to_string(), retryable)
    }
}

/// The locally derived SAS and the peer metadata known when it is shown.
/// The SAS is the only value a human compares; the rest is display context.
#[derive(Debug, Clone)]
pub struct PairingPrompt {
    pub sas: String,
    pub peer_device_id: Uuid,
    pub peer_name: String,
    pub peer_platform: String,
}

#[derive(Debug, Clone)]
pub struct PeerSession {
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    /// Display-only hardware hints from the peer. Older peers send nothing, so
    /// these are empty rather than wrong.
    pub manufacturer: String,
    pub model: String,
    pub os_version: String,
    pub public_key: [u8; 32],
    pub cert_fingerprint: [u8; 32],
    pub sas: String,
    pub session_id: Uuid,
    pub capabilities: Capabilities,
    pub resumed: bool,
}

/// In-memory per-IP pairing throttle for responder-side attempts.
#[derive(Debug)]
pub struct PairingLimiter {
    failures: HashMap<IpAddr, FailureState>,
    max_failures: u32,
    base_delay: Duration,
    max_delay: Duration,
}

#[derive(Debug)]
struct FailureState {
    failures: u32,
    blocked_until: Instant,
}

impl Default for PairingLimiter {
    fn default() -> Self {
        Self {
            failures: HashMap::new(),
            max_failures: 5,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
        }
    }
}

impl PairingLimiter {
    /// Reject a peer that is still inside a cooldown window.
    pub fn check(&self, peer: IpAddr) -> Result<(), HandshakeError> {
        let Some(state) = self.failures.get(&peer) else {
            return Ok(());
        };
        if state.failures < self.max_failures {
            return Ok(());
        }
        let now = Instant::now();
        if state.blocked_until > now {
            let retry_after_secs = state.blocked_until.duration_since(now).as_secs().max(1);
            return Err(HandshakeError::RateLimited { retry_after_secs });
        }
        Ok(())
    }

    /// Record one failed attempt, extending the cooldown after the threshold.
    pub fn record_failure(&mut self, peer: IpAddr) {
        let state = self.failures.entry(peer).or_insert(FailureState {
            failures: 0,
            blocked_until: Instant::now(),
        });
        state.failures = state.failures.saturating_add(1);
        if state.failures >= self.max_failures {
            let exponent = (state.failures - self.max_failures).min(6);
            let seconds = self
                .base_delay
                .as_secs()
                .saturating_mul(1u64 << exponent)
                .min(self.max_delay.as_secs());
            state.blocked_until = Instant::now() + Duration::from_secs(seconds);
        }
    }

    /// Clear the failure history after a successful pairing.
    pub fn record_success(&mut self, peer: IpAddr) {
        self.failures.remove(&peer);
    }
}

impl PeerSession {
    pub fn trust_entry(&self) -> TrustEntry {
        TrustEntry {
            device_id: self.device_id,
            name: self.name.clone(),
            platform: self.platform.clone(),
            public_key: self.public_key,
            cert_fingerprint: self.cert_fingerprint,
            paired_at_ms: now_ms(),
            clipboard_policy: fluxy_core::ClipboardPolicy::Ask,
            // A fresh pairing inherits the global switch; the user can
            // override it per device afterwards.
            auto_accept: fluxy_core::AutoAcceptPolicy::Inherit,
        }
    }
}

/// Client side of the initial SAS pairing handshake.
pub async fn client_pair<F>(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
    confirm: F,
) -> Result<PeerSession, HandshakeError>
where
    F: Fn(&str) -> bool + Send + Sync,
{
    client_pair_async(connection, identity, tls, trust, |prompt| {
        std::future::ready(confirm(&prompt.sas))
    })
    .await
}

/// Client side of the initial SAS pairing with an asynchronous confirmation.
/// The future may wait for a UI action without blocking the runtime.
pub async fn client_pair_async<F, Fut>(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
    confirm: F,
) -> Result<PeerSession, HandshakeError>
where
    F: Fn(PairingPrompt) -> Fut + Send + Sync,
    Fut: Future<Output = bool> + Send,
{
    client_handshake(connection, identity, tls, trust, false, confirm).await
}

/// Client side of a trusted resume. No SAS is shown; both certificates and
/// public keys must match an existing trust entry.
pub async fn client_resume(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
) -> Result<PeerSession, HandshakeError> {
    client_handshake(connection, identity, tls, trust, true, |_| {
        std::future::ready(true)
    })
    .await
}

async fn client_handshake<F, Fut>(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
    resuming: bool,
    confirm: F,
) -> Result<PeerSession, HandshakeError>
where
    F: Fn(PairingPrompt) -> Fut + Send + Sync,
    Fut: Future<Output = bool> + Send,
{
    timeout(HANDSHAKE_TIMEOUT, async {
        let (mut send, mut recv) = match connection.open_bi().await {
            Ok(pair) => pair,
            Err(error) => {
                // "connection lost" alone says nothing about who closed it.
                tracing::warn!(
                    %error,
                    reason = ?connection.close_reason(),
                    remote = %connection.remote_address(),
                    "could not open the handshake stream"
                );
                return Err(HandshakeError::Transport(TransportError::Stream(
                    error.to_string(),
                )));
            }
        };

        tracing::info!("client handshake: sending hello");
        let nonce = random_nonce()?;
        let hello = make_hello(identity, tls, nonce, resuming);
        let hello_envelope =
            make_envelope(MessageKind::Hello, identity, None, encode_payload(&hello));
        write_envelope(&mut send, &hello_envelope).await?;

        let ack_envelope = match read_envelope(&mut recv).await {
            Ok(envelope) => {
                tracing::info!("client handshake: hello acknowledged");
                envelope
            }
            Err(error) => {
                tracing::warn!(%error, reason = ?connection.close_reason(), "client handshake: reading the hello ack failed");
                return Err(error.into());
            }
        };
        ensure_kind(&ack_envelope, MessageKind::HelloAck)?;
        let ack: HelloAck = decode_payload(&ack_envelope.payload)?;
        let peer_cert =
            peer_certificate_fingerprint(connection).ok_or(HandshakeError::NoPeerCertificate)?;
        verify_hello_ack(&ack, &peer_cert)?;
        if ack.resuming != resuming {
            return Err(HandshakeError::Pairing(
                "HelloAck resume mode mismatch".into(),
            ));
        }

        let session_id = Uuid::now_v7();
        let local_caps = default_capabilities();
        let caps_envelope = make_envelope(
            MessageKind::Capabilities,
            identity,
            Some(session_id),
            encode_payload(&local_caps),
        );
        write_envelope(&mut send, &caps_envelope).await?;
        let peer_caps_envelope = match read_envelope(&mut recv).await {
            Ok(envelope) => {
                tracing::info!("client handshake: capabilities exchanged");
                envelope
            }
            Err(error) => {
                tracing::warn!(%error, reason = ?connection.close_reason(), "client handshake: reading capabilities failed");
                return Err(error.into());
            }
        };
        ensure_kind(&peer_caps_envelope, MessageKind::Capabilities)?;
        ensure_session(&peer_caps_envelope, session_id)?;
        let peer_caps: Capabilities = decode_payload(&peer_caps_envelope.payload)?;
        let capabilities = negotiate_capabilities(&local_caps, &peer_caps);

        let peer_public = fixed::<32>(&ack.public_key, "HelloAck public key")?;
        let peer_device = uuid_from_slice(&ack.device_id)?;
        let peer_name = ack.name.clone();
        let peer_platform = ack.os.clone();
        let peer_manufacturer = ack.manufacturer.clone();
        let peer_model = ack.model.clone();
        let peer_os_version = ack.os_version.clone();
        let mut sas = String::new();
        if resuming {
            verify_trusted_peer(trust, &ack.device_id, &ack.public_key, &peer_cert)?;
        } else {
            let peer_nonce = fixed::<32>(&ack.nonce, "HelloAck nonce")?;
            sas = compute_sas(
                &local_sas(identity, tls, nonce),
                &peer_sas(ack.device_id.as_slice(), peer_nonce, peer_cert)?,
            )?;

            let accepted = confirm(PairingPrompt {
                sas: sas.clone(),
                peer_device_id: peer_device,
                peer_name: peer_name.clone(),
                peer_platform: peer_platform.clone(),
            })
            .await;
            if !accepted {
                let request = PairRequest {
                    sas: sas.clone(),
                    confirmed: false,
                };
                let envelope = make_envelope(
                    MessageKind::PairRequest,
                    identity,
                    Some(session_id),
                    encode_payload(&request),
                );
                let _ = write_envelope(&mut send, &envelope).await;
                return Err(HandshakeError::Pairing("SAS rejected by user".into()));
            }

            let request = PairRequest {
                sas: sas.clone(),
                confirmed: true,
            };
            let envelope = make_envelope(
                MessageKind::PairRequest,
                identity,
                Some(session_id),
                encode_payload(&request),
            );
            write_envelope(&mut send, &envelope).await?;

            let reply_envelope = read_envelope(&mut recv).await?;
            ensure_kind(&reply_envelope, MessageKind::PairReply)?;
            ensure_session(&reply_envelope, session_id)?;
            let reply: PairReply = decode_payload(&reply_envelope.payload)?;
            if !reply.accepted {
                return Err(HandshakeError::Pairing(reply.message));
            }

            let confirmation = PairConfirm { accepted: true };
            let envelope = make_envelope(
                MessageKind::PairConfirm,
                identity,
                Some(session_id),
                encode_payload(&confirmation),
            );
            write_envelope(&mut send, &envelope).await?;
        }

        let ready_envelope = read_envelope(&mut recv).await?;
        ensure_kind(&ready_envelope, MessageKind::Ready)?;
        ensure_session(&ready_envelope, session_id)?;
        let ready: Ready = decode_payload(&ready_envelope.payload)?;
        if !ready.paired {
            return Err(HandshakeError::Pairing(
                "peer did not complete pairing".into(),
            ));
        }

        let peer = PeerSession {
            device_id: peer_device,
            name: peer_name,
            platform: peer_platform,
            manufacturer: peer_manufacturer,
            model: peer_model,
            os_version: peer_os_version,
            public_key: peer_public,
            cert_fingerprint: peer_cert,
            sas,
            session_id,
            capabilities,
            resumed: resuming,
        };
        if !resuming {
            trust.insert(peer.trust_entry());
        }
        Ok(peer)
    })
    .await
    .map_err(|_| HandshakeError::Timeout)?
}

/// Server side of the same handshake. `confirm` is called with the
/// locally derived SAS before any trust entry is written. Failed attempts are
/// recorded in the supplied per-IP limiter.
pub async fn server_pair<F>(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
    limiter: &mut PairingLimiter,
    confirm: F,
) -> Result<PeerSession, HandshakeError>
where
    F: Fn(&str) -> bool + Send + Sync,
{
    // The synchronous caller has no UI to notify early, so the observer is a no-op.
    server_pair_async(
        connection,
        identity,
        tls,
        trust,
        limiter,
        |_| {},
        |prompt| std::future::ready(confirm(&prompt.sas)),
    )
    .await
}

/// Server side of the same handshake with an asynchronous confirmation.
pub async fn server_pair_async<F, Fut, G>(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
    limiter: &mut PairingLimiter,
    on_code: G,
    confirm: F,
) -> Result<PeerSession, HandshakeError>
where
    F: Fn(PairingPrompt) -> Fut + Send + Sync,
    Fut: Future<Output = bool> + Send,
    G: Fn(&PairingPrompt) + Send + Sync,
{
    let peer = connection.remote_address().ip();
    limiter.check(peer)?;
    let result = server_pair_attempt(connection, identity, tls, trust, on_code, confirm).await;
    match &result {
        Ok(_) => limiter.record_success(peer),
        Err(_) => limiter.record_failure(peer),
    }
    result
}

async fn server_pair_attempt<F, Fut, G>(
    connection: &Connection,
    identity: &Identity,
    tls: &TlsIdentity,
    trust: &mut TrustStore,
    on_code: G,
    confirm: F,
) -> Result<PeerSession, HandshakeError>
where
    F: Fn(PairingPrompt) -> Fut + Send + Sync,
    Fut: Future<Output = bool> + Send,
    G: Fn(&PairingPrompt) + Send + Sync,
{
    timeout(HANDSHAKE_TIMEOUT, async {
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .map_err(|e| HandshakeError::Transport(TransportError::Stream(e.to_string())))?;

        let hello_envelope = read_envelope(&mut recv).await?;
        ensure_kind(&hello_envelope, MessageKind::Hello)?;
        let hello: Hello = decode_payload(&hello_envelope.payload)?;
        let peer_cert =
            peer_certificate_fingerprint(connection).ok_or(HandshakeError::NoPeerCertificate)?;
        verify_hello(&hello, &peer_cert)?;
        let peer_device = uuid_from_slice(&hello.device_id)?;
        let peer_name = hello.name.clone();
        let peer_platform = hello.os.clone();
        let peer_manufacturer = hello.manufacturer.clone();
        let peer_model = hello.model.clone();
        let peer_os_version = hello.os_version.clone();
        let peer_public = fixed::<32>(&hello.public_key, "Hello public key")?;
        let resuming = hello.resuming;

        if resuming
            && let Err(error) =
                verify_trusted_peer(trust, &hello.device_id, &hello.public_key, &peer_cert)
        {
            send_error(
                &mut send,
                identity,
                None,
                ErrorCode::Trust,
                &error.to_string(),
            )
            .await;
            return Err(error);
        }

        let nonce = random_nonce()?;
        let ack = make_hello_ack(identity, tls, nonce, resuming);
        let ack_envelope =
            make_envelope(MessageKind::HelloAck, identity, None, encode_payload(&ack));
        write_envelope(&mut send, &ack_envelope).await?;

        let caps_envelope = read_envelope(&mut recv).await?;
        ensure_kind(&caps_envelope, MessageKind::Capabilities)?;
        let session_id = uuid_from_slice(&caps_envelope.session_id)?;
        if session_id.is_nil() {
            return Err(HandshakeError::Pairing(
                "capabilities require a session id".into(),
            ));
        }
        let peer_caps: Capabilities = decode_payload(&caps_envelope.payload)?;
        let local_caps = default_capabilities();
        let capabilities = negotiate_capabilities(&local_caps, &peer_caps);
        let caps_reply = make_envelope(
            MessageKind::Capabilities,
            identity,
            Some(session_id),
            encode_payload(&local_caps),
        );
        write_envelope(&mut send, &caps_reply).await?;

        let mut sas = String::new();
        if !resuming {
            let peer_nonce = fixed::<32>(&hello.nonce, "Hello nonce")?;
            sas = compute_sas(
                &local_sas(identity, tls, nonce),
                &peer_sas(&hello.device_id, peer_nonce, peer_cert)?,
            )?;

            // Both sides now know the code; the responder shows it before the
            // initiator's acknowledgement arrives, so the two screens can be
            // compared at the same moment.
            on_code(&PairingPrompt {
                sas: sas.clone(),
                peer_device_id: peer_device,
                peer_name: peer_name.clone(),
                peer_platform: peer_platform.clone(),
            });

            let request_envelope = read_envelope(&mut recv).await?;
            ensure_kind(&request_envelope, MessageKind::PairRequest)?;
            let request: PairRequest = decode_payload(&request_envelope.payload)?;
            ensure_session(&request_envelope, session_id)?;

            if !request.confirmed {
                send_pair_rejection(
                    &mut send,
                    identity,
                    session_id,
                    &sas,
                    "peer rejected pairing",
                )
                .await;
                return Err(HandshakeError::Pairing("peer rejected pairing".into()));
            }
            if request.sas != sas {
                send_pair_rejection(&mut send, identity, session_id, &sas, "SAS mismatch").await;
                return Err(HandshakeError::SasMismatch);
            }
            let accepted = confirm(PairingPrompt {
                sas: sas.clone(),
                peer_device_id: peer_device,
                peer_name: peer_name.clone(),
                peer_platform: peer_platform.clone(),
            })
            .await;
            if !accepted {
                send_pair_rejection(
                    &mut send,
                    identity,
                    session_id,
                    &sas,
                    "SAS rejected by user",
                )
                .await;
                return Err(HandshakeError::Pairing("SAS rejected by user".into()));
            }

            let reply = PairReply {
                accepted: true,
                sas: sas.clone(),
                message: String::new(),
            };
            let envelope = make_envelope(
                MessageKind::PairReply,
                identity,
                Some(session_id),
                encode_payload(&reply),
            );
            write_envelope(&mut send, &envelope).await?;

            let confirm_envelope = read_envelope(&mut recv).await?;
            ensure_kind(&confirm_envelope, MessageKind::PairConfirm)?;
            ensure_session(&confirm_envelope, session_id)?;
            let confirmation: PairConfirm = decode_payload(&confirm_envelope.payload)?;
            if !confirmation.accepted {
                return Err(HandshakeError::Pairing("peer rejected pairing".into()));
            }
        }

        let ready = Ready { paired: true };
        let envelope = make_envelope(
            MessageKind::Ready,
            identity,
            Some(session_id),
            encode_payload(&ready),
        );
        write_envelope(&mut send, &envelope).await?;

        let peer = PeerSession {
            device_id: peer_device,
            name: peer_name,
            platform: peer_platform,
            manufacturer: peer_manufacturer,
            model: peer_model,
            os_version: peer_os_version,
            public_key: peer_public,
            cert_fingerprint: peer_cert,
            sas,
            session_id,
            capabilities,
            resumed: resuming,
        };
        if !resuming {
            trust.insert(peer.trust_entry());
        }
        Ok(peer)
    })
    .await
    .map_err(|_| HandshakeError::Timeout)?
}

async fn send_pair_rejection(
    send: &mut SendStream,
    identity: &Identity,
    session_id: Uuid,
    sas: &str,
    message: &str,
) {
    let reply = PairReply {
        accepted: false,
        sas: sas.to_string(),
        message: message.to_string(),
    };
    let envelope = make_envelope(
        MessageKind::PairReply,
        identity,
        Some(session_id),
        encode_payload(&reply),
    );
    let _ = write_envelope(send, &envelope).await;
}

fn make_hello(identity: &Identity, tls: &TlsIdentity, nonce: [u8; 32], resuming: bool) -> Hello {
    let info = fluxy_sysinfo::cached();
    let mut hello = Hello {
        protocol_version: PROTOCOL_VERSION,
        device_id: identity.device_id().as_bytes().to_vec(),
        name: device_name(),
        os: std::env::consts::OS.to_string(),
        cert_fingerprint: tls.fingerprint.to_vec(),
        nonce: nonce.to_vec(),
        public_key: identity.public_key().to_vec(),
        transcript_signature: Vec::new(),
        resuming,
        manufacturer: info.manufacturer.clone(),
        model: info.model.clone(),
        os_version: info.display_os(),
    };
    hello.transcript_signature = identity.sign(&hello_transcript(&hello)).to_bytes().to_vec();
    hello
}

fn make_hello_ack(
    identity: &Identity,
    tls: &TlsIdentity,
    nonce: [u8; 32],
    resuming: bool,
) -> HelloAck {
    let info = fluxy_sysinfo::cached();
    let mut ack = HelloAck {
        protocol_version: PROTOCOL_VERSION,
        device_id: identity.device_id().as_bytes().to_vec(),
        name: device_name(),
        os: std::env::consts::OS.to_string(),
        cert_fingerprint: tls.fingerprint.to_vec(),
        nonce: nonce.to_vec(),
        transcript_signature: Vec::new(),
        public_key: identity.public_key().to_vec(),
        resuming,
        manufacturer: info.manufacturer.clone(),
        model: info.model.clone(),
        os_version: info.display_os(),
    };
    ack.transcript_signature = identity
        .sign(&hello_ack_transcript(&ack))
        .to_bytes()
        .to_vec();
    ack
}

fn verify_hello(hello: &Hello, peer_cert: &[u8; 32]) -> Result<(), HandshakeError> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(HandshakeError::Identity("protocol version mismatch".into()));
    }
    if hello.cert_fingerprint.as_slice() != peer_cert {
        return Err(HandshakeError::Identity(
            "Hello cert fingerprint mismatch".into(),
        ));
    }
    let public = fixed::<32>(&hello.public_key, "Hello public key")?;
    Identity::verify_bytes(
        &public,
        &hello_transcript(hello),
        &hello.transcript_signature,
    )
    .map_err(|e| HandshakeError::Identity(e.to_string()))
}

fn verify_hello_ack(ack: &HelloAck, peer_cert: &[u8; 32]) -> Result<(), HandshakeError> {
    if ack.protocol_version != PROTOCOL_VERSION {
        return Err(HandshakeError::Identity("protocol version mismatch".into()));
    }
    if ack.cert_fingerprint.as_slice() != peer_cert {
        return Err(HandshakeError::Identity(
            "HelloAck cert fingerprint mismatch".into(),
        ));
    }
    let public = fixed::<32>(&ack.public_key, "HelloAck public key")?;
    Identity::verify_bytes(
        &public,
        &hello_ack_transcript(ack),
        &ack.transcript_signature,
    )
    .map_err(|e| HandshakeError::Identity(e.to_string()))
}

fn hello_transcript(hello: &Hello) -> Vec<u8> {
    transcript(
        b"fluxy-hello-v1",
        hello.protocol_version,
        &[
            &hello.device_id,
            hello.name.as_bytes(),
            hello.os.as_bytes(),
            &hello.cert_fingerprint,
            &hello.nonce,
            &hello.public_key,
            &[hello.resuming as u8],
        ],
    )
}

fn hello_ack_transcript(ack: &HelloAck) -> Vec<u8> {
    transcript(
        b"fluxy-hello-ack-v1",
        ack.protocol_version,
        &[
            &ack.device_id,
            ack.name.as_bytes(),
            ack.os.as_bytes(),
            &ack.cert_fingerprint,
            &ack.nonce,
            &ack.public_key,
            &[ack.resuming as u8],
        ],
    )
}

fn transcript(prefix: &[u8], version: u32, fields: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(prefix);
    out.extend_from_slice(&version.to_be_bytes());
    for field in fields {
        out.extend_from_slice(&(field.len() as u32).to_be_bytes());
        out.extend_from_slice(field);
    }
    out
}

fn local_sas(identity: &Identity, tls: &TlsIdentity, nonce: [u8; 32]) -> SasInput {
    SasInput {
        device_id: identity.device_id(),
        nonce,
        cert_fingerprint: tls.fingerprint,
    }
}

fn peer_sas(device: &[u8], nonce: [u8; 32], cert: [u8; 32]) -> Result<SasInput, HandshakeError> {
    Ok(SasInput {
        device_id: uuid_from_slice(device)?,
        nonce,
        cert_fingerprint: cert,
    })
}

pub(crate) fn make_envelope(
    kind: MessageKind,
    identity: &Identity,
    session: Option<Uuid>,
    payload: Vec<u8>,
) -> Envelope {
    let mut message_id = Uuid::now_v7().as_bytes().to_vec();
    message_id[6] = 0x70;
    Envelope {
        protocol_version: PROTOCOL_VERSION,
        message_id,
        source_device_id: identity.device_id().as_bytes().to_vec(),
        session_id: session.unwrap_or_else(Uuid::nil).as_bytes().to_vec(),
        created_at_ms: now_ms(),
        kind: kind as i32,
        payload,
    }
}

fn verify_trusted_peer(
    trust: &TrustStore,
    device_id: &[u8],
    public_key: &[u8],
    cert_fingerprint: &[u8],
) -> Result<(), HandshakeError> {
    let device_id = uuid_from_slice(device_id)?;
    let entry = trust.get(&device_id).ok_or_else(|| {
        HandshakeError::Untrusted(format!("{device_id} is not in the trust store"))
    })?;
    if entry.public_key.as_slice() != public_key {
        return Err(HandshakeError::Untrusted(
            "peer public key changed since pairing".into(),
        ));
    }
    if entry.cert_fingerprint.as_slice() != cert_fingerprint {
        return Err(HandshakeError::Untrusted(
            "peer certificate changed since pairing".into(),
        ));
    }
    Ok(())
}

async fn send_error(
    send: &mut SendStream,
    identity: &Identity,
    session_id: Option<Uuid>,
    code: ErrorCode,
    message: &str,
) {
    let error = WireError {
        code: u32::from(code.as_u16()),
        message: message.to_string(),
        retryable: false,
    };
    let envelope = make_envelope(
        MessageKind::Error,
        identity,
        session_id,
        encode_payload(&error),
    );
    let _ = write_envelope(send, &envelope).await;
}

fn ensure_kind(envelope: &Envelope, expected: MessageKind) -> Result<(), HandshakeError> {
    if envelope.kind != expected as i32 {
        return Err(HandshakeError::Pairing(format!(
            "expected {expected:?}, got {}",
            envelope.kind
        )));
    }
    Ok(())
}

fn ensure_session(envelope: &Envelope, expected: Uuid) -> Result<(), HandshakeError> {
    if envelope.session_id.as_slice() != expected.as_bytes() {
        return Err(HandshakeError::Pairing("session mismatch".into()));
    }
    Ok(())
}

fn default_capabilities() -> Capabilities {
    Capabilities {
        max_control_frame: fluxy_proto::MAX_CONTROL_FRAME as u32,
        max_chunk_size: 4 * 1024 * 1024,
        max_streams: 8,
        features: vec!["folder".into(), "resume".into(), "clipboard".into()],
        clipboard_max_bytes: 1024 * 1024,
    }
}

fn negotiate_capabilities(local: &Capabilities, peer: &Capabilities) -> Capabilities {
    let features = local
        .features
        .iter()
        .filter(|feature| peer.features.contains(feature))
        .cloned()
        .collect();
    Capabilities {
        max_control_frame: local.max_control_frame.min(peer.max_control_frame),
        max_chunk_size: local.max_chunk_size.min(peer.max_chunk_size),
        max_streams: local.max_streams.min(peer.max_streams),
        features,
        clipboard_max_bytes: local.clipboard_max_bytes.min(peer.clipboard_max_bytes),
    }
}

fn peer_certificate_fingerprint(connection: &Connection) -> Option<[u8; 32]> {
    let identity = connection.peer_identity()?;
    let certs = identity
        .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
        .ok()?;
    let cert = certs.first()?;
    Some(*blake3::hash(cert.as_ref()).as_bytes())
}

fn fixed<const N: usize>(bytes: &[u8], field: &str) -> Result<[u8; N], HandshakeError> {
    bytes
        .try_into()
        .map_err(|_| HandshakeError::Identity(format!("{field} must be {N} bytes")))
}

fn uuid_from_slice(bytes: &[u8]) -> Result<Uuid, HandshakeError> {
    Uuid::from_slice(bytes).map_err(|_| HandshakeError::Identity("invalid device id".into()))
}

fn random_nonce() -> Result<[u8; 32], getrandom::Error> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)?;
    Ok(nonce)
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Fluxy device".into())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use fluxy_core::{Identity, TrustStore};

    use crate::transport::{accept, client_endpoint, connect, server_endpoint};

    fn loopback() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    #[test]
    fn pairing_limiter_blocks_after_repeated_failures() {
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let mut limiter = PairingLimiter::default();
        for _ in 0..5 {
            limiter.record_failure(ip);
        }
        assert!(matches!(
            limiter.check(ip),
            Err(HandshakeError::RateLimited { .. })
        ));
        limiter.record_success(ip);
        assert!(limiter.check(ip).is_ok());
    }

    #[test]
    fn capabilities_use_the_intersection() {
        let local = Capabilities {
            max_control_frame: 8_000_000,
            max_chunk_size: 4_000_000,
            max_streams: 8,
            features: vec!["folder".into(), "clipboard".into()],
            clipboard_max_bytes: 1_000_000,
        };
        let peer = Capabilities {
            max_control_frame: 6_000_000,
            max_chunk_size: 4_000_000,
            max_streams: 2,
            features: vec!["clipboard".into(), "resume".into()],
            clipboard_max_bytes: 500_000,
        };
        let negotiated = negotiate_capabilities(&local, &peer);
        assert_eq!(negotiated.max_control_frame, 6_000_000);
        assert_eq!(negotiated.max_chunk_size, 4_000_000);
        assert_eq!(negotiated.max_streams, 2);
        assert_eq!(negotiated.clipboard_max_bytes, 500_000);
        assert_eq!(negotiated.features, vec!["clipboard"]);
    }

    #[tokio::test]
    async fn pair_writes_both_trust_stores_and_matches_sas() {
        let server_identity = Identity::generate();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let server_id = server_identity.device_id();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let session = server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await
            .map_err(|e| e.to_string());
            let _ = connection.closed().await;
            (trust, session)
        });

        let client_identity = Identity::generate();
        let client_tls = TlsIdentity::generate("client").unwrap();
        let client_id = client_identity.device_id();
        let client = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&client, addr).await.unwrap();
        let mut client_trust = TrustStore::default();
        let client_result = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut client_trust,
            |_| true,
        )
        .await;
        connection.close(0u32.into(), b"done");
        client.wait_idle().await;
        let (server_trust, server_result) = server_task.await.unwrap();
        let client_session = client_result
            .unwrap_or_else(|e| panic!("client failed: {e}; server: {server_result:?}"));
        let server_session = server_result.unwrap_or_else(|e| panic!("server failed: {e}"));
        assert_eq!(client_session.device_id, server_id);
        assert_eq!(server_session.device_id, client_id);
        assert_eq!(client_session.sas, server_session.sas);
        assert_eq!(client_session.session_id, server_session.session_id);
        assert!(client_trust.is_trusted(&server_id));
        assert!(server_trust.is_trusted(&client_id));
        assert_eq!(client_trust.len(), 1);
        assert_eq!(server_trust.len(), 1);
    }

    #[tokio::test]
    async fn async_pairing_allows_delayed_human_confirmation() {
        let server_identity = Identity::generate();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let server_id = server_identity.device_id();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let result = server_pair_async(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| {},
                |prompt| async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    prompt.sas.len() == 6
                },
            )
            .await;
            let _ = connection.closed().await;
            (trust, result)
        });

        let client_identity = Identity::generate();
        let client_tls = TlsIdentity::generate("client").unwrap();
        let client_id = client_identity.device_id();
        let client = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&client, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let result = client_pair_async(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |prompt| async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                prompt.sas.len() == 6
            },
        )
        .await;
        connection.close(0u32.into(), b"done");
        client.wait_idle().await;

        let (server_trust, server_result) = server_task.await.unwrap();
        let client_session = result.unwrap_or_else(|e| panic!("client failed: {e}"));
        let server_session = server_result.unwrap_or_else(|e| panic!("server failed: {e}"));
        assert_eq!(client_session.device_id, server_id);
        assert_eq!(server_session.device_id, client_id);
        assert_eq!(client_session.sas, server_session.sas);
        assert!(trust.is_trusted(&server_id));
        assert!(server_trust.is_trusted(&client_id));
    }

    #[tokio::test]
    async fn initiator_rejection_writes_no_trust() {
        let server_identity = Identity::generate();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let result = server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| true,
            )
            .await;
            (trust, result.is_err())
        });

        let client_identity = Identity::generate();
        let client_tls = TlsIdentity::generate("client").unwrap();
        let client = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&client, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let result = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| false,
        )
        .await;
        assert!(result.is_err());
        assert!(trust.is_empty());
        connection.close(0u32.into(), b"rejected");
        client.wait_idle().await;

        let (server_trust, server_rejected) = server_task.await.unwrap();
        assert!(server_rejected);
        assert!(server_trust.is_empty());
    }

    #[tokio::test]
    async fn user_rejection_writes_no_trust() {
        let server_identity = Identity::generate();
        let server_tls = TlsIdentity::generate("server").unwrap();
        let server_fingerprint = server_tls.fingerprint;
        let server = server_endpoint(&server_tls, loopback()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            let mut trust = TrustStore::default();
            let result = server_pair(
                &connection,
                &server_identity,
                &server_tls,
                &mut trust,
                &mut PairingLimiter::default(),
                |_| false,
            )
            .await;
            (trust, result.is_err())
        });

        let client_identity = Identity::generate();
        let client_tls = TlsIdentity::generate("client").unwrap();
        let client = client_endpoint(loopback(), &client_tls, server_fingerprint)
            .await
            .unwrap();
        let connection = connect(&client, addr).await.unwrap();
        let mut trust = TrustStore::default();
        let result = client_pair(
            &connection,
            &client_identity,
            &client_tls,
            &mut trust,
            |_| true,
        )
        .await;
        assert!(result.is_err());
        assert!(trust.is_empty());

        let (server_trust, server_rejected) = server_task.await.unwrap();
        assert!(server_rejected);
        assert!(server_trust.is_empty());
        connection.close(0u32.into(), b"rejected");
        client.wait_idle().await;
    }
}
