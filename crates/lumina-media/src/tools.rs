//! Shared resolution of project-local FFmpeg tools.

use std::path::PathBuf;

use crate::error::MediaError;
use crate::model::MediaToolStatus;

/// Resource directory exported by the Tauri host for domain calls that do not
/// receive an AppHandle (subtitle, ASR, and MCP child-process paths).
pub const RESOURCE_DIR_ENV: &str = "LUMINA_RESOURCE_DIR";

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

/// Dev-workspace `native/` roots, ordered by proximity to this crate manifest.
/// Monorepo 拆分后 media crate 不再与 `native/` 同目录：先保留 manifest 直系，
/// 再向上兼容查找 `apps/desktop/src-tauri/native`（M1 布局）与旧 `src-tauri/native`。
/// 打包期/测试期的显式 `resource_dir` 与 exe 相对路径不受影响。
pub fn workspace_native_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(dir.join("native"));
    for _ in 0..6 {
        let Some(parent) = dir.parent().map(PathBuf::from) else {
            break;
        };
        dir = parent;
        roots.push(dir.join("native"));
        roots.push(
            dir.join("apps")
                .join("desktop")
                .join("src-tauri")
                .join("native"),
        );
    }
    roots
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

    // 1. Packaged builds: the Tauri host exports its resolved resource dir so
    // domain crates and the --lumina-mcp child use the same bundled tools.
    if let Some(resource) = std::env::var_os(RESOURCE_DIR_ENV).map(PathBuf::from) {
        paths.push(resource.join("ffmpeg").join(windows_name));
        paths.push(resource.join("ffmpeg").join(unix_name));
    }

    // 2. 开发期：crate manifest 或其祖先目录下的 native/ffmpeg/
    for root in workspace_native_roots() {
        paths.push(root.join("ffmpeg").join(windows_name));
        paths.push(root.join("ffmpeg").join(unix_name));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // 3. Windows 安装后：exe 同目录或 ffmpeg/ 子目录
            paths.push(dir.join(windows_name));
            paths.push(dir.join(unix_name));
            paths.push(dir.join("ffmpeg").join(windows_name));
            paths.push(dir.join("ffmpeg").join(unix_name));

            // 4. macOS .app bundle：exe 在 MacOS/，resources 在 ../Resources/
            if let Some(parent) = dir.parent() {
                let resources = parent.join("Resources");
                paths.push(resources.join("ffmpeg").join(windows_name));
                paths.push(resources.join("ffmpeg").join(unix_name));
            }
        }
    }

    // 5. macOS/Linux：系统 PATH（开发机或旧安装的 fallback）
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
    fn unix_resource_dir_fallback_is_supported() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-tool-status-{}-unix-resource",
            std::process::id()
        ));
        let ffmpeg_dir = dir.join("ffmpeg");
        std::fs::create_dir_all(&ffmpeg_dir).expect("mkdir");
        let expected = ffmpeg_dir.join("ffprobe");
        std::fs::write(&expected, b"fake").expect("seed");

        let resolved = resolve_ffprobe_with(Some(&dir)).expect("unix resource ffprobe");
        assert_eq!(resolved, expected);
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
