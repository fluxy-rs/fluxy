//! Tauri commands and the generated frontend contract.

use tauri::ipc::Channel;
use tauri::{State, Wry};
use tauri_specta::{Builder, collect_commands};
use tokio::sync::broadcast::error::RecvError;

use crate::contract::{
    AppEvent, AppSnapshot, AppState, ChatMessageView, ClipboardReceivePolicy, ClipboardShareView,
    DeviceAutoAccept, EngineEvent, EngineStatusView, IpcError, NearbyPeerView, NetworkStatusView,
    PairingChallengeView, RegisteredFileView, RegisteredFolderView, SettingsSnapshot,
    SubscriptionInfo, SystemInsets, TransferAction, TransferSummary,
};
use crate::notify::ShellLabels;

#[tauri::command]
#[specta::specta]
pub fn engine_status(state: State<'_, AppState>) -> Result<EngineStatusView, IpcError> {
    state.status()
}

#[tauri::command]
#[specta::specta]
pub fn state_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, IpcError> {
    state.snapshot()
}

#[tauri::command]
#[specta::specta]
pub fn events_subscribe(
    state: State<'_, AppState>,
    channel: Channel<crate::contract::AppEventEnvelope>,
) -> Result<SubscriptionInfo, IpcError> {
    // Emit a first heartbeat before subscribing so this client receives an
    // immediate, real event without a duplicate arriving through the stream.
    let envelope = state.hub.emit(AppEvent::Engine {
        data: EngineEvent::Heartbeat {
            uptime_ms: state.hub.uptime_ms(),
        },
    });
    let mut receiver = state.hub.subscribe();
    channel
        .send(envelope.clone())
        .map_err(|error| IpcError::internal(error.to_string()))?;

    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(envelope) => {
                    if channel.send(envelope).is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    // The next delivered envelope will carry a visible seq gap;
                    // the frontend rebuilds from state_snapshot instead of
                    // closing the channel.
                    tracing::warn!(skipped, "event channel lagged; continuing");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });

    Ok(SubscriptionInfo {
        seq: envelope.seq,
        snapshot_version: envelope.snapshot_version,
    })
}

#[tauri::command]
#[specta::specta]
pub fn identity_rotate(state: State<'_, AppState>) -> Result<EngineStatusView, IpcError> {
    state.rotate_identity()
}

/// Devices discovery has seen, so the UI can offer to pair instead of asking for
/// an address.
#[tauri::command]
#[specta::specta]
pub fn nearby_peers(state: State<'_, AppState>) -> Vec<NearbyPeerView> {
    state.nearby_peers()
}

