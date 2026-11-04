fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    // SAFETY: build scripts are single-threaded here; no other thread reads PROTOC.
    unsafe { std::env::set_var("PROTOC", protoc) };
    let mut config = prost_build::Config::new();
    config
        .compile_protos(&["proto/fluxy.proto"], &["proto"])
        .expect("compile fluxy.proto");
    println!("cargo:rerun-if-changed=proto/fluxy.proto");
}
