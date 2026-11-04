use crate::{DiscoveredPeer, LocalNetworkInfo, OpenedSource, SystemInsetsView};
use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tauri::plugin::PluginHandle;

/// The handle Tauri gives us for the Android/iOS side of this plugin.
pub struct FluxyMobile<R: Runtime>(PluginHandle<R>);

// PluginHandle is cloneable regardless of the runtime type; a derive would add a
// needless `R: Clone` bound that Wry does not satisfy.
impl<R: Runtime> Clone for FluxyMobile<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Empty {}

/// What the platform answers for a read: the Kotlin side resolves an object so
/// a missing secret can be distinguished from an empty one.
#[derive(Debug, Deserialize, Serialize)]
pub struct SecretValue {
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExportArgs {
    pub path: String,
    pub name: String,
    pub mime: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscoveryStartArgs {
    pub port: i32,
    pub device_id: String,
    pub name: String,
    pub fingerprint: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChromeColor {
    pub color: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SourceUri {
    pub uri: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KeystoreStatus {
    pub available: bool,
}

impl<R: Runtime> FluxyMobile<R> {
    /// Post a chat notification carrying the conversation's device id, so a tap
    /// opens that thread instead of the message list.
    pub fn notify_chat(&self, device_id: &str, title: &str, body: &str) -> Result<(), String> {
        let payload = serde_json::json!({
            "deviceId": device_id,
            "title": title,
            "body": body,
        });
        self.0
            .run_mobile_plugin::<serde_json::Value>("notifyChat", payload)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn new(handle: PluginHandle<R>) -> Self {
        Self(handle)
    }

    pub async fn secret_get(&self) -> Result<Option<String>, String> {
        let answer: SecretValue = self
            .0
            .run_mobile_plugin("secretGet", Empty {})
            .map_err(|error| error.to_string())?;
        Ok(answer.value)
    }

    pub async fn secret_set(&self, value: String) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<()>("secretSet", Payload { value })
            .map_err(|error| error.to_string())
    }

    pub async fn pick_source(&self) -> Result<OpenedSource, String> {
        self.0
            .run_mobile_plugin("fsPickSource", Empty {})
            .map_err(|error| error.to_string())
    }

    pub async fn open_source(&self, uri: String) -> Result<OpenedSource, String> {
        self.0
            .run_mobile_plugin("fsOpenSource", SourceUri { uri })
            .map_err(|error| error.to_string())
    }

    /// Default device name: the user's name for the phone, else brand + model.
    pub async fn device_name(&self) -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct Named {
            name: String,
        }
        self.0
            .run_mobile_plugin::<Named>("deviceName", Empty {})
            .map(|payload| payload.name)
            .map_err(|error| error.to_string())
    }

    /// Whether the platform lets the app use the local network.
    pub async fn local_network_granted(&self) -> Result<crate::LocalNetworkState, String> {
        self.0
            .run_mobile_plugin::<crate::LocalNetworkState>("localNetworkGranted", Empty {})
            .map_err(|error| error.to_string())
    }

    /// Current text on the phone clipboard.
    pub async fn clipboard_read(&self) -> Result<crate::ClipboardText, String> {
        self.0
            .run_mobile_plugin::<crate::ClipboardText>("clipboardRead", Empty {})
            .map_err(|error| error.to_string())
    }

    /// Replace the phone clipboard.
    pub async fn clipboard_write(&self, text: String) -> Result<(), String> {
        #[derive(Serialize)]
        struct Payload {
            text: String,
        }
        self.0
            .run_mobile_plugin::<()>("clipboardWrite", Payload { text })
            .map_err(|error| error.to_string())
    }

    /// Drain the files shared into the app since the last call.
    pub async fn shared_files(&self) -> Result<crate::SharedBatch, String> {
        self.0
            .run_mobile_plugin::<crate::SharedBatch>("sharedFiles", Empty {})
            .map_err(|error| error.to_string())
    }

    /// Keep the process in the foreground for the duration of a transfer.
    pub async fn foreground_start(&self) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<()>("foregroundStart", Empty {})
            .map_err(|error| error.to_string())
    }

    /// Release the foreground slot.
    pub async fn foreground_stop(&self) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<()>("foregroundStop", Empty {})
            .map_err(|error| error.to_string())
    }

    pub async fn export(&self, path: String, name: String, mime: String) -> Result<String, String> {
        #[derive(Deserialize, Serialize)]
        struct ExportUri {
            uri: String,
        }
        let answer: ExportUri = self
            .0
            .run_mobile_plugin("fsExport", ExportArgs { path, name, mime })
            .map_err(|error| error.to_string())?;
        Ok(answer.uri)
    }

    pub async fn discovery_start(
        &self,
        port: u16,
        device_id: String,
        name: String,
        fingerprint: String,
    ) -> Result<bool, String> {
        #[derive(Deserialize, Serialize)]
        struct Started {
            started: bool,
        }
        let answer: Started = self
            .0
            .run_mobile_plugin(
                "discoveryStart",
                DiscoveryStartArgs {
                    port: port as i32,
                    device_id,
                    name,
                    fingerprint,
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(answer.started)
    }

    pub async fn discovery_stop(&self) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<()>("discoveryStop", Empty {})
            .map_err(|error| error.to_string())
    }

    pub async fn discovery_peers(&self) -> Result<Vec<DiscoveredPeer>, String> {
        #[derive(Deserialize, Serialize)]
        struct Peers {
            peers: Vec<DiscoveredPeer>,
        }
        let answer: Peers = self
            .0
            .run_mobile_plugin("discoveryPeers", Empty {})
            .map_err(|error| error.to_string())?;
        Ok(answer.peers)
    }

    pub async fn local_network_info(&self) -> Result<LocalNetworkInfo, String> {
        self.0
            .run_mobile_plugin("localNetworkInfo", Empty {})
            .map_err(|error| error.to_string())
    }

    pub async fn set_chrome_color(&self, color: String) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<()>("setChromeColor", ChromeColor { color })
            .map_err(|error| error.to_string())
    }

    pub async fn system_insets(&self) -> Result<SystemInsetsView, String> {
        self.0
            .run_mobile_plugin("systemInsets", Empty {})
            .map_err(|error| error.to_string())
    }

    pub async fn storage_available(&self) -> Result<i64, String> {
        #[derive(Deserialize, Serialize)]
        struct Storage {
            #[serde(rename = "freeBytes")]
            free_bytes: i64,
        }
        let answer: Storage = self
            .0
            .run_mobile_plugin("storageAvailable", Empty {})
            .map_err(|error| error.to_string())?;
        Ok(answer.free_bytes)
    }

    pub async fn keystore_available(&self) -> Result<bool, String> {
        let answer: KeystoreStatus = self
            .0
            .run_mobile_plugin("keystoreAvailable", Empty {})
            .map_err(|error| error.to_string())?;
        Ok(answer.available)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Payload {
    pub value: String,
}
