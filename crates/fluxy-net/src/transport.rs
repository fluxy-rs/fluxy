use std::net::SocketAddr;
use std::time::Duration;

use quinn::{Connection, Endpoint, RecvStream, SendStream};
use socket2::{Domain, Protocol, Socket, Type};
use thiserror::Error;
use tokio::time::timeout;

use fluxy_core::{Error, ErrorCode};
use fluxy_proto::{Envelope, MAX_CONTROL_FRAME, decode_frame, encode_frame};

use crate::interface::{InterfaceBinding, bind_socket_to_interface};
use crate::tls::{SERVER_NAME, TlsIdentity, client_config, server_config};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("endpoint setup failed: {0}")]
    Endpoint(String),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("connection timed out")]
    Timeout,
    #[error("control stream failed: {0}")]
    Stream(String),
    #[error("protocol error: {0}")]
    Protocol(#[from] fluxy_proto::ProtoError),
}

impl From<TransportError> for Error {
    fn from(value: TransportError) -> Self {
        Error::new(ErrorCode::Protocol, value.to_string(), true)
    }
}

/// Requested UDP socket buffer, in bytes. Loops and fast LANs need room for a
/// window's worth of datagrams.
const UDP_SOCKET_BUFFER_BYTES: usize = 8 * 1024 * 1024;

pub async fn server_endpoint(
    identity: &TlsIdentity,
    addr: SocketAddr,
) -> Result<Endpoint, TransportError> {
    let (endpoint, _pinned) = server_endpoint_bound(identity, addr, None).await?;
    Ok(endpoint)
}
/// A server endpoint that is pinned to the chosen interface when the platform
/// supports it.
///
/// The second value says whether the pin was applied; a platform without the
/// option still gets a working listener, and the runtime reports that routing
/// may still choose another path.
pub async fn server_endpoint_bound(
    identity: &TlsIdentity,
    addr: SocketAddr,
    binding: Option<&InterfaceBinding>,
) -> Result<(Endpoint, bool), TransportError> {
    let config = server_config(identity).map_err(|e| TransportError::Endpoint(e.to_string()))?;
    let (socket, pinned) = bound_udp_socket(addr, binding)?;
    let runtime = quinn::default_runtime()
        .ok_or_else(|| TransportError::Endpoint("no async runtime available".into()))?;
    let endpoint = Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(config),
        socket,
        runtime,
    )
    .map_err(|e| TransportError::Endpoint(e.to_string()))?;
    Ok((endpoint, pinned))
}

/// Create a UDP socket, bind it to `addr`, and pin it to the chosen interface.
fn bound_udp_socket(
    addr: SocketAddr,
    binding: Option<&InterfaceBinding>,
) -> Result<(std::net::UdpSocket, bool), TransportError> {
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| TransportError::Endpoint(e.to_string()))?;
    let pinned = match binding {
        Some(binding) => match bind_socket_to_interface(&socket, binding) {
            Ok(pinned) => pinned,
            Err(error) => {
                tracing::warn!(
                    interface = %binding.name,
                    index = binding.index,
                    %error,
                    "unable to pin the socket to the chosen interface"
                );
                false
            }
        },
        None => false,
    };
    // QUIC runs over a datagram socket, and the OS default (64 KiB on
    // Windows) is far smaller than the in-flight window the transport
    // advertises. A small buffer makes the kernel drop datagrams, which QUIC
    // reads as congestion and answers by halving the rate, so the socket is
    // sized to hold a burst instead. The OS may clamp it; that is fine.
    let buffer = UDP_SOCKET_BUFFER_BYTES;
    if let Err(error) = socket.set_recv_buffer_size(buffer) {
        tracing::debug!(%error, "unable to grow the UDP receive buffer");
    }
    if let Err(error) = socket.set_send_buffer_size(buffer) {
        tracing::debug!(%error, "unable to grow the UDP send buffer");
    }
    socket
        .bind(&addr.into())
        .map_err(|e| TransportError::Endpoint(e.to_string()))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| TransportError::Endpoint(e.to_string()))?;
    Ok((socket.into(), pinned))
}

pub async fn client_endpoint(
    local: SocketAddr,
    identity: &TlsIdentity,
    expected_fingerprint: [u8; 32],
) -> Result<Endpoint, TransportError> {
    let config = client_config(identity, expected_fingerprint)
        .map_err(|e| TransportError::Endpoint(e.to_string()))?;
    let mut endpoint =
        Endpoint::client(local).map_err(|e| TransportError::Endpoint(e.to_string()))?;
    endpoint.set_default_client_config(config);
    Ok(endpoint)
}