#[tauri::command]
#[specta::specta]
pub fn pair_start(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<PairingChallengeView, IpcError> {
    state.pair_start(&device_id)
}

/// Pair with an address and fingerprint typed by the user, for networks where
/// discovery cannot see the peer (a NATed emulator, multicast filtering).
#[tauri::command]
#[specta::specta]
pub fn pair_manual(
    state: State<'_, AppState>,
    address: String,
    fingerprint: String,
) -> Result<PairingChallengeView, IpcError> {
    state.pair_manual(address, fingerprint)
}

#[tauri::command]
#[specta::specta]
pub fn pair_confirm(
    state: State<'_, AppState>,
    challenge_id: String,
    accepted: bool,
) -> Result<(), IpcError> {
    state.pair_confirm(&challenge_id, accepted)
}

#[tauri::command]
#[specta::specta]
pub fn pair_cancel(state: State<'_, AppState>, challenge_id: String) -> Result<(), IpcError> {
    state.pair_cancel(&challenge_id)
}

#[tauri::command]
#[specta::specta]
pub fn trust_remove(state: State<'_, AppState>, device_id: String) -> Result<bool, IpcError> {
    state.trust_remove(&device_id)
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_set_policy(
    state: State<'_, AppState>,
    device_id: String,
    policy: ClipboardReceivePolicy,
) -> Result<(), IpcError> {
    state.clipboard_set_policy(&device_id, policy)
}

#[tauri::command]
#[specta::specta]
pub fn chat_open(state: State<'_, AppState>, device_id: String) -> Result<(), IpcError> {
    state.chat_open(&device_id)
}

#[tauri::command]
#[specta::specta]
pub async fn chat_send(
    state: State<'_, AppState>,
    device_id: String,
    text: String,
) -> Result<ChatMessageView, IpcError> {
    state.chat_send(&device_id, text).await
}

#[tauri::command]
#[specta::specta]
pub async fn chat_retry(
    state: State<'_, AppState>,
    device_id: String,
    message_id: String,
) -> Result<ChatMessageView, IpcError> {
    state.chat_retry(&device_id, &message_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn clipboard_share(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<ClipboardShareView, IpcError> {
    state.clipboard_share(&device_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn clipboard_accept(
    state: State<'_, AppState>,
    device_id: String,
    offer_id: String,
) -> Result<ClipboardShareView, IpcError> {
    state.clipboard_accept(&device_id, &offer_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn clipboard_reject(
    state: State<'_, AppState>,
    device_id: String,
    offer_id: String,
) -> Result<ClipboardShareView, IpcError> {
    state.clipboard_reject(&device_id, &offer_id).await
}

/// Native tray and notification strings are localized by the WebView i18n
/// catalog; the frontend pushes them again whenever the locale changes.
#[tauri::command]
#[specta::specta]
pub fn shell_set_labels(state: State<'_, AppState>, labels: ShellLabels) -> Result<(), IpcError> {
    state.notifier.set_labels(labels);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn discovery_diagnostics(state: State<'_, AppState>) -> Result<NetworkStatusView, IpcError> {
    Ok(state.network.status())
}

/// Open the native picker and register the chosen file. The path never
/// crosses the IPC boundary; only the opaque handle and its metadata do.
#[tauri::command]
#[specta::specta]
pub async fn pick_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<Option<RegisteredFileView>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut builder = app.dialog().file();
    if let Some(title) = title {
        builder = builder.set_title(title);
    }
    builder.pick_file(move |path| {
        let _ = sender.send(path);
    });
    let picked = receiver
        .await
        .map_err(|_| IpcError::internal("the file picker closed unexpectedly"))?;
    match picked {
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|error| IpcError::internal(error.to_string()))?;
            state
                .transfer
                .register_file(&path.to_string_lossy())
                .map(Some)
        }
        None => Ok(None),
    }
}

/// Open the platform picker and register the chosen document as a sendable
/// file. The descriptor crosses from the plugin and becomes the source; only
/// the opaque view is returned.
/// Android opens the picker; desktop has no picker here and answers 4601.
#[tauri::command]
#[specta::specta]
pub async fn pick_source_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<RegisteredFileView>, IpcError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, state);
        Err(IpcError {
            code: "unsupported".into(),
            code_number: 4601,
            message: "picking a file is only available on mobile".into(),
            retryable: false,
        })
    }
    #[cfg(target_os = "android")]
    {
        use std::os::unix::io::FromRawFd;
        use tauri::Manager;

        let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
        let opened = handle.pick_source().await.map_err(|message| IpcError {
            code: "file_access".into(),
            code_number: 3003,
            message,
            retryable: true,
        })?;
        if opened.cancelled {
            return Ok(None);
        }
        if opened.fd <= 0 {
            return Err(IpcError {
                code: "file_access".into(),
                code_number: 3003,
                message: "the picker returned no descriptor".into(),
                retryable: false,
            });
        }
        // Ownership moves to Rust once the descriptor is handed over.
        let file = unsafe { std::fs::File::from_raw_fd(opened.fd as i32) };
        tracing::info!(
            uri = %opened.uri,
            name = %opened.name,
            size = opened.size,
            "registering a picked document"
        );
        match state.transfer.register_descriptor_source(
            opened.uri,
            file,
            opened.name,
            Some(opened.mime),
        ) {
            Ok(view) => {
                tracing::info!(file_id = %view.file_id, "document registered");
                Ok(Some(view))
            }
            Err(error) => {
                tracing::warn!(code = %error.code, message = %error.message, "document registration failed");
                Err(error)
            }
        }
    }
}

/// Recent application log lines, oldest first.
///
/// A release build has nothing attached to its stderr, so this is the only way
/// to see why something failed on a device that has already been installed.
#[tauri::command]
#[specta::specta]
pub fn app_log_tail(limit: usize) -> Result<Vec<String>, IpcError> {
    let capped = limit.clamp(1, 5_000);
    fluxy_app::logging::tail_recent(capped).map_err(|error| IpcError {
        code: "log".into(),
        code_number: 4901,
        message: error.to_string(),
        retryable: true,
    })
}

/// Register a document handed to us by URI (share sheet or a deep link).
///
/// `fs_open_source` only opens the descriptor; the file has to be registered with
/// the transfer runtime before it has an id that `start_transfers` accepts. The
/// picker path does this inline, which is why a shared file used to arrive with
/// no `file_id` and a send that silently did nothing.
#[tauri::command]
#[specta::specta]
pub async fn open_shared_source(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    uri: String,
) -> Result<Option<RegisteredFileView>, IpcError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, state, uri);
        Err(IpcError {
            code: "unsupported".into(),
            code_number: 4601,
            message: "registering a shared document is only available on mobile".into(),
            retryable: false,
        })
    }
    #[cfg(target_os = "android")]
    {
        use std::os::unix::io::FromRawFd;
        use tauri::Manager;

        let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
        let opened = handle.open_source(uri).await.map_err(|message| IpcError {
            code: "file_access".into(),
            code_number: 3003,
            message,
            retryable: true,
        })?;
        if opened.cancelled || opened.fd <= 0 {
            return Ok(None);
        }
        let file = unsafe { std::fs::File::from_raw_fd(opened.fd as i32) };
        tracing::info!(
            uri = %opened.uri,
            name = %opened.name,
            size = opened.size,
            "registering a shared document"
        );
        match state.transfer.register_descriptor_source(
            opened.uri,
            file,
            opened.name,
            Some(opened.mime),
        ) {
            Ok(view) => {
                tracing::info!(file_id = %view.file_id, "shared document registered");
                Ok(Some(view))
            }
            Err(error) => {
                tracing::warn!(code = %error.code, message = %error.message, "shared document registration failed");
                Err(error)
            }
        }
    }
}

/// Pick one folder for a batch send. Only the opaque id and a summary cross
/// this boundary; the paths stay in the runtime.
#[tauri::command]
#[specta::specta]
pub async fn pick_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<Option<RegisteredFolderView>, IpcError> {
    #[cfg(not(desktop))]
    {
        let _ = (&app, &state, &title);
        return Err(IpcError {
            code: "unsupported".into(),
            code_number: 4601,
            message: "folder selection is not available on this platform yet".into(),
            retryable: false,
        });
    }

    #[cfg(desktop)]
    {
        use tauri_plugin_dialog::DialogExt;

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut builder = app.dialog().file();
        if let Some(title) = title {
            builder = builder.set_title(title);
        }
        builder.pick_folder(move |path| {
            let _ = sender.send(path);
        });
        let picked = receiver
            .await
            .map_err(|_| IpcError::internal("the folder picker closed unexpectedly"))?;
        match picked {
            Some(path) => {
                let path = path
                    .into_path()
                    .map_err(|error| IpcError::internal(error.to_string()))?;
                state
                    .transfer
                    .register_folder(&path.to_string_lossy())
                    .map(Some)
            }
            None => Ok(None),
        }
    }
}

/// Pick several local files at once. Directories are refused by the picker;
/// only opaque handles cross this boundary.
#[tauri::command]
#[specta::specta]
pub async fn pick_files(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<Vec<RegisteredFileView>, IpcError> {
    use tauri_plugin_dialog::DialogExt;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut builder = app.dialog().file();
    if let Some(title) = title {
        builder = builder.set_title(title);
    }
    builder.pick_files(move |paths| {
        let _ = sender.send(paths);
    });
    let picked = receiver
        .await
        .map_err(|_| IpcError::internal("the file picker closed unexpectedly"))?;
    let Some(paths) = picked else {
        return Ok(Vec::new());
    };
    let mut views = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path
            .into_path()
            .map_err(|error| IpcError::internal(error.to_string()))?;
        views.push(state.transfer.register_file(&path.to_string_lossy())?);
    }
    Ok(views)
}

/// Queue one transfer per registered file for a trusted device.
#[tauri::command]
#[specta::specta]
pub async fn start_transfers(
    state: State<'_, AppState>,
    device_id: String,
    file_ids: Vec<String>,
) -> Result<Vec<TransferSummary>, IpcError> {
    match state.transfer.start_transfers(&device_id, &file_ids).await {
        Ok(summaries) => {
            tracing::info!(count = summaries.len(), "transfers queued");
            Ok(summaries)
        }
        Err(error) => {
            tracing::warn!(
                code = %error.code,
                message = %error.message,
                files = ?file_ids,
                "queuing transfers failed"
            );
            Err(error)
        }
    }
}

/// Register a folder for a batch send. The path stays in the runtime; the
/// WebView only receives an id and a summary.
#[tauri::command]
#[specta::specta]
pub fn register_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<RegisteredFolderView, IpcError> {
    state.transfer.register_folder(&path)
}

/// Queue one transfer per file of a registered folder.
#[tauri::command]
#[specta::specta]
pub fn start_folder(
    state: State<'_, AppState>,
    device_id: String,
    folder_id: String,
) -> Result<Vec<TransferSummary>, IpcError> {
    state.transfer.start_folder(&device_id, &folder_id)
}

/// Change the global active-transfer cap and persist it.
#[tauri::command]
#[specta::specta]
pub fn set_transfer_concurrency(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_transfer_concurrency(limit)
}

/// Rename this device and persist it. The LAN advertisement is restarted so
/// other devices see the new name, which drops sessions in flight.
#[tauri::command]
#[specta::specta]
pub fn set_device_name(
    state: State<'_, AppState>,
    name: String,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_device_name(name)
}

/// Choose where received files land. An empty value restores the default.
#[tauri::command]
#[specta::specta]
pub fn set_receive_dir(
    state: State<'_, AppState>,
    directory: Option<String>,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_receive_dir(directory)
}

/// Open the native folder picker and return the chosen path, or None when the
/// dialog was dismissed. The path is only proposed; the caller decides whether
/// to keep it.
#[tauri::command]
#[specta::specta]
pub async fn pick_receive_dir(
    app: tauri::AppHandle,
    title: Option<String>,
) -> Result<Option<String>, IpcError> {
    #[cfg(not(desktop))]
    {
        let _ = (&app, &title);
        return Err(IpcError {
            code: "unsupported".into(),
            code_number: 4601,
            message: "folder selection is not available on this platform yet".into(),
            retryable: false,
        });
    }

    #[cfg(desktop)]
    {
        use tauri_plugin_dialog::DialogExt;

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut builder = app.dialog().file();
        if let Some(title) = title {
            builder = builder.set_title(title);
        }
        builder.pick_folder(move |path| {
            let _ = sender.send(path);
        });
        let picked = receiver
            .await
            .map_err(|_| IpcError::internal("the folder picker closed unexpectedly"))?;
        match picked {
            Some(path) => {
                let path = path
                    .into_path()
                    .map_err(|error| IpcError::internal(error.to_string()))?;
                Ok(Some(path.to_string_lossy().to_string()))
            }
            None => Ok(None),
        }
    }
}

/// Paint the window behind the status bar to match the page background.
#[tauri::command]
#[specta::specta]
pub async fn set_chrome_color(app: tauri::AppHandle, color: String) -> Result<(), IpcError> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
        return handle
            .set_chrome_color(color)
            .await
            .map_err(IpcError::internal);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, color);
        Ok(())
    }
}

