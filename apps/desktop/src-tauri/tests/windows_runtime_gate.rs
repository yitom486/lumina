//! Standalone Windows runtime gate.
//!
//! This test intentionally does not link the `lumina-app` library. The normal
//! `--lib` harness links libmpv into the test executable, so a native loader
//! failure can prevent all Rust assertions from starting. This gate loads the
//! staged DLL directly and reports a useful Win32 error instead.

#[cfg(windows)]
mod windows_runtime {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(path: *const u16) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }

    fn staged_libmpv_path() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let deps_dir = executable.parent()?;
        let debug_dir = deps_dir.parent()?;
        let candidates = [
            deps_dir.join("libmpv-2.dll"),
            debug_dir.join("libmpv-2.dll"),
            debug_dir.join("mpv").join("libmpv-2.dll"),
        ];
        candidates.into_iter().find(|path| path.is_file())
    }

    fn load_library(path: &Path) -> Result<*mut c_void, i32> {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default());
        }
        Ok(module)
    }

    #[test]
    fn staged_libmpv_loads_with_the_current_windows_runtime() {
        let Some(path) = staged_libmpv_path() else {
            panic!(
                "Windows native runtime gate: libmpv-2.dll is not staged; build/copy the matching x64 libmpv runtime first"
            );
        };

        let module = load_library(&path).unwrap_or_else(|error| {
            panic!(
                "Windows native runtime gate: libmpv-2.dll could not be loaded (Win32 error {error}); check the x64 libmpv dependency/runtime installation"
            )
        });

        let freed = unsafe { FreeLibrary(module) };
        assert_ne!(freed, 0, "Windows native runtime gate: FreeLibrary failed");
    }
}

#[cfg(not(windows))]
#[test]
fn windows_runtime_gate_is_not_applicable_on_non_windows() {}
