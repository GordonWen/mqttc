//! Compile the Slint UI and, on Windows targets, embed `assets/icon.ico` plus DWM/user32.
//! Darwin binaries must be built on a Mac; this script does not cross-compile for macOS.
//!
//! Cargo runs this file *before* compiling `src/`. `slint_build::compile` writes Rust
//! into `OUT_DIR`; `src/main.rs` pulls it in with `slint::include_modules!()`.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    slint_build::compile("ui/app-window.slint").unwrap();
    embed_windows_icon();
}

fn embed_windows_icon() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let res_path = out_dir.join("mqttc-icon.res");
    let windres = windows_windres();

    // Tell Cargo to re-run this script when the icon sources change.
    println!("cargo:rerun-if-changed=assets/windows.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon.png");

    let status = Command::new(&windres)
        .current_dir(manifest_dir.join("assets"))
        .args(["windows.rc", "-O", "coff", "-o"])
        .arg(&res_path)
        .status()
        .unwrap_or_else(|err| panic!("failed to run {windres}: {err}"));
    if !status.success() {
        panic!("{windres} failed with {status}");
    }

    // Link the compiled `.res` (icon) and the Win32 libs used by title-bar coloring.
    println!("cargo:rustc-link-arg={}", res_path.display());
    println!("cargo:rustc-link-lib=dwmapi");
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=advapi32");
}

fn windows_windres() -> String {
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows-gnu") {
        if target.starts_with("aarch64") {
            "aarch64-w64-mingw32-windres".to_string()
        } else {
            "x86_64-w64-mingw32-windres".to_string()
        }
    } else {
        "windres".to_string()
    }
}
