//! Shared resolution of project-local FFmpeg tools.

use std::path::PathBuf;

use crate::media::error::MediaError;

pub fn resolve_ffprobe() -> Result<PathBuf, MediaError> {
    resolve_tool("ffprobe", "ffprobe.exe")
}

pub fn resolve_ffmpeg() -> Result<PathBuf, MediaError> {
    resolve_tool("ffmpeg", "ffmpeg.exe")
}

fn resolve_tool(unix_name: &str, windows_name: &str) -> Result<PathBuf, MediaError> {
    for path in tool_candidates(unix_name, windows_name) {
        if path.is_file() {
            tracing::debug!(path = %path.display(), tool = unix_name, "resolved ffmpeg tool");
            return Ok(path);
        }
    }
    Err(MediaError::probe_not_found(Some(&format!(
        "place {windows_name} under src-tauri/native/ffmpeg/ (see README)"
    ))))
}

fn tool_candidates(unix_name: &str, windows_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    paths.push(manifest.join("native").join("ffmpeg").join(windows_name));
    paths.push(manifest.join("native").join("ffmpeg").join(unix_name));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(windows_name));
            paths.push(dir.join(unix_name));
            paths.push(dir.join("ffmpeg").join(windows_name));
            paths.push(dir.join("ffmpeg").join(unix_name));
        }
    }
    paths
}
