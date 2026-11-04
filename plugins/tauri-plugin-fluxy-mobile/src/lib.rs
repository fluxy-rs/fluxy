//! Platform bridges Fluxy needs on mobile: a Keystore-backed secret store today,
//! file access and discovery as the A-03 to A-07 work lands.
//!
//! The desktop build keeps its own implementations, so every command here is a
//! no-op unless a mobile plugin answers it.

#[cfg(mobile)]
use tauri::Manager;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime};

#[cfg(mobile)]
mod mobile;
#[cfg(mobile)]
pub use mobile::FluxyMobile;

/// What the platform knows about the active network. Desktop leaves it empty.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalNetworkInfo {
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub interface: Option<String>,
    /// Every address the active link holds. The desktop path can enumerate
    /// interfaces itself; Android cannot (SELinux blocks the tables), so this is
    /// the only way the listener learns which address to bind.
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub wifi: bool,
    #[serde(default)]
    pub hotspot: bool,
}

/// System bar insets in pixels. Zero everywhere but mobile.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemInsetsView {
    pub top: i32,
    pub bottom: i32,
}

/// A document the platform picker (or a saved URI) handed over. It exists on
/// every target so the command signatures compile; only mobile fills it in.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpenedSource {
    /// Defaults to false so a platform bridge that opened an existing URI
    /// without mentioning the flag still deserializes.
    #[serde(default)]
    pub cancelled: bool,
    /// Only present when `cancelled` is false; a dismissed picker sends nothing
    /// else, so every field defaults.
    #[serde(default)]
    pub fd: i64,
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime: String,
}

/// Read the secret a mobile platform keeps for us. Desktop returns None so the
/// caller falls back to its own store.
#[tauri::command]
async fn secret_get<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.secret_get().await.map_err(|error: String| error);
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(None)
    }
}

/// Store a secret in the platform keystore.
#[tauri::command]
async fn secret_set<R: Runtime>(app: AppHandle<R>, value: String) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle
            .secret_set(value)
            .await
            .map_err(|error: String| error);
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, value);
        Err("the platform secret store is not available on this platform".into())
    }
}

/// Open the system picker and hand back an open descriptor. The caller owns the
/// descriptor and must close it.
#[tauri::command]
async fn fs_pick_source<R: Runtime>(app: AppHandle<R>) -> Result<OpenedSource, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.pick_source().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Err("document picking is not available on this platform".into())
    }
}

/// Reopen a document by URI, for a resume after a restart.
#[tauri::command]
async fn fs_open_source<R: Runtime>(
    app: AppHandle<R>,
    uri: String,
) -> Result<OpenedSource, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.open_source(uri).await;
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, uri);
        Err("document access is not available on this platform".into())
    }
}

/// The phone clipboard's current text.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipboardText {
    pub text: String,
}

#[tauri::command]
async fn clipboard_read<R: Runtime>(app: AppHandle<R>) -> Result<ClipboardText, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.clipboard_read().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Err("the system clipboard is not available here".into())
    }
}

#[tauri::command]
async fn clipboard_write<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.clipboard_write(text).await;
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, text);
        Err("the system clipboard is not available here".into())
    }
}

/// Default device name: the user's name for the phone, else brand + model.
#[tauri::command]
async fn device_name<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.device_name().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(String::new())
    }
}

/// Whether the platform lets the app use the local network.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalNetworkState {
    pub granted: bool,
}

#[tauri::command]
async fn local_network_granted<R: Runtime>(app: AppHandle<R>) -> Result<LocalNetworkState, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.local_network_granted().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(LocalNetworkState { granted: true })
    }
}

/// Files that were shared into the app from another application. Each entry is
/// a content:// URI the shell can register through `fs_open_source`.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedFile {
    pub uri: String,
}

/// A drained share: the files plus any device a deep link named.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedBatch {
    #[serde(default)]
    pub files: Vec<SharedFile>,
    #[serde(default)]
    pub device: Option<String>,
}

