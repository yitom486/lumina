//! Shared resolution of project-local FFmpeg tools.

use std::path::PathBuf;

use crate::media::error::MediaError;

/// 解析 ffprobe 路径，可选传入 Tauri resource_dir（打包后更可靠）。
pub fn resolve_ffprobe_with(resource_dir: Option<&PathBuf>) -> Result<PathBuf, MediaError> {
    resolve_tool("ffprobe", "ffprobe.exe", resource_dir)
}

pub fn resolve_ffprobe() -> Result<PathBuf, MediaError> {
    resolve_tool("ffprobe", "ffprobe.exe", None)
}

pub fn resolve_ffmpeg() -> Result<PathBuf, MediaError> {
    resolve_tool("ffmpeg", "ffmpeg.exe", None)
}

/// 解析 ffmpeg 路径，可选传入 Tauri resource_dir。
pub fn resolve_ffmpeg_with(resource_dir: Option<&PathBuf>) -> Result<PathBuf, MediaError> {
    resolve_tool("ffmpeg", "ffmpeg.exe", resource_dir)
}

fn resolve_tool(
    unix_name: &str,
    windows_name: &str,
    resource_dir: Option<&PathBuf>,
) -> Result<PathBuf, MediaError> {
    for path in tool_candidates(unix_name, windows_name, resource_dir) {
        if path.is_file() {
            tracing::debug!(path = %path.display(), tool = unix_name, "resolved ffmpeg tool");
            return Ok(path);
        }
    }
    Err(MediaError::probe_not_found(Some(&format!(
        "place {windows_name} under native/ffmpeg/ (tauri resources) or alongside the executable"
    ))))
}

fn tool_candidates(
    unix_name: &str,
    windows_name: &str,
    resource_dir: Option<&PathBuf>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 0. Tauri resource_dir（打包后最可靠）：resources/ffmpeg/
    if let Some(res) = resource_dir {
        paths.push(res.join("ffmpeg").join(windows_name));
        paths.push(res.join("ffmpeg").join(unix_name));
    }

    // 1. 开发期：CARGO_MANIFEST_DIR/native/ffmpeg/
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    paths.push(manifest.join("native").join("ffmpeg").join(windows_name));
    paths.push(manifest.join("native").join("ffmpeg").join(unix_name));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // 2. Windows/Linux 安装后：exe 同目录或 ffmpeg/ 子目录
            paths.push(dir.join(windows_name));
            paths.push(dir.join(unix_name));
            paths.push(dir.join("ffmpeg").join(windows_name));
            paths.push(dir.join("ffmpeg").join(unix_name));

            // 3. macOS .app bundle：exe 在 MacOS/，resources 在 ../Resources/
            if let Some(parent) = dir.parent() {
                let resources = parent.join("Resources");
                paths.push(resources.join("ffmpeg").join(windows_name));
                paths.push(resources.join("ffmpeg").join(unix_name));
            }
        }
    }
    paths
}
