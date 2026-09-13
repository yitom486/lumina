//! Locate optional yt-dlp binary (app data → native/ytdl → exe dir).

use std::path::PathBuf;

use crate::process_util::command;
use crate::ytdl::error::YtdlError;
use crate::ytdl::model::YtdlStatus;

#[cfg(windows)]
pub(crate) const CLI_NAME: &str = "yt-dlp.exe";
#[cfg(not(windows))]
pub(crate) const CLI_NAME: &str = "yt-dlp";

pub fn install_supported() -> bool {
    cfg!(windows)
}

pub fn install_root() -> PathBuf {
    if let Some(base) = dirs_data_dir() {
        return base.join("lumina").join("yt-dlp");
    }
    PathBuf::from("yt-dlp")
}

pub fn status() -> YtdlStatus {
    let cli = find_cli_any();
    let install_supported = install_supported();
    match cli {
        Some(path) => {
            let version = cli_version(&path);
            let message = version
                .as_deref()
                .map(|value| format!("在线视频解析已就绪（版本 {value}）"))
                .unwrap_or_else(|| "在线视频解析已就绪（版本未知）".into());
            YtdlStatus {
                available: true,
                cli_ready: true,
                cli_path: Some(path.to_string_lossy().to_string()),
                version,
                install_supported,
                message,
            }
        }
        None => YtdlStatus {
            available: false,
            cli_ready: false,
            cli_path: None,
            version: None,
            install_supported,
            message: if install_supported {
                "尚未安装在线解析组件，可一键下载".into()
            } else {
                YtdlError::not_configured(None).message
            },
        },
    }
}

pub fn cli_version(path: &std::path::Path) -> Option<String> {
    let output = command(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn require_cli() -> Result<PathBuf, YtdlError> {
    find_cli_any().ok_or_else(|| {
        YtdlError::not_configured(Some(
            "missing yt-dlp under APPDATA/lumina/yt-dlp or native/ytdl",
        ))
    })
}

pub fn find_cli_any() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(install_root().join(CLI_NAME));

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("native").join("ytdl").join(CLI_NAME));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(CLI_NAME));
            candidates.push(dir.join("yt-dlp").join(CLI_NAME));
        }
    }

    // PATH fallback (brew / pip on Unix)
    #[cfg(not(windows))]
    {
        if let Ok(p) = which::which(CLI_NAME) {
            candidates.push(p);
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn dirs_data_dir() -> Option<PathBuf> {
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
    fn status_message_is_chinese() {
        let s = status();
        assert!(s
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert!(!s.message.to_ascii_lowercase().contains("yt-dlp"));
    }
}
