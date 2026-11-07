use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use fluxy_net::NetworkSelection;
use fluxy_net::interface::{SelectionMode, parse_address};

use crate::{Error, ErrorCode};

pub const CONFIG_VERSION: u32 = 1;

/// The port the QUIC listener asks for. A random port sounds tidier, but every
/// restart then invalidates every address a peer has learned and a live chat
/// session points at a socket nobody is listening on, so transfers that are
/// started right after a restart fail to connect. Pinned, the address survives
/// restarts.
pub const DEFAULT_LISTEN_PORT: u16 = 49784;

/// Default number of transfers that may be active at the same time.
///
/// One at a time is the conservative default: on a LAN one transfer already
/// saturates the link, so extra concurrency mostly splits the same bandwidth.
/// It is a throughput choice, not a correctness one - each transfer names its
/// stream with its transfer id, so any value up to the cap runs safely, and a
/// user who wants a batch to move together can raise it in settings.
pub const DEFAULT_ACTIVE_TRANSFERS: u32 = 1;
/// The user-facing floor for the active-transfer cap.
pub const MIN_ACTIVE_TRANSFERS: u32 = 1;
/// The user-facing ceiling for the active-transfer cap.
pub const MAX_ACTIVE_TRANSFERS: u32 = 8;

/// Clamp a user-supplied active-transfer limit into the supported range.
pub fn clamp_active_transfers(limit: u32) -> u32 {
    limit.clamp(MIN_ACTIVE_TRANSFERS, MAX_ACTIVE_TRANSFERS)
}

/// Longest device name the UI and the discovery records accept, in characters
/// rather than bytes so a CJK name is not truncated mid-word.
pub const MAX_DEVICE_NAME_CHARS: usize = 64;

/// Bound a user-supplied device name: trim, drop control characters, and fall
/// back to the machine default when nothing is left. The name is advertised in
/// mDNS and sent in the handshake, so it must never be empty or contain a
/// newline that could forge another TXT record.
pub fn sanitize_device_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_DEVICE_NAME_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        default_device_name()
    } else {
        trimmed.to_string()
    }
}

fn default_max_active_transfers() -> u32 {
    DEFAULT_ACTIVE_TRANSFERS
}

fn default_notifications() -> bool {
    true
}

fn default_autostart() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub version: u32,
    pub device_name: String,
    pub listen_port: u16,
    pub discovery_port: u16,
    pub clipboard_max_bytes: u64,
    pub auto_clipboard: bool,
    /// Global cap on simultaneous active transfers. Defaulted so a config
    /// written by an earlier version still loads.
    #[serde(default = "default_max_active_transfers")]
    pub max_active_transfers: u32,
    /// Accept incoming transfers from trusted devices without asking. Defaulted
    /// so an older config still loads, and off: receiving a file is a decision.
    #[serde(default)]
    pub auto_accept_transfers: bool,
    /// Desktop notifications for chat, clipboard, and transfer events.
    /// Defaulted so a config written by an earlier version still loads.
    #[serde(default = "default_notifications")]
    pub notifications: bool,
    /// Register the app as a login item. Off by default; the settings page and
    /// the tray both own the switch.
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    /// Which interface and address to advertise and listen on. Defaulted so a
    /// config written by an earlier version still loads.
    #[serde(default)]
    pub network: NetworkPreference,
    /// Where received files land. None means the default
    /// <data_dir>/received, which is what every earlier version used.
    #[serde(default)]
    pub receive_dir: Option<PathBuf>,
    pub dev_allow_loopback: bool,
}

/// The persisted form of the plan's section 3.5 selection.
///
/// The address is stored as a string because a link-local IPv6 address carries
/// its interface scope ("fe80::1%Wi-Fi") and the user may paste exactly what the
/// settings page showed. The interface list itself never leaves the machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkPreference {
    pub mode: SelectionMode,
    pub interface_id: Option<String>,
    pub address: Option<String>,
}

impl Default for NetworkPreference {
    fn default() -> Self {
        Self {
            mode: SelectionMode::Auto,
            interface_id: None,
            address: None,
        }
    }
}

impl NetworkPreference {
    pub fn auto() -> Self {
        Self::default()
    }

