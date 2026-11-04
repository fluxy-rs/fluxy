pub mod chat;
pub mod clipboard;
pub mod commands;
pub mod contract;
pub mod network;
pub mod notify;
pub mod startup;
pub mod transfer;

use std::time::Duration;

use fluxy_app::{AppPaths, Engine};
use tauri::Manager;
#[cfg(desktop)]
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
#[cfg(desktop)]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::contract::AppState;
#[cfg(desktop)]
use crate::notify::{TrayMenu, focus_main};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(debug_assertions)]
    if export_bindings_if_requested() {
        return;
    }

    // A login start passes --hidden so the app lands in the tray instead of
    // throwing a window in the user's face.
    let start_hidden = std::env::args().any(|arg| arg == startup::HIDDEN_FLAG);

    // Single-instance is a desktop concern: on Android the OS already keeps one
    // task per app, and the crate has no Android build at all.
    #[cfg(desktop)]
    let builder = tauri::Builder::default()
        // Must be the first plugin: it decides whether this process becomes the
        // running instance or hands its launch over to it.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    #[cfg(not(desktop))]
    let builder = tauri::Builder::default();
    let app = builder
        .plugin(tauri_plugin_notification::init())
        // The barcode scanner plugin is mobile-only (the crate is #![cfg(mobile)]),
        // so registering it unconditionally breaks the desktop build.
        .plugin(desktop_or_mobile_barcode())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fluxy_mobile::init())
        .invoke_handler(commands::specta_builder().invoke_handler())
        .on_window_event(|window, event| {
            // M3-04: closing the window must not stop the app. The tray keeps
            // running, so the network, chat, and clipboard keep receiving until
            // the user picks Quit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.should_hide_on_close() {
                    api.prevent_close();
                    if let Err(error) = window.hide() {
                        tracing::warn!(%error, "could not hide the window to the tray");
                    }
                }
                return;
            }
            // Window-level drag and drop keeps the local path in Rust: the
            // WebView only receives opaque file handles and base names.
            let tauri::WindowEvent::DragDrop(drag) = event else {
                return;
            };
            let state = window.state::<AppState>();
            match drag {
                tauri::DragDropEvent::Enter { .. } | tauri::DragDropEvent::Over { .. } => {
                    state.transfer.set_drag_hover(true);
                }
                tauri::DragDropEvent::Leave => state.transfer.set_drag_hover(false),
                tauri::DragDropEvent::Drop { paths, .. } => {
                    state.transfer.accept_drop(paths.clone());
                }
                _ => {}
            }
        })
        .setup(move |app| {
            // The data directory has to come from the platform: desktop resolves
            // it from the OS, Android only knows its own files directory through
            // the activity. Both are available here, and no command can arrive
            // before this hook finishes.
            let paths = resolve_paths(app).map_err(std::io::Error::other)?;
            fluxy_app::logging::init(&paths.log_dir)
                .map_err(|error| std::io::Error::other(format!("initialize logging: {error}")))?;
            #[cfg(target_os = "android")]
            let identity_file = paths.identity_file.clone();
            let engine = Engine::load(paths)
                .map_err(|error| std::io::Error::other(format!("load engine: {error}")))?;
            app.manage(AppState::new(engine));

            #[cfg(target_os = "android")]
            install_android_file_manager(app);
            #[cfg(target_os = "android")]
            use_android_receive_dir(app);
            #[cfg(all(target_os = "android", debug_assertions))]
            probe_picker(app);

            // Android 13 gates notifications behind a runtime permission. The
            // manifest only declares it; until the user grants it the system
            // drops every notification silently, which is what made a message
            // arrive with no alert at all. Ask once, after the plugin exists.

            #[cfg(target_os = "android")]
            {
                use tauri_plugin_notification::NotificationExt;
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match handle.notification().request_permission() {
                        Ok(state) => tracing::info!(?state, "notification permission requested"),
                        Err(error) => {
                            tracing::warn!(%error, "could not request notification permission")
                        }
                    }
                });
            }

            let state = app.state::<AppState>();
            state.transfer.set_app_handle(app.handle().clone());
            // The notifier is the bridge between the chat/transfer runtime and
            // the OS. On desktop the tray installs it; a phone has no tray, so
            // without this every notification returned early and nothing was
            // ever shown - a message arrived in complete silence.
            #[cfg(not(desktop))]
            state.notifier.attach(app.handle().clone());
            #[cfg(target_os = "android")]
            crate::clipboard::set_app_handle(app.handle().clone());
            #[cfg(target_os = "android")]
            {
                // The system knows what the user calls this phone; the Rust
                // default is only a placeholder, and every Android build would
                // otherwise advertise that placeholder over mDNS.
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Manager;
                    let plugin = handle
                        .state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
                    let Ok(name) = plugin.device_name().await else {
                        return;
                    };
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        return;
                    }
                    let state = handle.state::<AppState>();
                    // Never overwrite a name the user chose.
                    let placeholder = fluxy_app::config::default_device_name();
                    if state.device_name().as_deref() != Some(placeholder.as_str()) {
                        return;
                    }
                    match state.set_device_name(name.clone()) {
                        Ok(_) => tracing::info!(device_name = %name, "adopted the system device name"),
                        Err(error) => {
                            tracing::warn!(code = %error.code, "could not adopt the system device name")
                        }
                    }
                });
            }
            #[cfg(desktop)]
            setup_tray(app, &state)?;
            if !start_hidden && let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            // Checkpoints outlive the process; surface them as interrupted
            // transfers so the user can resume or drop them.
            state.transfer.restore_interrupted();
            state.reconcile_autostart();
            state.start_network();
            // Reads the clipboard only while the automatic-send switch is on.
            state.start_clipboard_watch();
            state.emit_started();
            spawn_heartbeat(app.handle().clone());
            #[cfg(target_os = "android")]
            seal_identity(app.handle().clone(), identity_file);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let state = app_handle.state::<AppState>();
            state.stop_network();
            state.emit_stopped();
        }
    });
}

