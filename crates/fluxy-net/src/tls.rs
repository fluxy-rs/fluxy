use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, ServerConfig, TransportConfig, VarInt};
use rustls::DistinguishedName;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use thiserror::Error;

use fluxy_core::{Error, ErrorCode};

pub const ALPN: &[u8] = b"fluxy/1";
pub const SERVER_NAME: &str = "fluxy.local";

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("certificate generation failed: {0}")]
    Generate(String),
    #[error("TLS configuration failed: {0}")]
    Config(String),
    #[error("certificate fingerprint mismatch")]
    FingerprintMismatch,
    #[error("certificate storage failed: {0}")]
    Io(#[from] std::io::Error),
}

impl From<TlsError> for Error {
    fn from(value: TlsError) -> Self {
        Error::new(ErrorCode::Identity, value.to_string(), false)
    }
}

#[derive(Clone)]
pub struct TlsIdentity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub fingerprint: [u8; 32],
}

impl std::fmt::Debug for TlsIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsIdentity")
            .field("cert_len", &self.cert_der.len())
            .field("fingerprint", &hex_prefix(&self.fingerprint))
            .finish()
    }
}

impl TlsIdentity {
    /// Load a stable self-signed certificate/key pair, creating it once.
    pub fn load_or_create(
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
        name: &str,
    ) -> Result<Self, TlsError> {
        let cert_exists = cert_path.exists();
        let key_exists = key_path.exists();
        if cert_exists && key_exists {
            let cert_der = std::fs::read(cert_path)?;
            let key_der = std::fs::read(key_path)?;
            return Self::from_der(cert_der, key_der);
        }
        if cert_exists || key_exists {
            return Err(TlsError::Generate(
                "certificate and key must exist together".into(),
            ));
        }

        let identity = Self::generate(name)?;
        if let Some(parent) = cert_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(cert_path, &identity.cert_der)?;
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(key_path)?;
            file.write_all(&identity.key_der)?;
        }
        #[cfg(not(unix))]
        std::fs::write(key_path, &identity.key_der)?;
        Ok(identity)
    }

    pub fn from_der(cert_der: Vec<u8>, key_der: Vec<u8>) -> Result<Self, TlsError> {
        if cert_der.is_empty() || key_der.is_empty() {
            return Err(TlsError::Generate(
                "certificate and key must not be empty".into(),
            ));
        }
        let fingerprint = *blake3::hash(&cert_der).as_bytes();
        Ok(Self {
            cert_der,
            key_der,
            fingerprint,
        })
    }

    pub fn generate(name: &str) -> Result<Self, TlsError> {
        let certified =
            rcgen::generate_simple_self_signed(vec![name.to_string(), SERVER_NAME.to_string()])
                .map_err(|e| TlsError::Generate(e.to_string()))?;
        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.signing_key.serialize_der();
        let fingerprint = *blake3::hash(&cert_der).as_bytes();
        Ok(Self {
            cert_der,
            key_der,
            fingerprint,
        })
    }

    pub fn cert(&self) -> CertificateDer<'static> {
        CertificateDer::from(self.cert_der.clone())
    }

    pub fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone()))
    }
}

pub fn server_config(identity: &TlsIdentity) -> Result<ServerConfig, TlsError> {
    let provider = rustls::crypto::ring::default_provider();
    let algs = provider.signature_verification_algorithms;
    let client_verifier = Arc::new(AnyClientCertVerifier {
        algs,
        schemes: algs.supported_schemes(),
    });
    let mut crypto = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(vec![identity.cert()], identity.private_key())
        .map_err(|e| TlsError::Config(e.to_string()))?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let quic = QuicServerConfig::try_from(crypto).map_err(|e| TlsError::Config(e.to_string()))?;
    let mut config = ServerConfig::with_crypto(Arc::new(quic));
    config.transport_config(Arc::new(transport_config()));
    Ok(config)
}

