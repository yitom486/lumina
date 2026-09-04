//! Optional login cookies for online resolve. Preference only — never log cookie bodies.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::ytdl::error::YtdlError;
use crate::ytdl::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CookieMode {
    #[default]
    None,
    Browser,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CookieBrowser {
    #[default]
    Chrome,
    Edge,
    Firefox,
}

impl CookieBrowser {
    pub fn as_yt_dlp(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Firefox => "Firefox",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CookieSettings {
    pub mode: CookieMode,
    #[serde(default)]
    pub browser: CookieBrowser,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlCookieStatus {
    pub mode: CookieMode,
    pub browser: CookieBrowser,
    pub file_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlCookieConfigInput {
    pub mode: CookieMode,
    pub browser: Option<CookieBrowser>,
    pub file_path: Option<String>,
}

fn settings_path() -> PathBuf {
    paths::install_root().join("cookies.json")
}

pub fn status() -> YtdlCookieStatus {
    let settings = load();
    let message = match settings.mode {
        CookieMode::None => "未使用登录态（公开视频通常可播）".into(),
        CookieMode::Browser => format!(
            "解析时从 {} 读取登录态（仅本机；不进入 AI 上下文）",
            settings.browser.label()
        ),
        CookieMode::File => match settings.file_path.as_deref() {
            Some(path) if !path.trim().is_empty() => {
                let name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("cookies.txt");
                format!("已配置登录态文件：{name}")
            }
            _ => "已选择文件模式，但尚未指定 Cookie 文件".into(),
        },
    };
    YtdlCookieStatus {
        mode: settings.mode,
        browser: settings.browser,
        file_path: settings.file_path,
        message,
    }
}

pub fn load() -> CookieSettings {
    let path = settings_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return CookieSettings::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save(input: YtdlCookieConfigInput) -> Result<YtdlCookieStatus, YtdlError> {
    let mut settings = CookieSettings {
        mode: input.mode,
        browser: input.browser.unwrap_or_default(),
        file_path: input
            .file_path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
    };

    match settings.mode {
        CookieMode::None => {
            settings.file_path = None;
        }
        CookieMode::Browser => {
            settings.file_path = None;
        }
        CookieMode::File => {
            let path = settings
                .file_path
                .as_deref()
                .ok_or_else(|| YtdlError::invalid("请选择 Cookie 文件"))?;
            if !Path::new(path).is_file() {
                return Err(YtdlError::invalid("Cookie 文件不存在或不可读"));
            }
        }
    }

    let root = paths::install_root();
    fs::create_dir_all(&root).map_err(|error| {
        YtdlError::internal(Some(&format!("create cookie settings dir: {error}")))
    })?;

    let json = serde_json::to_string_pretty(&settings)
        .map_err(|error| YtdlError::internal(Some(&format!("serialize cookie settings: {error}"))))?;
    fs::write(settings_path(), json)
        .map_err(|error| YtdlError::internal(Some(&format!("write cookie settings: {error}"))))?;

    tracing::info!(
        mode = ?settings.mode,
        browser = ?settings.browser,
        has_file = settings.file_path.is_some(),
        "ytdl cookie settings saved"
    );
    Ok(status())
}

/// Append cookie-related args. Never logs cookie file contents.
pub fn apply_to_command(cmd: &mut Command, settings: &CookieSettings) -> Result<(), YtdlError> {
    match settings.mode {
        CookieMode::None => Ok(()),
        CookieMode::Browser => {
            cmd.arg("--cookies-from-browser");
            cmd.arg(settings.browser.as_yt_dlp());
            Ok(())
        }
        CookieMode::File => {
            let path = settings
                .file_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| YtdlError::invalid("请选择 Cookie 文件"))?;
            if !Path::new(path).is_file() {
                return Err(YtdlError::invalid("Cookie 文件不存在或不可读"));
            }
            cmd.arg("--cookies");
            cmd.arg(path);
            Ok(())
        }
    }
}

/// Map resolver stderr into LoginRequired when it looks like auth / cookie failure.
pub fn classify_resolve_stderr(stderr: &str) -> YtdlError {
    let lower = stderr.to_ascii_lowercase();
    let cookie_read = lower.contains("could not copy")
        || lower.contains("failed to load cookies")
        || lower.contains("unable to load cookies")
        || lower.contains("browser is open")
        || lower.contains("dpapi")
        || (lower.contains("could not find") && lower.contains("cookie"));
    if cookie_read {
        return YtdlError::cookie_unavailable(Some(stderr));
    }

    let login = lower.contains("sign in")
        || lower.contains("login required")
        || lower.contains("please log in")
        || lower.contains("private video")
        || lower.contains("members only")
        || lower.contains("confirm your age")
        || lower.contains("cookies are needed")
        || lower.contains("use --cookies");
    if login {
        return YtdlError::login_required(Some(stderr));
    }

    YtdlError::resolve_failed(Some(stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_arg_names() {
        assert_eq!(CookieBrowser::Chrome.as_yt_dlp(), "chrome");
        assert_eq!(CookieBrowser::Edge.as_yt_dlp(), "edge");
        assert_eq!(CookieBrowser::Firefox.as_yt_dlp(), "firefox");
    }

    #[test]
    fn classify_login_and_cookie_errors() {
        let login = classify_resolve_stderr("ERROR: Sign in to confirm you're not a bot");
        assert_eq!(login.code, crate::ytdl::YtdlErrorCode::LoginRequired);
        assert!(!login.message.to_ascii_lowercase().contains("yt-dlp"));
        assert!(login
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));

        let cookie = classify_resolve_stderr("ERROR: Could not copy cookies from chrome");
        assert_eq!(cookie.code, crate::ytdl::YtdlErrorCode::LoginRequired);
        assert!(cookie.message.contains("浏览器") || cookie.message.contains("登录"));
    }

    #[test]
    fn apply_browser_args() {
        let mut cmd = Command::new("yt-dlp");
        let settings = CookieSettings {
            mode: CookieMode::Browser,
            browser: CookieBrowser::Edge,
            file_path: None,
        };
        apply_to_command(&mut cmd, &settings).unwrap();
        let debug = format!("{cmd:?}");
        assert!(debug.contains("cookies-from-browser"));
        assert!(debug.contains("edge"));
    }
}
