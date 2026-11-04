fn main() {
    // Every command has to be listed here or the permission it generates does
    // not exist and the WebView's invoke is rejected with "command not found".
    tauri_plugin::Builder::new(&[
        "secret_get",
        "secret_set",
        "keystore_available",
        "fs_pick_source",
        "fs_open_source",
        "fs_export",
        "storage_available",
        "device_name",
        "local_network_granted",
        "clipboard_read",
        "clipboard_write",
        "shared_files",
        "foreground_start",
        "foreground_stop",
        "system_insets",
        "set_chrome_color",
        "local_network_info",
        "discovery_start",
        "discovery_stop",
        "discovery_peers",
    ])
    .android_path("android")
    .ios_path("ios")
    .try_build()
    .expect("build the fluxy mobile plugin");
}