pub fn client_config(
    identity: &TlsIdentity,
    expected_fingerprint: [u8; 32],
) -> Result<ClientConfig, TlsError> {
    let provider = rustls::crypto::ring::default_provider();
    let algs = provider.signature_verification_algorithms;
    let verifier = Arc::new(PinnedVerifier {
        expected: expected_fingerprint,
        algs,
        schemes: algs.supported_schemes(),
    });
    let mut crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![identity.cert()], identity.private_key())
        .map_err(|e| TlsError::Config(e.to_string()))?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let quic = QuicClientConfig::try_from(crypto).map_err(|e| TlsError::Config(e.to_string()))?;
    let mut config = ClientConfig::new(Arc::new(quic));
    config.transport_config(Arc::new(transport_config()));
    Ok(config)
}

fn transport_config() -> TransportConfig {
    let mut config = TransportConfig::default();
    // The windows have to cover the whole in-flight window of a fast LAN
    // transfer: four chunk streams with two 4 MiB chunks each is 32 MiB, and a
    // window smaller than that stalls the sender at every acknowledgement.
    config
        .max_concurrent_bidi_streams(VarInt::from_u32(16))
        .max_concurrent_uni_streams(VarInt::from_u32(32))
        .stream_receive_window(VarInt::from_u32(16 * 1024 * 1024))
        .receive_window(VarInt::from_u32(128 * 1024 * 1024))
        // Loopback and most LANs carry 1500-byte frames; 1452 keeps the packet
        // under that once QUIC and UDP headers are counted. Path MTU discovery
        // raises or lowers this once the handshake completes - measured on
        // loopback, a jumbo start (9000) added only ~5% because the limit is the
        // socket's packet rate, not the packet size.
        .initial_mtu(1452)
        // Cubic backs off hard on a lossy or briefly busy Wi-Fi link and then
        // creeps back up; on a LAN the link is fast and mostly idle, which is
        // exactly the case BBR is designed for.
        .congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()))
        .keep_alive_interval(Some(Duration::from_secs(10)));
    config
}

#[derive(Debug)]
struct PinnedVerifier {
    expected: [u8; 32],
    algs: WebPkiSupportedAlgorithms,
    schemes: Vec<SignatureScheme>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let actual = *blake3::hash(end_entity.as_ref()).as_bytes();
        if actual == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(RustlsError::General(
                "fluxy certificate fingerprint mismatch".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes.clone()
    }
}

/// Bootstrap client-certificate verifier. It validates the handshake signature
/// but does not pin the client certificate; the application Hello/SAS exchange
/// authenticates the peer and writes the pin into the trust store.
#[derive(Debug)]
struct AnyClientCertVerifier {
    algs: WebPkiSupportedAlgorithms,
    schemes: Vec<SignatureScheme>,
}

impl ClientCertVerifier for AnyClientCertVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        if end_entity.is_empty() {
            return Err(RustlsError::General("empty client certificate".into()));
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes.clone()
    }
}

fn hex_prefix(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_certificate_has_stable_fingerprint() {
        let identity = TlsIdentity::generate("test").unwrap();
        assert_eq!(
            identity.fingerprint,
            *blake3::hash(&identity.cert_der).as_bytes()
        );
        assert!(!identity.key_der.is_empty());
    }

    #[test]
    fn persisted_identity_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("tls-cert.der");
        let key = dir.path().join("tls-key.der");
        let first = TlsIdentity::load_or_create(&cert, &key, "test").unwrap();
        let second = TlsIdentity::load_or_create(&cert, &key, "test").unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.cert_der, second.cert_der);
        assert_eq!(first.key_der, second.key_der);
    }

    #[test]
    fn pinned_verifier_accepts_only_matching_certificate() {
        let identity = TlsIdentity::generate("test").unwrap();
        let provider = rustls::crypto::ring::default_provider();
        let algs = provider.signature_verification_algorithms;
        let verifier = PinnedVerifier {
            expected: identity.fingerprint,
            algs,
            schemes: algs.supported_schemes(),
        };
        let name = ServerName::try_from(SERVER_NAME).unwrap();
        let now = UnixTime::now();
        assert!(
            verifier
                .verify_server_cert(&identity.cert(), &[], &name, &[], now)
                .is_ok()
        );

        let wrong = PinnedVerifier {
            expected: [0u8; 32],
            algs,
            schemes: algs.supported_schemes(),
        };
        assert!(
            wrong
                .verify_server_cert(&identity.cert(), &[], &name, &[], now)
                .is_err()
        );
    }
}
