//! Network selection, authenticated QUIC transport, and discovery probes.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use fluxy_core::{Error, ErrorCode};

pub mod discovery;
pub mod fallback;
pub mod handshake;
pub mod interface;
pub mod session;
pub mod tls;
pub mod transfer;
pub mod transport;

pub use discovery::{
    DEFAULT_PEER_TTL, DiscoveryConfig, DiscoveryError, DiscoveryEvent, MdnsDiscovery,
    PeerAdvertisement, PeerRecord, PeerTable, SERVICE_TYPE,
};
pub use fallback::{
    FALLBACK_PEER_TTL, FALLBACK_PROBE_INTERVAL, FallbackConfig, FallbackDiagnostics,
    FallbackDiscovery, FallbackError, FallbackEvent, default_gateway,
};
pub use handshake::{
    HandshakeError, PairingLimiter, PairingPrompt, PeerSession, client_pair, client_pair_async,
    client_resume, server_pair, server_pair_async,
};
pub use interface::{
    AddressInfo, InterfaceBinding, InterfaceInfo, InterfaceKind, Selection, SelectionError,
    SelectionMode, SelectionSource, default_route_interface, display_order, enumerate,
    format_address, parse_address, select_auto, select_manual,
};
pub use session::{
    Backoff, ClipboardShareOffer, ControlSender, ControlSession, MAX_CLIPBOARD_BYTES,
    MAX_CLIPBOARD_HOP, PersistentSession, SessionConfig, SessionError, SessionEvent,
    TransferMessage, TransferOfferDraft, accept_session, connect_trusted,
    connect_trusted_with_retry,
};
pub use tls::{TlsIdentity, client_config, server_config};
pub use transfer::{
    CheckpointResume, TRANSFER_IO_TIMEOUT, TRANSFER_TICK, TransferConfig, TransferError,
    TransferOptions, manifest_from_proto, manifest_to_proto, prepare_sink, receive_file,
    receive_file_resumable, send_file,
};
pub use transport::{
    TransportError, accept, client_endpoint, connect, read_envelope, server_endpoint,
    server_endpoint_bound, write_envelope,
};

pub const DISCOVERY_PORT: u16 = 42420;
pub const DISCOVERY_MAGIC: &[u8] = b"FLUX-DISCOVER/1";

/// What the runtime should bind, after the persisted preference is combined with
/// the listening port and the development switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSelection {
    pub mode: SelectionMode,
    /// Interface the user picked. Only used in manual mode.
    pub interface_id: Option<String>,
    /// Address the user picked. Required in manual mode; `None` in auto mode
    /// means "choose for me".
    pub address: Option<IpAddr>,
    pub port: u16,
    pub dev_loopback: bool,
}

impl NetworkSelection {
    /// Automatic selection with no persisted preference.
    pub fn auto(port: u16, dev_loopback: bool) -> Self {
        Self {
            mode: SelectionMode::Auto,
            interface_id: None,
            address: None,
            port,
            dev_loopback,
        }
    }

    /// A concrete manual selection, used by the CLI and by the settings page.
    pub fn manual(interface_id: impl Into<String>, address: IpAddr, port: u16) -> Self {
        Self {
            mode: SelectionMode::Manual,
            interface_id: Some(interface_id.into()),
            address: Some(address),
            port,
            dev_loopback: address.is_loopback(),
        }
    }

    pub fn loopback(port: u16) -> Self {
        Self::manual("loopback", IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port)
    }

    /// The cheap checks that do not need the interface table. Whether the chosen
    /// interface and address still exist is decided at bind time, because only
    /// the runtime can enumerate them.
    pub fn validate(&self) -> Result<(), Error> {
        let address = match self.mode {
            SelectionMode::Auto => self.address,
            SelectionMode::Manual => {
                let address = self.address.ok_or_else(|| {
                    Error::new(
                        ErrorCode::NetworkUnavailable,
                        "manual selection needs an address",
                        false,
                    )
                })?;
                if self
                    .interface_id
                    .as_deref()
                    .map(|id| id.trim().is_empty())
                    .unwrap_or(true)
                {
                    return Err(Error::new(
                        ErrorCode::NetworkUnavailable,
                        "manual selection needs an interface",
                        false,
                    ));
                }
                Some(address)
            }
        };
        let Some(address) = address else {
            return Ok(());
        };
        if address.is_loopback() && !self.dev_loopback {
            return Err(Error::new(
                ErrorCode::NetworkUnavailable,
                "loopback requires --dev-loopback or dev.allowLoopback=true",
                false,
            ));
        }
        if address.is_unspecified() {
            return Err(Error::new(
                ErrorCode::NetworkUnavailable,
                "address must be concrete",
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryProbe {
    pub device_id: [u8; 16],
    pub nonce: [u8; 32],
}

impl DiscoveryProbe {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(DISCOVERY_MAGIC.len() + 16 + 32);
        out.extend_from_slice(DISCOVERY_MAGIC);
        out.extend_from_slice(&self.device_id);
        out.extend_from_slice(&self.nonce);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != DISCOVERY_MAGIC.len() + 48 || !bytes.starts_with(DISCOVERY_MAGIC) {
            return Err(Error::new(
                ErrorCode::Protocol,
                "invalid discovery probe",
                false,
            ));
        }
        let mut device_id = [0u8; 16];
        let mut nonce = [0u8; 32];
        device_id.copy_from_slice(&bytes[DISCOVERY_MAGIC.len()..DISCOVERY_MAGIC.len() + 16]);
        nonce.copy_from_slice(&bytes[DISCOVERY_MAGIC.len() + 16..]);
        Ok(Self { device_id, nonce })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_requires_explicit_dev_mode() {
        let mut selection = NetworkSelection::loopback(1234);
        selection.dev_loopback = false;
        assert!(selection.validate().is_err());
        selection.dev_loopback = true;
        assert!(selection.validate().is_ok());
    }

    #[test]
    fn probe_round_trips() {
        let probe = DiscoveryProbe {
            device_id: [1; 16],
            nonce: [2; 32],
        };
        assert_eq!(DiscoveryProbe::decode(&probe.encode()).unwrap(), probe);
        assert!(DiscoveryProbe::decode(b"bad").is_err());
    }
}