pub async fn connect(
    endpoint: &Endpoint,
    remote: SocketAddr,
) -> Result<Connection, TransportError> {
    let connecting = endpoint
        .connect(remote, SERVER_NAME)
        .map_err(|e| TransportError::Connect(e.to_string()))?;
    timeout(Duration::from_secs(10), connecting)
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|e| TransportError::Connect(e.to_string()))
}

pub async fn accept(endpoint: &Endpoint) -> Result<Connection, TransportError> {
    // Two waits with very different causes: endpoint.accept() is "the QUIC
    // driver has not surfaced an Incoming yet", the timeout is "the handshake
    // is stalled". Timing them apart is the only way to tell which one a slow
    // inbound connection is stuck in.
    let waiting = std::time::Instant::now();
    let incoming = endpoint
        .accept()
        .await
        .ok_or_else(|| TransportError::Endpoint("endpoint closed".into()))?;
    let queued_ms = waiting.elapsed().as_millis() as u64;
    tracing::info!(queued_ms, remote = %incoming.remote_address(), "quic incoming surfaced");
    let handshaking = std::time::Instant::now();
    let outcome = timeout(Duration::from_secs(10), incoming)
        .await
        .map_err(|_| TransportError::Timeout)?;
    tracing::info!(
        handshake_ms = handshaking.elapsed().as_millis() as u64,
        "quic handshake resolved"
    );
    outcome.map_err(|e| TransportError::Connect(e.to_string()))
}

pub async fn write_envelope(
    send: &mut SendStream,
    envelope: &Envelope,
) -> Result<(), TransportError> {
    let frame = encode_frame(envelope)?;
    send.write_all(&frame)
        .await
        .map_err(|e| TransportError::Stream(e.to_string()))
}

pub async fn read_envelope(recv: &mut RecvStream) -> Result<Envelope, TransportError> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|e| TransportError::Stream(e.to_string()))?;
    let len = u32::from_be_bytes(prefix) as usize;
    if len > MAX_CONTROL_FRAME {
        return Err(TransportError::Protocol(
            fluxy_proto::ProtoError::FrameTooLarge(len, MAX_CONTROL_FRAME),
        ));
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body)
        .await
        .map_err(|e| TransportError::Stream(e.to_string()))?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&prefix);
    frame.extend_from_slice(&body);
    decode_frame(&frame).map_err(TransportError::from)
}

pub async fn echo_once(connection: &Connection) -> Result<(), TransportError> {
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|e| TransportError::Stream(e.to_string()))?;
    let envelope = read_envelope(&mut recv).await?;
    write_envelope(&mut send, &envelope).await?;
    send.finish()
        .map_err(|e| TransportError::Stream(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxy_proto::{MessageKind, PROTOCOL_VERSION};
    use std::net::{IpAddr, Ipv4Addr};

    fn envelope() -> Envelope {
        let mut message_id = vec![7u8; 16];
        message_id[6] = 0x70;
        Envelope {
            protocol_version: PROTOCOL_VERSION,
            message_id,
            source_device_id: vec![3u8; 16],
            session_id: vec![0u8; 16],
            created_at_ms: 1,
            kind: MessageKind::Ping as i32,
            payload: b"ping".to_vec(),
        }
    }

    #[tokio::test]
    async fn pinned_loopback_echo_round_trips() {
        let server_identity = TlsIdentity::generate("server").unwrap();
        let server = server_endpoint(
            &server_identity,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        )
        .await
        .unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = accept(&server).await.unwrap();
            echo_once(&connection).await.unwrap();
            connection.closed().await;
        });

        let client_identity = TlsIdentity::generate("client").unwrap();
        let client = client_endpoint(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &client_identity,
            server_identity.fingerprint,
        )
        .await
        .unwrap();
        let connection = connect(&client, addr).await.unwrap();
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        let sent = envelope();
        write_envelope(&mut send, &sent).await.unwrap();
        let received = read_envelope(&mut recv).await.unwrap();
        assert_eq!(received, sent);
        send.finish().unwrap();
        connection.close(0u32.into(), b"done");
        client.wait_idle().await;
        server_task.abort();
    }

    #[tokio::test]
    async fn wrong_fingerprint_is_rejected() {
        let server_identity = TlsIdentity::generate("server").unwrap();
        let server = server_endpoint(
            &server_identity,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        )
        .await
        .unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = accept(&server).await;
        });

        let client_identity = TlsIdentity::generate("client").unwrap();
        let client = client_endpoint(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &client_identity,
            [0u8; 32],
        )
        .await
        .unwrap();
        let result = connect(&client, addr).await;
        assert!(result.is_err());
        client.wait_idle().await;
        server_task.abort();
    }
}
