fn main() {
    // tauri-build only reruns when the configuration changes, so replacing the
    // files under icons/ left the old artwork embedded in the binary. Watch the
    // directory itself so a new icon actually reaches the exe.
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=tauri.conf.json");

    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("run tauri build script");

    // tauri-build embeds its resources into binaries only. Link one shared
    // Common Controls v6 manifest into every target (including unit tests) so
    // the tray/menu code can resolve TaskDialogIndirect on Windows.
    // Target the Windows platform, not the build host: a cross build from
    // Windows to Android would otherwise link a Windows .lib into the .so.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for_everything("test-resource.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embed application manifest");
    }
}