#[cfg(desktop)]
/// Create the tray icon and native menu. The labels are placeholders until the
/// frontend pushes the current i18n catalog through shell_set_labels.
///
/// The menu is the shell's only always-available surface, so it carries the two
/// switches (automatic clipboard sending and the login item), a help entry, and
/// the quit action. Check states are seeded from the persisted preferences.
fn setup_tray(app: &tauri::App, state: &AppState) -> tauri::Result<()> {
    let labels = state.notifier.labels();
    let settings = state.snapshot().map(|snapshot| snapshot.settings).ok();
    let open = MenuItem::with_id(app, "tray-open", &labels.open, true, None::<&str>)?;
    let auto_clipboard = CheckMenuItem::with_id(
        app,
        "tray-auto-clipboard",
        &labels.auto_clipboard,
        true,
        settings
            .as_ref()
            .map(|settings| settings.auto_clipboard)
            .unwrap_or(false),
        None::<&str>,
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "tray-autostart",
        &labels.autostart,
        true,
        settings
            .as_ref()
            .map(|settings| settings.autostart)
            .unwrap_or(false),
        None::<&str>,
    )?;
    let help = MenuItem::with_id(app, "tray-help", &labels.help, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", &labels.quit, true, None::<&str>)?;
    let before_switches = PredefinedMenuItem::separator(app)?;
    let before_actions = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &before_switches,
            &auto_clipboard,
            &autostart,
            &before_actions,
            &help,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("fluxy-tray")
        .menu(&menu)
        .tooltip(app.package_info().name.clone())
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let state = app.state::<AppState>();
            match event.id().as_ref() {
                "tray-open" => focus_main(app),
                "tray-auto-clipboard" => {
                    let current = state
                        .snapshot()
                        .map(|snapshot| snapshot.settings.auto_clipboard)
                        .unwrap_or(false);
                    match state.set_auto_clipboard(!current) {
                        Ok(settings) => state
                            .notifier
                            .set_auto_clipboard_checked(settings.auto_clipboard),
                        Err(error) => {
                            tracing::warn!(code = %error.code, "tray clipboard switch failed");
                            state.notifier.set_auto_clipboard_checked(current);
                        }
                    }
                }
                "tray-autostart" => {
                    let current = state
                        .snapshot()
                        .map(|snapshot| snapshot.settings.autostart)
                        .unwrap_or(false);
                    match state.set_autostart(!current) {
                        Ok(settings) => state.notifier.set_autostart_checked(settings.autostart),
                        Err(error) => {
                            tracing::warn!(code = %error.code, "tray autostart switch failed");
                            state.notifier.set_autostart_checked(current);
                        }
                    }
                }
                "tray-help" => {
                    focus_main(app);
                    state.request_help();
                }
                "tray-quit" => {
                    // A deliberate quit is the only close that ends the process.
                    state.begin_quit();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    state.notifier.attach(
        app.handle().clone(),
        TrayMenu {
            open,
            auto_clipboard,
            autostart,
            help,
            quit,
        },
    );
    tracing::info!("tray icon created");
    Ok(())
}

/// Install the descriptor-backed file manager on Android. Receiving keeps the
/// sandbox path, sending goes through the SAF descriptors the plugin hands over.
#[cfg(target_os = "android")]
fn install_android_file_manager(app: &tauri::App) {
    let state = app.state::<AppState>();
    let manager = fluxy_core::files::AndroidFileAccessManager::new(state.transfer.receive_root());
    state.transfer.set_manager(std::sync::Arc::new(manager));
    // The network runtime needs the plugin too: it asks for the gateway the
    // fallback probe should aim at, and later for discovered peers.
    let handle: tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry> = app
        .state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>()
        .inner()
        .clone();
    state.network.set_mobile(std::sync::Arc::new(handle));
    tracing::info!("android file manager installed");
}

/// Debug-only: if `files/probe-picker.txt` exists, open the document picker once
/// and log how the round trip ends. It exists so the picker can be checked from
/// a headless emulator without a UI entry point; the file is deleted first so a
/// second launch does not pop the picker again.
#[cfg(all(target_os = "android", debug_assertions))]
fn probe_picker(app: &tauri::App) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let Some(dir) = state.engine_data_dir() else {
        return;
    };
    let marker = dir.join("probe-picker.txt");
    if !marker.exists() {
        return;
    }
    let _ = std::fs::remove_file(&marker);
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let mobile = handle.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
        match mobile.pick_source().await {
            Ok(opened) if opened.cancelled => tracing::info!("picker probe: cancelled"),
            Ok(opened) => tracing::info!(
                name = %opened.name,
                size = opened.size,
                "picker probe: opened a document"
            ),
            Err(error) => tracing::warn!(%error, "picker probe: failed"),
        }
    });
}

