//! Embed `assets/icon.ico` into the Windows executable.
//!
//! The runtime window icon is set separately in `main.rs`; this resource is what
//! Explorer and the file-properties dialog show before the app is launched.

fn main() {
    println!("cargo:rerun-if-changed=windows.rc");
    println!("cargo:rerun-if-changed=../../assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    embed_resource::compile("windows.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed the Windows icon resource");
}
