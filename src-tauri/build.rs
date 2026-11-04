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
    //
    // Two guards, and both are needed. `#[cfg(windows)]` is evaluated while
    // compiling this script, that is against the host, and the crate is only a
    // build-dependency on a Windows host - without it a Linux or macOS build
    // fails with "cannot find module or crate `embed_resource`". The callback
    // then checks the *target*: a cross build from Windows to Android must not
    // link a Windows .lib into the .so.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for_everything("test-resource.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embed application manifest");
    }
}
