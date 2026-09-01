//! Load `libmpv-2.dll` before first libmpv FFI use (installer bundles it under resources).

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

    pub fn libmpv_dll_candidates(app: &AppHandle) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(path) = exe_dir_dll_candidate() {
            candidates.push(path);
        }
        if let Ok(resource) = app.path().resource_dir() {
            candidates.push(resource.join("libmpv-2.dll"));
        }
        candidates
    }

    pub fn exe_dir_dll_candidate() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join("libmpv-2.dll"))
    }

    pub fn ensure_libmpv_loaded(app: &AppHandle) -> Result<(), PlayerError> {
        if libmpv_already_loaded() {
            return Ok(());
        }

        for path in libmpv_dll_candidates(app) {
            if !path.is_file() {
                continue;
            }
            if load_dll(&path).is_ok() {
                tracing::info!(path = %path.display(), "loaded libmpv-2.dll");
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
            let path = exe_dir_dll_candidate().expect("current exe path");
            assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("libmpv-2.dll"));
        }
    }
}

#[cfg(windows)]
pub use win::{ensure_libmpv_loaded, exe_dir_dll_candidate, libmpv_dll_candidates};

#[cfg(not(windows))]
pub fn ensure_libmpv_loaded(_app: &tauri::AppHandle) -> Result<(), crate::player::error::PlayerError> {
    Ok(())
}