/// Point received files at the directory the plugin reported. The marker is
/// written by the Kotlin side before this hook runs, so the read is plain.
#[cfg(target_os = "android")]
fn use_android_receive_dir(app: &tauri::App) {
    let state = app.state::<AppState>();
    let marker = match state.engine_data_dir() {
        Some(dir) => dir.join("receive-dir.txt"),
        None => return,
    };
    let Ok(contents) = std::fs::read_to_string(&marker) else {
        return;
    };
    let directory = std::path::PathBuf::from(contents.trim());
    if directory.as_os_str().is_empty() {
        return;
    }
    state.transfer.set_receive_root(directory);
    tracing::info!(path = %contents.trim(), "android receive directory set");
}

/// Move the plain identity into the platform keystore and drop the plain copy.
/// Runs after the engine has loaded it, so a crash during sealing leaves a
/// working plain file rather than no identity at all.
#[cfg(target_os = "android")]
fn seal_identity(app: tauri::AppHandle, identity_file: std::path::PathBuf) {
    use tauri::Manager;

    tauri::async_runtime::spawn(async move {
        let Ok(identity) = std::fs::read_to_string(&identity_file) else {
            return;
        };
        let device_id = serde_json::from_str::<serde_json::Value>(&identity)
            .ok()
            .and_then(|value| {
                value
                    .get("device_id")
                    .and_then(|id| id.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        let mobile = app.state::<tauri_plugin_fluxy_mobile::FluxyMobile<tauri::Wry>>();
        match mobile.secret_set(identity).await {
            Ok(()) => match std::fs::remove_file(&identity_file) {
                Ok(()) => {
                    tracing::info!(%device_id, "identity sealed in the platform keystore")
                }
                Err(error) => tracing::warn!(%error, "identity sealed but the plain file stayed"),
            },
            Err(error) => tracing::warn!(%error, "could not seal the identity"),
        }
    });
}

/// Where the app keeps its data. Desktop asks the OS; Android has to ask the
/// activity, so this can only run from the setup hook.
fn resolve_paths(app: &tauri::App) -> Result<AppPaths, String> {
    #[cfg(desktop)]
    {
        let _ = app;
        AppPaths::resolve().map_err(|error| error.to_string())
    }
    #[cfg(mobile)]
    {
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("resolve the app data directory: {error}"))?;
        Ok(AppPaths::from_root(directory))
    }
}

#[cfg(debug_assertions)]
fn export_bindings_if_requested() -> bool {
    if std::env::var_os("FLUXY_EXPORT_BINDINGS").is_none() {
        return false;
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    commands::specta_builder()
        .export(specta_typescript::Typescript::default(), &path)
        .expect("export TypeScript bindings");
    println!("wrote {}", path.display());
    true
}

/// Keep a low-rate engine heartbeat flowing so the WebView can detect a dead
/// channel and so the event sequence remains observable while idle.
fn spawn_heartbeat(handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        interval.tick().await;
        loop {
            interval.tick().await;
            let state = handle.state::<AppState>();
            if state.hub.receiver_count() == 0 {
                continue;
            }
            state.emit_heartbeat();
        }
    });
}

/// The barcode scanner crate is #![cfg(mobile)], so it has no symbols at all on
/// a desktop build. Returning an empty plugin keeps the builder chain identical
/// and costs nothing where scanning is unavailable anyway.
fn desktop_or_mobile_barcode<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    #[cfg(mobile)]
    {
        tauri_plugin_barcode_scanner::init()
    }
    #[cfg(not(mobile))]
    {
        tauri::plugin::Builder::new("barcode-scanner").build()
    }
}
