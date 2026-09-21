//! Cross-cutting system commands (crash/log export and startup diagnostics).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use serde::Serialize;

const CRASH_MARKER_FILE: &str = "last-session.json";
static PREVIOUS_UNCLEAN_EXIT: OnceLock<bool> = OnceLock::new();
static CRASH_PHASE: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStartupNotice {
    pub code: &'static str,
    pub message: &'static str,
    pub can_retry: bool,
}

/// Start a diagnostic session before native playback is initialized.
///
/// The marker is intentionally small and contains no media path, subtitle
/// text, agent content, or process stderr.  It stays unclean until the normal
/// shutdown path marks it clean, so an access violation that bypasses Rust's
/// panic machinery is still visible on the next launch.
pub(crate) fn initialize_crash_diagnostics() {
    let previous_unclean = read_unclean_marker();
    let _ = PREVIOUS_UNCLEAN_EXIT.set(previous_unclean);
    write_marker(false, None, None, None);
    install_native_exception_filter();
    if previous_unclean {
        tracing::warn!("previous Lumina session ended unexpectedly; recovery notice is available");
    }
}

pub(crate) fn mark_clean_shutdown() {
    write_marker(true, None, None, None);
}

pub(crate) fn set_crash_phase(phase: &'static str) {
    CRASH_PHASE.store(phase_code(phase), Ordering::Relaxed);
}

/// User-facing startup projection.  Diagnostics remain in the log/marker;
/// the UI receives only a stable business message.
#[tauri::command]
pub fn system_startup_notice() -> Option<SystemStartupNotice> {
    PREVIOUS_UNCLEAN_EXIT
        .get()
        .copied()
        .and_then(startup_notice)
}

fn startup_notice(previous_unclean: bool) -> Option<SystemStartupNotice> {
    previous_unclean.then_some(SystemStartupNotice {
        code: "NativePlaybackCrashed",
        message: "上次播放进程异常退出。可以重试播放；如果问题反复出现，请关闭硬件解码后再试。",
        can_retry: true,
    })
}

fn marker_path() -> PathBuf {
    log_dir().join(CRASH_MARKER_FILE)
}

fn read_unclean_marker() -> bool {
    let Ok(contents) = std::fs::read_to_string(marker_path()) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };
    matches!(
        value.get("clean").and_then(serde_json::Value::as_bool),
        Some(false)
    )
}

fn write_marker(
    clean: bool,
    exception_code: Option<String>,
    fault_address: Option<usize>,
    thread_id: Option<u32>,
) {
    let path = marker_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let marker = serde_json::json!({
        "version": 1,
        "clean": clean,
        "phase": phase_name(CRASH_PHASE.load(Ordering::Relaxed)),
        "exceptionCode": exception_code,
        "faultAddress": fault_address.map(|address| format!("0x{address:X}")),
        "threadId": thread_id,
    });
    if let Ok(contents) = serde_json::to_vec(&marker) {
        // The marker is deliberately tiny. Direct replacement works on
        // Windows too, where rename does not overwrite an existing file.
        let _ = std::fs::write(path, contents);
    }
}

fn phase_code(phase: &str) -> u8 {
    match phase {
        "player_attach" => 1,
        "player_poll" => 2,
        "shutdown" => 3,
        _ => 0,
    }
}

fn phase_name(code: u8) -> &'static str {
    match code {
        1 => "player_attach",
        2 => "player_poll",
        3 => "shutdown",
        _ => "startup",
    }
}

#[cfg(windows)]
fn install_native_exception_filter() {
    use windows::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter;

    unsafe {
        let _ = SetUnhandledExceptionFilter(Some(native_exception_filter));
    }
}

#[cfg(not(windows))]
fn install_native_exception_filter() {}

#[cfg(windows)]
unsafe extern "system" fn native_exception_filter(
    exception_info: *const windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) -> i32 {
    use windows::Win32::System::Threading::GetCurrentThreadId;

    let (exception_code, fault_address) = if exception_info.is_null() {
        (None, None)
    } else {
        let record = (*exception_info).ExceptionRecord;
        if record.is_null() {
            (None, None)
        } else {
            (
                Some(format!("0x{:08X}", (*record).ExceptionCode.0 as u32)),
                Some((*record).ExceptionAddress as usize),
            )
        }
    };
    let thread_id = Some(GetCurrentThreadId());
    write_marker(false, exception_code.clone(), fault_address, thread_id);
    tracing::error!(
        exception_code = ?exception_code,
        fault_address = ?fault_address,
        thread_id = ?thread_id,
        phase = phase_name(CRASH_PHASE.load(Ordering::Relaxed)),
        "native exception captured before process termination"
    );
    0
}

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

pub(crate) fn data_dir() -> Option<PathBuf> {
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

    #[test]
    fn startup_notice_is_safe_and_business_facing() {
        let notice = startup_notice(true).expect("unclean session should produce a notice");
        assert_eq!(notice.code, "NativePlaybackCrashed");
        assert!(notice.message.contains("异常退出"));
        assert!(!notice.message.contains("0xc0000005"));
        assert!(!notice.message.contains("DLL"));
        assert!(startup_notice(false).is_none());
    }
}
