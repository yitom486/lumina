//! Load libmpv shared library before first FFI use (installer bundles under resources/mpv).

#[cfg(windows)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use tauri::AppHandle;
    use tauri::Manager;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};

    use crate::player::error::{PlayerError, PlayerErrorCode};

    pub fn libmpv_candidates(app: &AppHandle) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(path) = exe_dir_candidate("libmpv-2.dll") {
            candidates.push(path);
        }
        if let Ok(resource) = app.path().resource_dir() {
            candidates.push(resource.join("mpv").join("libmpv-2.dll"));
            candidates.push(resource.join("libmpv-2.dll"));
        }
        candidates
    }

    pub fn exe_dir_candidate(name: &str) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join(name))
    }

    pub fn ensure_libmpv_loaded(app: &AppHandle) -> Result<(), PlayerError> {
        if libmpv_already_loaded() {
            return Ok(());
        }

        for path in libmpv_candidates(app) {
            if !path.is_file() {
                continue;
            }
            if load_dll(&path).is_ok() {
                tracing::info!(path = %path.display(), "loaded libmpv runtime");
                return Ok(());
            }
        }

        Err(PlayerError::new(
            PlayerErrorCode::NativeWindowError,
            "播放组件未就绪，请重新安装应用",
            Some("libmpv-2.dll not found beside executable or bundled resources".into()),
        ))
    }

    fn libmpv_already_loaded() -> bool {
        let name = wide("libmpv-2.dll");
        unsafe {
            GetModuleHandleW(PCWSTR(name.as_ptr()))
                .map(|module| module != HMODULE::default())
                .unwrap_or(false)
        }
    }

    fn load_dll(path: &Path) -> Result<HMODULE, PlayerError> {
        let wide = wide_path(path);
        unsafe {
            LoadLibraryW(PCWSTR(wide.as_ptr())).map_err(|error| {
                PlayerError::new(
                    PlayerErrorCode::NativeWindowError,
                    "播放组件未就绪，请重新安装应用",
                    Some(format!("LoadLibraryW({}): {error}", path.display())),
                )
            })
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn exe_dir_candidate_ends_with_libmpv_dll() {
            let path = exe_dir_candidate("libmpv-2.dll").expect("current exe path");
            assert_eq!(
                path.file_name().and_then(|name| name.to_str()),
                Some("libmpv-2.dll")
            );
        }
    }
}

#[cfg(unix)]
mod unix {
    use std::path::PathBuf;

    use libloading::Library;
    use tauri::AppHandle;
    use tauri::Manager;

    use crate::player::error::PlayerError;

    pub fn libmpv_candidates(app: &AppHandle) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Ok(resource) = app.path().resource_dir() {
            let mpv_dir = resource.join("mpv");
            candidates.push(mpv_dir.join("libmpv.dylib"));
            candidates.push(mpv_dir.join("libmpv.2.dylib"));
            candidates.push(mpv_dir.join("libmpv.so"));
            candidates.push(mpv_dir.join("libmpv.so.2"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("libmpv.dylib"));
                candidates.push(dir.join("libmpv.so.2"));
            }
        }
        candidates
    }

    pub fn ensure_libmpv_loaded(app: &AppHandle) -> Result<(), PlayerError> {
        for path in libmpv_candidates(app) {
            if !path.is_file() {
                continue;
            }
            match unsafe { Library::new(&path) } {
                Ok(_library) => {
                    tracing::info!(path = %path.display(), "loaded libmpv runtime");
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "failed to load libmpv candidate");
                }
            }
        }

        // Linked against system libmpv at build time — assume loader can resolve it.
        Ok(())
    }
}

#[cfg(windows)]
pub use win::{ensure_libmpv_loaded, exe_dir_candidate, libmpv_candidates};

#[cfg(unix)]
pub use unix::{ensure_libmpv_loaded, libmpv_candidates};
