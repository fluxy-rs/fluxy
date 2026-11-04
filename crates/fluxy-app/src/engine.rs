use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use fluxy_core::{
    AutoAcceptPolicy, ClipboardPolicy, Error, Identity, IdentityStore, TrustEntry, TrustStore,
};
use fluxy_net::TlsIdentity;

use crate::config::{AppConfig, AppPaths};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub version: String,
    pub protocol_version: u32,
    pub device_id: String,
    pub device_name: String,
    pub data_dir: String,
    pub listen_port: u16,
    pub trusted_device_count: usize,
    pub dev_loopback: bool,
    /// SHA-256 of this device's certificate as 64 lowercase hex characters.
    /// A pairing QR code carries it so the scanning device can pin the peer
    /// without waiting for discovery to see it.
    pub identity_fingerprint: String,
}

pub struct Engine {
    identity: Identity,
    trust: TrustStore,
    tls: TlsIdentity,
    config: AppConfig,
    paths: AppPaths,
    /// The port the listener actually bound. The configured value can be 0,
    /// which means "let the OS choose", so the status has to report the real
    /// one once the network runtime knows it.
    bound_port: std::sync::atomic::AtomicU16,
}

impl Engine {
    pub fn load(paths: AppPaths) -> Result<Self, Error> {
        paths.ensure()?;
        let config = AppConfig::load_or_default(&paths.config_file)?;
        if !paths.config_file.exists() {
            config.save(&paths.config_file)?;
        }
        let identity = IdentityStore::load_or_create(&paths.identity_file)?;
        let tls = TlsIdentity::load_or_create(
            &paths.tls_cert_file,
            &paths.tls_key_file,
            "fluxy-desktop",
        )?;
        let trust = TrustStore::load_or_default(&paths.trust_file)?;
        tracing::info!(
            device_id = %identity.device_id(),
            version = env!("CARGO_PKG_VERSION"),
            data_dir = %paths.data_dir.display(),
            "engine loaded"
        );
        Ok(Self {
            identity,
            trust,
            tls,
            config,
            paths,
            bound_port: std::sync::atomic::AtomicU16::new(0),
        })
    }

    pub fn status(&self) -> EngineStatus {
        EngineStatus {
            version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: fluxy_proto::PROTOCOL_VERSION,
            device_id: self.identity.device_id().to_string(),
            device_name: self.config.device_name.clone(),
            data_dir: self.paths.data_dir.display().to_string(),
            listen_port: match self.bound_port.load(std::sync::atomic::Ordering::Relaxed) {
                0 => self.config.listen_port,
                bound => bound,
            },
            trusted_device_count: self.trust.len(),
            dev_loopback: self.config.dev_allow_loopback,
            identity_fingerprint: self
                .tls
                .fingerprint
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        }
    }