    pub fn manual(interface_id: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            mode: SelectionMode::Manual,
            interface_id: Some(interface_id.into()),
            address: Some(address.into()),
        }
    }

    pub fn is_auto(&self) -> bool {
        self.mode == SelectionMode::Auto
    }

    /// Combine the preference with the listening port and the development switch
    /// into what the runtime binds.
    pub fn to_selection(&self, port: u16, dev_loopback: bool) -> Result<NetworkSelection, Error> {
        let address = match self.address.as_deref().map(str::trim) {
            Some(text) if !text.is_empty() => Some(parse_address(text)?.0),
            _ => None,
        };
        let selection = NetworkSelection {
            mode: self.mode,
            interface_id: self.interface_id.clone(),
            address,
            port,
            dev_loopback,
        };
        selection.validate()?;
        Ok(selection)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            device_name: default_device_name(),
            listen_port: DEFAULT_LISTEN_PORT,
            discovery_port: fluxy_net::DISCOVERY_PORT,
            clipboard_max_bytes: 1024 * 1024,
            auto_clipboard: false,
            max_active_transfers: DEFAULT_ACTIVE_TRANSFERS,
            auto_accept_transfers: false,
            notifications: true,
            autostart: false,
            network: NetworkPreference::default(),
            receive_dir: None,
            dev_allow_loopback: false,
        }
    }
}

impl AppConfig {
    /// Point received files at a specific directory. The path is created when
    /// missing and must be a directory, never a file.
    pub fn set_receive_dir(&mut self, directory: Option<PathBuf>) -> Result<(), Error> {
        if let Some(directory) = &directory {
            fs::create_dir_all(directory)?;
            if !directory.is_dir() {
                return Err(Error::new(
                    ErrorCode::FileAccess,
                    "the receive directory is not a directory",
                    false,
                ));
            }
        }
        self.receive_dir = directory;
        Ok(())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, Error> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path)?;
        let mut config: Self = serde_json::from_slice(&bytes)?;
        // A config written before the port was pinned carries 0, meaning "any".
        // Adopting the fixed port here upgrades it without a version bump.
        if config.listen_port == 0 {
            config.listen_port = DEFAULT_LISTEN_PORT;
        }
        if config.version != CONFIG_VERSION {
            return Err(Error::new(
                ErrorCode::Protocol,
                "unsupported config version",
                false,
            ));
        }
        Ok(config)
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
}

