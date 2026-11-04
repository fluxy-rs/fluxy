use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, ErrorCode};

const TRUST_VERSION: u32 = 1;

/// What a trusted peer may do when it offers clipboard text. `Ask` is the
/// default: automatic writes require an explicit per-device opt-in.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardPolicy {
    #[default]
    Ask,
    Auto,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustEntry {
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    pub public_key: [u8; 32],
    pub cert_fingerprint: [u8; 32],
    pub paired_at_ms: i64,
    #[serde(default)]
    pub clipboard_policy: ClipboardPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    version: u32,
    entries: BTreeMap<Uuid, TrustEntry>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            version: TRUST_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

impl TrustStore {
    pub fn load_or_default(path: &Path) -> Result<Self, Error> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path)?;
        let store: Self = serde_json::from_slice(&bytes)?;
        if store.version != TRUST_VERSION {
            return Err(Error::new(
                ErrorCode::Trust,
                "unsupported trust store version",
                false,
            ));
        }
        Ok(store)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut tmp = PathBuf::from(path);
        tmp.set_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn insert(&mut self, entry: TrustEntry) {
        self.entries.insert(entry.device_id, entry);
    }

    /// Update the display name of a trusted peer, reporting whether it changed.
    ///
    /// Only the label moves; the trust material (public key, certificate
    /// fingerprint) is never touched by an observation from the network.
    pub fn rename(&mut self, device_id: &Uuid, name: &str) -> bool {
        let Some(entry) = self.entries.get_mut(device_id) else {
            return false;
        };
        if entry.name == name || name.trim().is_empty() {
            return false;
        }
        entry.name = name.to_string();
        true
    }

    pub fn get(&self, device_id: &Uuid) -> Option<&TrustEntry> {
        self.entries.get(device_id)
    }

    pub fn remove(&mut self, device_id: &Uuid) -> bool {
        self.entries.remove(device_id).is_some()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn is_trusted(&self, device_id: &Uuid) -> bool {
        self.entries.contains_key(device_id)
    }

    pub fn clipboard_policy(&self, device_id: &Uuid) -> Option<ClipboardPolicy> {
        self.entries
            .get(device_id)
            .map(|entry| entry.clipboard_policy)
    }

    /// Change the policy of an existing trust entry. Returns the previous
    /// policy, or `None` when the device is not trusted.
    pub fn set_clipboard_policy(
        &mut self,
        device_id: &Uuid,
        policy: ClipboardPolicy,
    ) -> Option<ClipboardPolicy> {
        self.entries
            .get_mut(device_id)
            .map(|entry| std::mem::replace(&mut entry.clipboard_policy, policy))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TrustEntry> {
        self.entries.values()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> TrustEntry {
        TrustEntry {
            device_id: Uuid::now_v7(),
            name: "peer".into(),
            platform: "linux".into(),
            public_key: [1; 32],
            cert_fingerprint: [2; 32],
            paired_at_ms: 1,
            clipboard_policy: ClipboardPolicy::Ask,
        }
    }

    #[test]
    fn persists_and_removes_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.json");
        let mut store = TrustStore::load_or_default(&path).unwrap();
        let item = entry();
        let id = item.device_id;
        store.insert(item);
        assert_eq!(
            store.set_clipboard_policy(&id, ClipboardPolicy::Auto),
            Some(ClipboardPolicy::Ask)
        );
        store.save(&path).unwrap();

        let mut loaded = TrustStore::load_or_default(&path).unwrap();
        assert!(loaded.is_trusted(&id));
        assert_eq!(loaded.clipboard_policy(&id), Some(ClipboardPolicy::Auto));
        assert_eq!(loaded.len(), 1);
        assert!(loaded.remove(&id));
        assert!(!loaded.is_trusted(&id));
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = TrustStore::load_or_default(&dir.path().join("none.json")).unwrap();
        assert!(store.is_empty());
    }
}
