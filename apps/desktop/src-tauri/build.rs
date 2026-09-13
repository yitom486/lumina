use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();
    link_and_stage_libmpv();
}

fn link_and_stage_libmpv() {
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        link_windows_mpv();
    } else if target.contains("darwin") {
        link_unix_mpv("libmpv.dylib");
    } else if target.contains("linux") {
        link_unix_mpv("libmpv.so");
    }
}

fn link_windows_mpv() {
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

    println!("cargo:rustc-link-arg=/DELAYLOAD:libmpv-2.dll");
    println!("cargo:rustc-link-lib=delayimp");
    println!("cargo:rustc-link-search=native={}", mpv_dir.display());
    println!("cargo:rustc-link-lib=dylib=mpv");

    let exe_dir = profile_dir();
    stage_file(&dll, &exe_dir.join("libmpv-2.dll"));
    stage_file(&dll, &exe_dir.join("deps").join("libmpv-2.dll"));
    stage_runtime_dir(&mpv_dir, &dll);
}

fn link_unix_mpv(expected_name: &str) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let mpv_dir = manifest_dir.join("native").join("mpv");
    let runtime_dir = mpv_dir.join("runtime");

    println!("cargo:rerun-if-changed={}", mpv_dir.display());
    println!("cargo:rerun-if-changed={}", runtime_dir.display());

    if let Some(local) = find_file(&mpv_dir, expected_name) {
        link_with_search_path(&mpv_dir);
        stage_runtime_dir(&mpv_dir, &local);
        return;
    }

    if find_file(&runtime_dir, expected_name).is_some() {
        link_with_search_path(&runtime_dir);
        return;
    }

    if let Ok(lib) = pkg_config::Config::new().probe("mpv") {
        for path in lib.link_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
        println!("cargo:rustc-link-lib=dylib=mpv");
        return;
    }

    println!(
        "cargo:warning=libmpv not found; place {expected_name} under {} or install pkg-config mpv",
        mpv_dir.display()
    );
}

fn link_with_search_path(dir: &Path) {
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=dylib=mpv");
    if env::var("TARGET").unwrap_or_default().contains("darwin") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources/mpv");
    } else {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../Resources/mpv");
    }
}

fn stage_runtime_dir(source_dir: &Path, file: &Path) {
    let runtime_dir = source_dir.join("runtime");
    let Some(name) = file.file_name() else {
        return;
    };
    let dest = runtime_dir.join(name);
    // Already living in runtime/ (common on macOS/Linux CI after brew copy).
    if same_path(file, &dest) {
        return;
    }
    let _ = fs::create_dir_all(&runtime_dir);
    stage_file(file, &dest);
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn profile_dir() -> PathBuf {
    let mut exe_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    for _ in 0..3 {
        exe_dir.pop();
    }
    exe_dir
}

fn find_file(dir: &Path, expected_name: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let expected = dir.join(expected_name);
    if expected.is_file() {
        return Some(expected);
    }
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name()?.to_string_lossy();
        if name.starts_with("libmpv") {
            return Some(path);
        }
    }
    None
}

fn stage_file(src: &Path, dest: &Path) {
    if !src.is_file() || same_path(src, dest) {
        return;
    }
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

    // Prefer copy over hard_link: CI dylibs may be read-only or cross-device.
    match fs::copy(src, dest) {
        Ok(_) => {}
        Err(error) => {
            println!(
                "cargo:warning=failed to copy {} to {}: {error}",
                src.display(),
                dest.display()
            );
        }
    }
}