#[tauri::command]
async fn shared_files<R: Runtime>(app: AppHandle<R>) -> Result<SharedBatch, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.shared_files().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(SharedBatch::default())
    }
}

/// Ask Android to hold the process in the foreground while a transfer runs.
/// Desktop builds have no such concept and return ok immediately.
#[tauri::command]
async fn foreground_start<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        handle.foreground_start().await
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(())
    }
}

/// Hand a received file to whatever app handles its type.
/// Desktop builds open it directly, so this is the mobile-only path.
#[tauri::command]
async fn open_file<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        handle.open_file(path).await
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, path);
        Ok(())
    }
}

/// Release the foreground slot.
#[tauri::command]
async fn foreground_stop<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        handle.foreground_stop().await
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(())
    }
}

/// Copy a finished file into the platform's public downloads.
#[tauri::command]
async fn fs_export<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    name: String,
    mime: String,
) -> Result<String, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.export(path, name, mime).await;
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, path, name, mime);
        Err("exporting to the platform downloads is not available here".into())
    }
}

/// A peer NsdManager resolved.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredPeer {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: u16,
}

/// Register the local service and start browsing through NsdManager.
#[tauri::command]
async fn discovery_start<R: Runtime>(
    app: AppHandle<R>,
    port: u16,
    device_id: String,
    name: String,
    fingerprint: String,
) -> Result<bool, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle
            .discovery_start(port, device_id, name, fingerprint)
            .await;
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, port, device_id, name, fingerprint);
        Ok(false)
    }
}

#[tauri::command]
async fn discovery_stop<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.discovery_stop().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(())
    }
}

/// Peers resolved so far.
#[tauri::command]
async fn discovery_peers<R: Runtime>(app: AppHandle<R>) -> Result<Vec<DiscoveredPeer>, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.discovery_peers().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(Vec::new())
    }
}

/// Active network facts: the gateway the fallback probe should aim at.
#[tauri::command]
async fn local_network_info<R: Runtime>(app: AppHandle<R>) -> Result<LocalNetworkInfo, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.local_network_info().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(LocalNetworkInfo::default())
    }
}

/// Paint the window behind the status bar so it matches the page background.
#[tauri::command]
async fn set_chrome_color<R: Runtime>(app: AppHandle<R>, color: String) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.set_chrome_color(color).await;
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, color);
        Ok(())
    }
}

/// System bar insets, so a mobile page can keep its content out of them.
#[tauri::command]
async fn system_insets<R: Runtime>(app: AppHandle<R>) -> Result<SystemInsetsView, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.system_insets().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(SystemInsetsView::default())
    }
}

/// Free bytes on the receive volume.
#[tauri::command]
async fn storage_available<R: Runtime>(app: AppHandle<R>) -> Result<i64, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle.storage_available().await;
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(0)
    }
}

/// Whether the platform store is usable. The settings page shows this on mobile.
#[tauri::command]
async fn keystore_available<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    #[cfg(mobile)]
    {
        let handle = app.state::<FluxyMobile<R>>();
        return handle
            .keystore_available()
            .await
            .map_err(|error: String| error);
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        Ok(false)
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("fluxy-mobile")
        .invoke_handler(tauri::generate_handler![
            secret_get,
            secret_set,
            keystore_available,
            fs_pick_source,
            fs_open_source,
            fs_export,
            storage_available,
            system_insets,
            set_chrome_color,
            local_network_info,
            device_name,
            local_network_granted,
            clipboard_read,
            clipboard_write,
            shared_files,
            foreground_start,
            foreground_stop,
            open_file,
            discovery_start,
            discovery_stop,
            discovery_peers
        ])
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin("com.thep0y.fluxy.mobile", "FluxyMobilePlugin")?;
                app.manage(FluxyMobile::new(handle));
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .build()
}