    /// Record the port the listener ended up on. Called by the network runtime
    /// after it binds, so the UI can show an address a peer can dial.
    pub fn set_bound_port(&mut self, port: u16) {
        self.bound_port
            .store(port, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn tls(&self) -> &TlsIdentity {
        &self.tls
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn trust_snapshot(&self) -> TrustStore {
        self.trust.clone()
    }

    pub fn trusted_devices(&self) -> Vec<TrustEntry> {
        self.trust.iter().cloned().collect()
    }

    /// Persist a newly paired peer. The caller must have completed SAS
    /// confirmation; this method does not create trust on its own.
    pub fn insert_trust(&mut self, entry: TrustEntry) -> Result<(), Error> {
        self.trust.insert(entry);
        self.trust.save(&self.paths.trust_file)
    }

    pub fn clipboard_policy(&self, device_id: &uuid::Uuid) -> Option<ClipboardPolicy> {
        self.trust.clipboard_policy(device_id)
    }

    pub fn auto_accept_policy(&self, device_id: &uuid::Uuid) -> Option<AutoAcceptPolicy> {
        self.trust.auto_accept_policy(device_id)
    }

    pub fn set_auto_accept_policy(
        &mut self,
        device_id: &uuid::Uuid,
        policy: AutoAcceptPolicy,
    ) -> Result<Option<AutoAcceptPolicy>, Error> {
        let previous = self.trust.set_auto_accept_policy(device_id, policy);
        if previous.is_some() {
            self.trust.save(&self.paths.trust_file)?;
        }
        Ok(previous)
    }

    pub fn set_clipboard_policy(
        &mut self,
        device_id: &uuid::Uuid,
        policy: ClipboardPolicy,
    ) -> Result<Option<ClipboardPolicy>, Error> {
        let previous = self.trust.set_clipboard_policy(device_id, policy);
        if previous.is_some() {
            self.trust.save(&self.paths.trust_file)?;
        }
        Ok(previous)
    }

    /// Adopt names discovery reports for already-trusted peers.
    ///
    /// The trust file records the name used at pairing time; a device that is
    /// renamed later would otherwise keep its old label forever, including
    /// while it is offline.
    pub fn sync_peer_names<'a, I>(&mut self, names: I) -> Result<bool, Error>
    where
        I: IntoIterator<Item = (&'a uuid::Uuid, &'a str)>,
    {
        let mut changed = false;
        for (device_id, name) in names {
            changed |= self.trust.rename(device_id, name);
        }
        if changed {
            self.trust.save(&self.paths.trust_file)?;
        }
        Ok(changed)
    }

    pub fn remove_trust(&mut self, device_id: &uuid::Uuid) -> Result<bool, Error> {
        let removed = self.trust.remove(device_id);
        if removed {
            self.trust.save(&self.paths.trust_file)?;
        }
        Ok(removed)
    }

    /// Persist a new active-transfer cap and return the updated config.
    pub fn set_max_active_transfers(&mut self, limit: u32) -> Result<AppConfig, Error> {
        self.config.max_active_transfers = crate::config::clamp_active_transfers(limit);
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// Persist a sanitized device name. The caller decides whether the LAN
    /// advertisement has to be restarted; the name itself is always bounded.
    pub fn set_device_name(&mut self, name: &str) -> Result<AppConfig, Error> {
        self.config.device_name = crate::config::sanitize_device_name(name);
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// Persist the desktop-notification preference.
    pub fn set_notifications(&mut self, enabled: bool) -> Result<AppConfig, Error> {
        self.config.notifications = enabled;
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// Persist the automatic clipboard-send switch. It is off by default and
    /// Persist whether incoming transfers are accepted without asking. Off by
    /// default, and read per offer rather than at start-up.
    pub fn set_auto_accept_transfers(&mut self, enabled: bool) -> Result<AppConfig, Error> {
        self.config.auto_accept_transfers = enabled;
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// only turns on when the user asks for it; the watcher reads it per tick.
    pub fn set_auto_clipboard(&mut self, enabled: bool) -> Result<AppConfig, Error> {
        self.config.auto_clipboard = enabled;
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// Persist the autostart preference. The login item itself is written by
    /// the shell, which owns the platform backend.
    pub fn set_autostart(&mut self, enabled: bool) -> Result<AppConfig, Error> {
        self.config.autostart = enabled;
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// Persist where received files land. The runtime picks the new directory
    /// up for the next transfer; transfers already writing keep their file.
    pub fn set_receive_dir(&mut self, directory: Option<PathBuf>) -> Result<AppConfig, Error> {
        self.config.set_receive_dir(directory)?;
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    /// The directory received files land in: the configured one, or the
    /// default under the data directory.
    pub fn receive_dir(&self) -> PathBuf {
        self.config
            .receive_dir
            .clone()
            .unwrap_or_else(|| self.paths.data_dir.join("received"))
    }

    /// What the network runtime should bind right now.
    pub fn network_selection(&self) -> Result<fluxy_net::NetworkSelection, Error> {
        self.config
            .network
            .to_selection(self.config.listen_port, self.config.dev_allow_loopback)
    }

    /// Persist the interface selection. Only the cheap checks happen here: a
    /// selection that cannot even be parsed is rejected, while "that interface is
    /// gone" is decided at bind time by the runtime, which answers with 2003.
    pub fn set_network_selection(
        &mut self,
        preference: crate::config::NetworkPreference,
    ) -> Result<AppConfig, Error> {
        preference.to_selection(self.config.listen_port, self.config.dev_allow_loopback)?;
        self.config.network = preference;
        self.config.save(&self.paths.config_file)?;
        Ok(self.config.clone())
    }

    pub fn rotate_identity(&mut self) -> Result<(), Error> {
        let identity = IdentityStore::reset(&self.paths.identity_file)?;
        self.identity = identity;
        self.trust.clear();
        self.trust.save(&self.paths.trust_file)?;
        tracing::warn!("identity rotated; all previous trust entries invalidated");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxy_core::TrustEntry;
    use uuid::Uuid;

    #[test]
    fn load_persists_identity_and_config() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        let first = Engine::load(paths.clone()).unwrap();
        let device_id = first.identity().device_id();
        let second = Engine::load(paths.clone()).unwrap();
        assert_eq!(second.identity().device_id(), device_id);
        assert!(paths.config_file.exists());
        assert!(paths.identity_file.exists());
        assert_eq!(second.status().version, "0.2.0");
    }

    #[test]
    fn the_transfer_cap_persists_and_is_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        let mut engine = Engine::load(paths.clone()).unwrap();
        assert_eq!(
            engine.config().max_active_transfers,
            crate::config::DEFAULT_ACTIVE_TRANSFERS
        );

        let updated = engine.set_max_active_transfers(5).unwrap();
        assert_eq!(updated.max_active_transfers, 5);
        assert_eq!(
            AppConfig::load_or_default(&paths.config_file)
                .unwrap()
                .max_active_transfers,
            5
        );

        let clamped = engine.set_max_active_transfers(0).unwrap();
        assert_eq!(
            clamped.max_active_transfers,
            crate::config::MIN_ACTIVE_TRANSFERS
        );
    }

    #[test]
    fn a_device_name_and_the_notification_preference_persist() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        let mut engine = Engine::load(paths.clone()).unwrap();
        assert!(engine.config().notifications);

        let renamed = engine.set_device_name("  Desk\tTop  ").unwrap();
        assert_eq!(renamed.device_name, "DeskTop");
        let muted = engine.set_notifications(false).unwrap();
        assert!(!muted.notifications);
        // Automatic clipboard sending is off until the user asks for it, and so
        // is the login item.
        assert!(!engine.config().auto_clipboard);
        assert!(engine.set_auto_clipboard(true).unwrap().auto_clipboard);
        assert!(!engine.config().autostart);
        assert!(engine.set_autostart(true).unwrap().autostart);

        let reloaded = AppConfig::load_or_default(&paths.config_file).unwrap();
        assert_eq!(reloaded.device_name, "DeskTop");
        assert!(!reloaded.notifications);
        assert!(reloaded.auto_clipboard);
        assert!(reloaded.autostart);
    }

    #[test]
    fn the_interface_selection_persists_and_rejects_a_bad_manual_pick() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        let mut engine = Engine::load(paths.clone()).unwrap();
        assert!(
            engine.network_selection().unwrap().mode == fluxy_net::interface::SelectionMode::Auto
        );

        let preference = crate::config::NetworkPreference::manual("Wi-Fi", "192.168.1.20");
        engine.set_network_selection(preference.clone()).unwrap();
        let reloaded = AppConfig::load_or_default(&paths.config_file).unwrap();
        assert_eq!(reloaded.network, preference);
        let selection = engine.network_selection().unwrap();
        assert_eq!(selection.mode, fluxy_net::interface::SelectionMode::Manual);
        assert_eq!(selection.address, Some("192.168.1.20".parse().unwrap()));
        assert_eq!(selection.interface_id.as_deref(), Some("Wi-Fi"));

        // A manual pick that cannot even be parsed is refused with 2003 and the
        // stored preference stays as it was.
        let error = engine
            .set_network_selection(crate::config::NetworkPreference::manual("Wi-Fi", "nope"))
            .expect_err("an invalid address");
        assert_eq!(error.code_number, 2003);
        assert_eq!(
            AppConfig::load_or_default(&paths.config_file)
                .unwrap()
                .network,
            preference
        );

        // Going back to automatic clears both fields.
        engine
            .set_network_selection(crate::config::NetworkPreference::auto())
            .unwrap();
        let selection = engine.network_selection().unwrap();
        assert_eq!(selection.mode, fluxy_net::interface::SelectionMode::Auto);
        assert_eq!(selection.address, None);
        assert_eq!(selection.interface_id, None);
    }

    #[test]
    fn rotation_invalidates_trust_and_changes_id() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path().to_path_buf());
        let mut engine = Engine::load(paths.clone()).unwrap();
        let old_id = engine.identity().device_id();
        engine.trust.insert(TrustEntry {
            device_id: Uuid::now_v7(),
            name: "peer".into(),
            platform: "test".into(),
            public_key: [1; 32],
            cert_fingerprint: [2; 32],
            paired_at_ms: 1,
            clipboard_policy: fluxy_core::ClipboardPolicy::Ask,
            auto_accept: fluxy_core::AutoAcceptPolicy::Inherit,
        });
        engine.trust.save(&paths.trust_file).unwrap();
        assert_eq!(engine.status().trusted_device_count, 1);

        engine.rotate_identity().unwrap();
        assert_ne!(engine.identity().device_id(), old_id);
        assert_eq!(engine.status().trusted_device_count, 0);
        let reloaded = Engine::load(paths).unwrap();
        assert_eq!(reloaded.status().trusted_device_count, 0);
        assert_ne!(reloaded.identity().device_id(), old_id);
    }
}