/// The placeholder the app uses when the platform reports no name at all.
pub fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Fluxy device".into())
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub identity_file: PathBuf,
    pub trust_file: PathBuf,
    pub tls_cert_file: PathBuf,
    pub tls_key_file: PathBuf,
    pub log_dir: PathBuf,
    pub transfers_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, Error> {
        let root = dirs::data_dir()
            .ok_or_else(|| Error::new(ErrorCode::Internal, "no OS data directory", false))?
            .join("fluxy");
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            config_file: root.join("config.json"),
            identity_file: root.join("identity.json"),
            trust_file: root.join("trust.json"),
            tls_cert_file: root.join("tls-cert.der"),
            tls_key_file: root.join("tls-key.der"),
            log_dir: root.join("logs"),
            transfers_dir: root.join("transfers"),
            data_dir: root,
        }
    }

    pub fn ensure(&self) -> Result<(), Error> {
        fs::create_dir_all(&self.data_dir)?;
        fs::create_dir_all(&self.log_dir)?;
        fs::create_dir_all(&self.transfers_dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = AppConfig::default();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.clipboard_max_bytes, 1024 * 1024);
        assert!(!config.auto_clipboard);
        assert!(!config.dev_allow_loopback);
        assert_eq!(config.max_active_transfers, DEFAULT_ACTIVE_TRANSFERS);
        assert!(config.notifications);
    }

    #[test]
    fn the_receive_directory_persists_and_rejects_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let target = dir.path().join("inbox");
        let mut config = AppConfig::default();
        assert!(config.receive_dir.is_none());

        config.set_receive_dir(Some(target.clone())).unwrap();
        assert_eq!(config.receive_dir.as_deref(), Some(target.as_path()));
        assert!(target.is_dir(), "the directory is created on demand");
        config.save(&path).unwrap();
        let reloaded = AppConfig::load_or_default(&path).unwrap();
        assert_eq!(reloaded.receive_dir.as_deref(), Some(target.as_path()));

        // A file is not a directory, and the previous value survives the refusal.
        let file = dir.path().join("plain.txt");
        fs::write(&file, b"x").unwrap();
        let mut rejected = reloaded.clone();
        assert!(rejected.set_receive_dir(Some(file)).is_err());
        assert_eq!(rejected.receive_dir.as_deref(), Some(target.as_path()));

        config.set_receive_dir(None).unwrap();
        assert!(config.receive_dir.is_none());
    }

    #[test]
    fn a_config_without_the_transfer_cap_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            br#"{"version":1,"device_name":"Old","listen_port":0,"discovery_port":42420,"clipboard_max_bytes":1048576,"auto_clipboard":false,"dev_allow_loopback":false}"#,
        )
        .unwrap();
        let config = AppConfig::load_or_default(&path).unwrap();
        assert_eq!(config.max_active_transfers, DEFAULT_ACTIVE_TRANSFERS);
        assert!(config.notifications);
    }

    #[test]
    fn a_device_name_is_bounded_and_never_empty() {
        assert_eq!(sanitize_device_name("  Studio  "), "Studio");
        assert_eq!(sanitize_device_name("Living\nRoom"), "LivingRoom");
        assert_eq!(sanitize_device_name("   "), default_device_name());
        let long = "é".repeat(MAX_DEVICE_NAME_CHARS + 20);
        let bounded = sanitize_device_name(&long);
        assert_eq!(bounded.chars().count(), MAX_DEVICE_NAME_CHARS);
    }

    #[test]
    fn the_transfer_cap_is_clamped() {
        assert_eq!(clamp_active_transfers(0), MIN_ACTIVE_TRANSFERS);
        assert_eq!(clamp_active_transfers(3), 3);
        assert_eq!(clamp_active_transfers(99), MAX_ACTIVE_TRANSFERS);
    }

    #[test]
    fn the_network_preference_defaults_to_automatic() {
        let config = AppConfig::default();
        assert!(config.network.is_auto());
        assert_eq!(config.network.interface_id, None);
        assert_eq!(config.network.address, None);
        let selection = config.network.to_selection(42420, false).unwrap();
        assert_eq!(selection.mode, SelectionMode::Auto);
        assert_eq!(selection.address, None);
        assert_eq!(selection.port, 42420);
    }

    #[test]
    fn a_config_without_the_network_preference_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            br#"{"version":1,"device_name":"Old","listen_port":0,"discovery_port":42420,"clipboard_max_bytes":1048576,"auto_clipboard":false,"dev_allow_loopback":false}"#,
        )
        .unwrap();
        let config = AppConfig::load_or_default(&path).unwrap();
        assert!(config.network.is_auto());
    }

    #[test]
    fn a_manual_preference_needs_an_interface_and_an_address() {
        let empty = NetworkPreference {
            mode: SelectionMode::Manual,
            interface_id: None,
            address: None,
        };
        let error = empty.to_selection(0, false).expect_err("no address");
        assert_eq!(error.code_number, 2003);

        let no_interface = NetworkPreference {
            mode: SelectionMode::Manual,
            interface_id: Some("  ".into()),
            address: Some("192.168.1.20".into()),
        };
        let error = no_interface
            .to_selection(0, false)
            .expect_err("no interface");
        assert_eq!(error.code_number, 2003);

        let unparsable = NetworkPreference::manual("Wi-Fi", "not-an-address");
        let error = unparsable.to_selection(0, false).expect_err("bad address");
        assert_eq!(error.code_number, 2003);
    }

    #[test]
    fn a_link_local_preference_keeps_its_scope_and_loopback_needs_the_switch() {
        let preference = NetworkPreference::manual("Wi-Fi", "fe80::20%Wi-Fi");
        let selection = preference.to_selection(0, false).unwrap();
        assert_eq!(selection.address, Some("fe80::20".parse().unwrap()));
        assert_eq!(selection.interface_id.as_deref(), Some("Wi-Fi"));

        let loopback = NetworkPreference::manual("Loopback", "127.0.0.1");
        assert_eq!(
            loopback.to_selection(0, false).unwrap_err().code_number,
            2003
        );
        assert!(loopback.to_selection(0, true).is_ok());
    }

    #[test]
    fn the_network_preference_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = AppConfig {
            network: NetworkPreference::manual("Wi-Fi", "fe80::20%Wi-Fi"),
            ..AppConfig::default()
        };
        config.save(&path).unwrap();
        let loaded = AppConfig::load_or_default(&path).unwrap();
        assert_eq!(loaded, config);
        assert_eq!(loaded.network.mode, SelectionMode::Manual);
        assert_eq!(loaded.network.address.as_deref(), Some("fe80::20%Wi-Fi"));
    }

    #[test]
    fn config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = AppConfig::default();
        config.save(&path).unwrap();
        assert_eq!(AppConfig::load_or_default(&path).unwrap(), config);
    }
}