/// System bar insets in pixels, so a mobile page can pad itself. Desktop has no
/// such bars and answers zero.
#[tauri::command]
#[specta::specta]
pub async fn system_insets(app: tauri::AppHandle) -> Result<SystemInsets, IpcError> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let handle = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
        return handle
            .system_insets()
            .await
            .map_err(IpcError::internal)
            .map(|view| SystemInsets {
                top: view.top,
                bottom: view.bottom,
            });
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(SystemInsets { top: 0, bottom: 0 })
    }
}

/// Turn desktop notifications on or off and persist the choice.
#[tauri::command]
#[specta::specta]
pub fn set_notifications(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_notifications(enabled)
}

/// Accept incoming transfers from trusted peers without asking. Off by
/// default: receiving a file is a decision.
#[tauri::command]
#[specta::specta]
pub fn set_auto_accept(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_auto_accept(enabled)
}

/// Turn the automatic clipboard watcher on or off and persist the choice. It is
/// off by default; when it is on the backend reads the clipboard every 300 ms
/// and offers a stable, locally-authored value to online trusted devices.
#[tauri::command]
#[specta::specta]
pub fn set_auto_clipboard(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_auto_clipboard(enabled)
}

/// Register or remove the login item and persist the choice. Off by default; the
/// login item starts the app with --hidden so it lands in the tray.
#[tauri::command]
#[specta::specta]
pub fn set_autostart(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, IpcError> {
    state.set_autostart(enabled)
}

/// Choose the local interface and address, persist the choice, and rebind the
/// LAN listener immediately. Automatic mode lets the picker follow the network;
/// manual mode pins one address and reports 2003 when it disappears instead of
/// falling back silently.
#[tauri::command]
#[specta::specta]
pub fn set_network_selection(
    state: State<'_, AppState>,
    mode: String,
    interface_id: Option<String>,
    address: Option<String>,
) -> Result<NetworkStatusView, IpcError> {
    state.set_network_selection(&mode, interface_id, address)
}

/// Register a local file and return the opaque handle used to send it.
#[tauri::command]
#[specta::specta]
pub fn register_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<RegisteredFileView, IpcError> {
    state.transfer.register_file(&path)
}

/// Offer one registered file to a trusted device.
#[tauri::command]
#[specta::specta]
pub async fn start_transfer(
    state: State<'_, AppState>,
    device_id: String,
    file_id: String,
) -> Result<TransferSummary, IpcError> {
    state.transfer.start_transfer(&device_id, &file_id).await
}

/// Accept or reject an incoming transfer offer.
#[tauri::command]
#[specta::specta]
pub fn transfer_respond(
    state: State<'_, AppState>,
    transfer_id: String,
    accepted: bool,
) -> Result<TransferSummary, IpcError> {
    state.transfer.respond(&transfer_id, accepted)
}

/// Set or clear a device's automatic acceptance of incoming files.
///
/// `Inherit` is a real value, not a reset: it means the global setting decides,
/// which is how a user who never touched this device keeps behaving.
#[tauri::command]
#[specta::specta]
pub fn device_set_auto_accept(
    state: State<'_, AppState>,
    device_id: String,
    policy: DeviceAutoAccept,
) -> Result<(), IpcError> {
    state.device_set_auto_accept(&device_id, policy)
}

/// Open a completed receive with the platform's default application.
///
/// The path never crosses the IPC boundary: the frontend names a transfer, and
/// the runtime decides whether it is openable and where it sits.
#[tauri::command]
#[specta::specta]
pub fn transfer_open(state: State<'_, AppState>, transfer_id: String) -> Result<(), IpcError> {
    state.transfer.open_received(&transfer_id)
}

/// Pause, resume, cancel, or retry one transfer.
#[tauri::command]
#[specta::specta]
pub fn transfer_control(
    state: State<'_, AppState>,
    transfer_id: String,
    action: TransferAction,
) -> Result<TransferSummary, IpcError> {
    state.transfer.control(&transfer_id, action)
}

#[tauri::command]
#[specta::specta]
pub fn transfers_list(state: State<'_, AppState>) -> Result<Vec<TransferSummary>, IpcError> {
    Ok(state.transfer.snapshot())
}

pub fn specta_builder() -> Builder<Wry> {
    Builder::<Wry>::new()
        // All transmitted integers are process-local sequence numbers, byte
        // counts, or millisecond timestamps and stay far below JS's 2^53
        // exact-integer limit. Keep them as numbers instead of BigInt on the
        // WebView boundary.
        .dangerously_cast_bigints_to_number()
        .commands(collect_commands![
            engine_status,
            state_snapshot,
            events_subscribe,
            identity_rotate,
            pair_start,
            nearby_peers,
            pair_manual,
            pair_confirm,
            pair_cancel,
            trust_remove,
            clipboard_set_policy,
            chat_open,
            chat_send,
            chat_retry,
            clipboard_share,
            clipboard_accept,
            clipboard_reject,
            shell_set_labels,
            discovery_diagnostics,
            pick_file,
            pick_folder,
            pick_files,
            register_file,
            register_folder,
            start_transfer,
            start_folder,
            start_transfers,
            set_transfer_concurrency,
            set_device_name,
            set_auto_accept,
            set_notifications,
            set_receive_dir,
            pick_receive_dir,
            pick_source_file,
            app_log_tail,
            open_shared_source,
            system_insets,
            set_chrome_color,
            set_auto_clipboard,
            set_autostart,
            set_network_selection,
            transfer_respond,
            transfer_control,
            transfer_open,
            device_set_auto_accept,
            transfers_list
        ])
}
