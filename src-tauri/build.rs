use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();
    link_and_copy_libmpv();
}

fn link_and_copy_libmpv() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let mpv_dir = manifest_dir.join("native").join("mpv");
    let dll = mpv_dir.join("libmpv-2.dll");
    let import_lib = mpv_dir.join("mpv.lib");

    println!("cargo:rerun-if-changed={}", mpv_dir.display());
    println!("cargo:rerun-if-changed={}", dll.display());
    println!("cargo:rerun-if-changed={}", import_lib.display());

    if !import_lib.exists() || !dll.exists() {
        println!(
            "cargo:warning=libmpv files missing in {}; expected mpv.lib and libmpv-2.dll",
            mpv_dir.display()
        );
        return;
    }

    println!("cargo:rustc-link-search=native={}", mpv_dir.display());
    println!("cargo:rustc-link-lib=dylib=mpv");

    let mut exe_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    for _ in 0..3 {
        exe_dir.pop();
    }

    place_dll(&dll, &exe_dir.join("libmpv-2.dll"));
    place_dll(&dll, &exe_dir.join("deps").join("libmpv-2.dll"));
}

fn place_dll(src: &Path, dest: &Path) {
    if dest.exists() {
        if let (Ok(src_meta), Ok(dest_meta)) = (fs::metadata(src), fs::metadata(dest)) {
            if src_meta.len() == dest_meta.len() {
                return;
            }
        }
    }

    if let Some(parent) = dest.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            println!("cargo:warning=cannot create {}: {error}", parent.display());
            return;
        }
    }

    let _ = fs::remove_file(dest);
    if fs::hard_link(src, dest).is_ok() {
        return;
    }
    if let Err(error) = fs::copy(src, dest) {
        println!(
            "cargo:warning=failed to copy libmpv-2.dll to {}: {error}",
            dest.display()
        );
    }
}
