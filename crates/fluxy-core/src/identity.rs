use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use getrandom::SysRng;
use rand_core::UnwrapErr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, ErrorCode};

const IDENTITY_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct StoredIdentity {
    version: u32,
    device_id: Uuid,
    secret_key: [u8; 32],
    created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Identity {
    device_id: Uuid,
    signing_key: SigningKey,
    created_at_ms: i64,
}

impl Identity {
    pub fn generate() -> Self {
        Self::generate_with(Uuid::now_v7())
    }

    fn generate_with(device_id: Uuid) -> Self {
        let mut csprng = UnwrapErr(SysRng);
        let signing_key = SigningKey::generate(&mut csprng);
        Self {
            device_id,
            signing_key,
            created_at_ms: now_ms(),
        }
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            device_id: Uuid::now_v7(),
            signing_key: SigningKey::from_bytes(&seed),
            created_at_ms: now_ms(),
        }
    }

    pub fn device_id(&self) -> Uuid {
        self.device_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    pub fn verify(
        public_key: &[u8; 32],
        message: &[u8],
        signature: &Signature,
    ) -> Result<(), Error> {
        let key = VerifyingKey::from_bytes(public_key).map_err(|e| {
            Error::new(
                ErrorCode::Identity,
                format!("invalid public key: {e}"),
                false,
            )
        })?;
        key.verify(message, signature).map_err(|e| {
            Error::new(
                ErrorCode::Identity,
                format!("signature verify failed: {e}"),
                false,
            )
        })
    }

    pub fn verify_bytes(
        public_key: &[u8; 32],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), Error> {
        let signature = Signature::from_slice(signature).map_err(|e| {
            Error::new(
                ErrorCode::Identity,
                format!("invalid signature encoding: {e}"),
                false,
            )
        })?;
        Self::verify(public_key, message, &signature)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let stored = StoredIdentity {
            version: IDENTITY_VERSION,
            device_id: self.device_id,
            secret_key: self.signing_key.to_bytes(),
            created_at_ms: self.created_at_ms,
        };
        write_atomic(path, &serde_json::to_vec_pretty(&stored)?)
    }
}

pub struct IdentityStore;

impl IdentityStore {
    /// Load an existing identity or create and persist a new one.
    pub fn load_or_create(path: &Path) -> Result<Identity, Error> {
        if path.exists() {
            return Self::load(path);
        }
        let identity = Identity::generate();
        identity.save(path)?;
        Ok(identity)
    }

    pub fn load(path: &Path) -> Result<Identity, Error> {
        let bytes = fs::read(path).map_err(|e| {
            Error::new(
                ErrorCode::Identity,
                format!("read {}: {e}", path.display()),
                false,
            )
        })?;
        let stored: StoredIdentity = serde_json::from_slice(&bytes)?;
        if stored.version != IDENTITY_VERSION {
            return Err(Error::new(
                ErrorCode::Identity,
                "unsupported identity version",
                false,
            ));
        }
        let signing_key = SigningKey::from_bytes(&stored.secret_key);
        Ok(Identity {
            device_id: stored.device_id,
            signing_key,
            created_at_ms: stored.created_at_ms,
        })
    }

    /// Rotate the key and device id. Existing trust entries intentionally no longer match.
    pub fn reset(path: &Path) -> Result<Identity, Error> {
        let identity = Identity::generate();
        identity.save(path)?;
        Ok(identity)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = PathBuf::from(path);
    tmp.set_extension("tmp");
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct SasInput {
    pub device_id: Uuid,
    pub nonce: [u8; 32],
    pub cert_fingerprint: [u8; 32],
}

/// SAS = BLAKE3(sorted (device_id, nonce, cert_fp)) first 6 decimal digits.
pub fn compute_sas(a: &SasInput, b: &SasInput) -> Result<String, Error> {
    if a.device_id == b.device_id {
        return Err(Error::new(
            ErrorCode::Identity,
            "SAS requires two distinct device ids",
            false,
        ));
    }
    let (first, second) = if a.device_id < b.device_id {
        (a, b)
    } else {
        (b, a)
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"fluxy-sas-v1");
    hasher.update(first.device_id.as_bytes());
    hasher.update(&first.nonce);
    hasher.update(&first.cert_fingerprint);
    hasher.update(second.device_id.as_bytes());
    hasher.update(&second.nonce);
    hasher.update(&second.cert_fingerprint);
    let digest = hasher.finalize();
    let value =
        u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("8-byte prefix")) % 1_000_000;
    Ok(format!("{value:06}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips_and_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        let first = IdentityStore::load_or_create(&path).unwrap();
        let second = IdentityStore::load_or_create(&path).unwrap();
        assert_eq!(first.device_id(), second.device_id());
        assert_eq!(first.public_key(), second.public_key());
        assert!(path.exists());
    }

    #[test]
    fn signature_verifies() {
        let identity = Identity::generate();
        let message = b"hello";
        let sig = identity.sign(message);
        Identity::verify(&identity.public_key(), message, &sig).unwrap();
        assert!(Identity::verify(&identity.public_key(), b"other", &sig).is_err());
    }

    #[test]
    fn reset_rotates_device_id_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        let first = IdentityStore::load_or_create(&path).unwrap();
        let second = IdentityStore::reset(&path).unwrap();
        assert_ne!(first.device_id(), second.device_id());
        assert_ne!(first.public_key(), second.public_key());
    }

    #[test]
    fn sas_is_stable_and_six_digits() {
        let a = SasInput {
            device_id: Uuid::now_v7(),
            nonce: [1; 32],
            cert_fingerprint: [2; 32],
        };
        let b = SasInput {
            device_id: Uuid::now_v7(),
            nonce: [3; 32],
            cert_fingerprint: [4; 32],
        };
        let one = compute_sas(&a, &b).unwrap();
        let two = compute_sas(&b, &a).unwrap();
        assert_eq!(one, two);
        assert_eq!(one.len(), 6);
        assert!(one.chars().all(|c| c.is_ascii_digit()));
    }
}
