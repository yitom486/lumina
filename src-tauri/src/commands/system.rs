//! Cross-cutting system commands (crash/log export).

use std::path::PathBuf;

/// Directory for daily-rotated file logs (`lumina.log.<date>`).
/// Falls back to the OS temp dir when no data dir is known — never fails,
/// so a missing data dir can never hide diagnostics.
pub(crate) fn log_dir() -> PathBuf {
    data_dir()
        .map(|base| base.join("lumina").join("logs"))
        .unwrap_or_else(std::env::temp_dir)
}

/// Crash/log export entry point for the UI. Infallible by design.
#[tauri::command]
pub async fn system_log_dir() -> String {
    log_dir().to_string_lossy().into_owned()
}

fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_never_fails_and_points_at_lumina_logs() {
        let dir = log_dir();
        assert!(!dir.as_os_str().is_empty());
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        // Either the lumina logs dir or the OS temp fallback.
        assert!(name == "logs" || dir == std::env::temp_dir());
    }

    #[test]
    fn command_returns_usable_path() {
        let path = tauri::async_runtime::block_on(system_log_dir());
        assert!(!path.trim().is_empty());
    }
}
