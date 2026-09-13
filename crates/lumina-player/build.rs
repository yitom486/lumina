//! Link-only glue for `lumina-player`'s own targets (unit-test harness).
//!
//! Canonical mpv link/stage logic (DELAYLOAD, staging, pkg-config fallback)
//! lives in `apps/desktop/src-tauri/build.rs` and stays there: the app owns
//! native resources (L0 §5.12). This script only contributes the import-library
//! search path so `cargo test -p lumina-player` links on Windows dev machines.
//! `libmpv2-sys` emits `rustc-link-lib=mpv` itself; no staging is done here.

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dir = env::var("LUMINA_MPV_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        manifest_dir.join("../../apps/desktop/src-tauri/native/mpv")
    });
    println!("cargo:rerun-if-changed={}", dir.display());
    let dir = dir.canonicalize().unwrap_or(dir);
    if dir.join("mpv.lib").is_file() || dir.join("libmpv-2.dll").is_file() {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
}
