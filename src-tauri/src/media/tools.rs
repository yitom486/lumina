//! Shared resolution of project-local FFmpeg tools.

use std::path::PathBuf;

use crate::media::error::MediaError;
use crate::media::model::MediaToolStatus;

/// Lightweight ffprobe presence check (no media file needed) for startup/settings UI.
/// Missing tool is data (`available: false`), never an error.
pub fn tool_status(resource_dir: Option<&PathBuf>) -> MediaToolStatus {
    match resolve_ffprobe_with(resource_dir) {
        Ok(_) => MediaToolStatus {
            available: true,
            message: "媒体分析已就绪".into(),
            hint: None,
        },
        Err(error) => {
            tracing::warn!(
                code = ?error.code,
                details = ?error.details,
                "media tool unavailable"
            );
            MediaToolStatus {
                available: false,
                message: error.message,
                hint: Some("安装包通常自带该组件；仍缺失时请重装应用。".into()),
            }
        }
    }
}

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
            // 2. Windows 安装后：exe 同目录或 ffmpeg/ 子目录
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

    // 4. macOS/Linux：系统 PATH（brew / apt 安装的 ffmpeg）
    if let Ok(p) = which::which(unix_name) {
        paths.push(p);
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn resource_dir_is_checked_first() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-tool-status-{}-resource-first",
            std::process::id()
        ));
        let ffmpeg_dir = dir.join("ffmpeg");
        std::fs::create_dir_all(&ffmpeg_dir).expect("mkdir");
        std::fs::write(ffmpeg_dir.join("ffprobe.exe"), b"fake").expect("seed");
        let status = tool_status(Some(&dir));
        assert!(status.available);
        assert!(has_cjk(&status.message));
        assert!(status.hint.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_candidates_prefer_resource_dir() {
        let res = PathBuf::from("/tmp/lumina-res-test");
        let list = tool_candidates("ffprobe", "ffprobe.exe", Some(&res));
        assert_eq!(list[0], res.join("ffmpeg").join("ffprobe.exe"));
        assert_eq!(list[1], res.join("ffmpeg").join("ffprobe"));
    }

    #[test]
    fn unavailable_message_reuses_probe_not_found_copy() {
        // Single source: tool_status unavailable reuses the domain constructor message.
        assert_eq!(
            MediaError::probe_not_found(None).message,
            "媒体分析组件未就绪"
        );
    }
}
