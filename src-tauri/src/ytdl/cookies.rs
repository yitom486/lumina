//! Optional login cookies for online resolve. Preference only — never log cookie bodies.
//!
//! Multi-account on desktop browsers = **profiles** (Chrome/Edge Person 1/2…),
//! not accounts inside one profile. yt-dlp: `--cookies-from-browser chrome:Profile 1`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::process_util::command;
use crate::ytdl::error::YtdlError;
use crate::ytdl::paths;

/// Tiny public clip used only to verify cookie DB decrypt + network auth path.
const COOKIE_TEST_URL: &str = "https://www.youtube.com/watch?v=jNQXAC9IVRw";

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
    /// Chrome/Edge folder name (`Default`, `Profile 1`) or Firefox profile name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlCookieStatus {
    pub mode: CookieMode,
    pub browser: CookieBrowser,
    pub browser_profile: Option<String>,
    pub file_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlCookieConfigInput {
    pub mode: CookieMode,
    pub browser: Option<CookieBrowser>,
    pub browser_profile: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileOption {
    /// Value passed to yt-dlp after `browser:` (e.g. `Default`, `Profile 1`).
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlCookieTestResult {
    pub ok: bool,
    pub message: String,
}

fn settings_path() -> PathBuf {
    paths::install_root().join("cookies.json")
}

pub fn status() -> YtdlCookieStatus {
    let settings = load();
    let message = match settings.mode {
        CookieMode::None => "未使用登录态（公开视频通常可播）".into(),
        CookieMode::Browser => {
            let profile = settings
                .browser_profile
                .as_deref()
                .filter(|p| !p.is_empty())
                .unwrap_or("默认配置档案");
            format!(
                "解析时从 {}「{}」读取登录态（请先关闭该浏览器；仅本机，不进 AI）",
                settings.browser.label(),
                profile
            )
        }
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
        browser_profile: settings.browser_profile.clone(),
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
        browser_profile: input
            .browser_profile
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
        file_path: input
            .file_path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
    };

    match settings.mode {
        CookieMode::None => {
            settings.browser_profile = None;
            settings.file_path = None;
        }
        CookieMode::Browser => {
            settings.file_path = None;
        }
        CookieMode::File => {
            settings.browser_profile = None;
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

    let json = serde_json::to_string_pretty(&settings).map_err(|error| {
        YtdlError::internal(Some(&format!("serialize cookie settings: {error}")))
    })?;
    fs::write(settings_path(), json)
        .map_err(|error| YtdlError::internal(Some(&format!("write cookie settings: {error}"))))?;

    tracing::info!(
        mode = ?settings.mode,
        browser = ?settings.browser,
        has_profile = settings.browser_profile.is_some(),
        has_file = settings.file_path.is_some(),
        "ytdl cookie settings saved"
    );
    Ok(status())
}

fn browser_cookie_spec(settings: &CookieSettings) -> String {
    match settings
        .browser_profile
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(profile) => format!("{}:{profile}", settings.browser.as_yt_dlp()),
        None => settings.browser.as_yt_dlp().to_string(),
    }
}

/// Append cookie-related args. Never logs cookie file contents.
pub fn apply_to_command(cmd: &mut Command, settings: &CookieSettings) -> Result<(), YtdlError> {
    match settings.mode {
        CookieMode::None => Ok(()),
        CookieMode::Browser => {
            cmd.arg("--cookies-from-browser");
            cmd.arg(browser_cookie_spec(settings));
            // Also dump a Netscape jar for libmpv CDN fetches (same auth as resolve).
            cmd.arg("--cookies");
            cmd.arg(mpv_cookies_export_path());
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

/// Netscape cookies file path used by the player for googlevideo / CDN requests.
pub fn mpv_cookies_export_path() -> PathBuf {
    paths::install_root().join("mpv-cookies.txt")
}

/// Cookie jar path for libmpv (never read or log contents).
pub fn cookies_file_for_player() -> Option<PathBuf> {
    let settings = load();
    match settings.mode {
        CookieMode::None => None,
        CookieMode::File => settings
            .file_path
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.is_file()),
        CookieMode::Browser => {
            let exported = mpv_cookies_export_path();
            if exported.is_file() {
                Some(exported)
            } else {
                None
            }
        }
    }
}

/// List local browser profiles for the given browser (Windows-first paths).
pub fn list_browser_profiles(
    browser: CookieBrowser,
) -> Result<Vec<BrowserProfileOption>, YtdlError> {
    match browser {
        CookieBrowser::Chrome | CookieBrowser::Edge => list_chromium_profiles(browser),
        CookieBrowser::Firefox => list_firefox_profiles(),
    }
}

fn list_chromium_profiles(browser: CookieBrowser) -> Result<Vec<BrowserProfileOption>, YtdlError> {
    let Some(user_data) = chromium_user_data_dir(browser) else {
        return Ok(Vec::new());
    };
    if !user_data.is_dir() {
        return Ok(Vec::new());
    }

    let mut name_by_id = std::collections::HashMap::new();
    let local_state = user_data.join("Local State");
    if let Ok(raw) = fs::read_to_string(&local_state) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(cache) = json
                .pointer("/profile/info_cache")
                .and_then(|v| v.as_object())
            {
                for (id, meta) in cache {
                    if let Some(name) = meta.get("name").and_then(|v| v.as_str()) {
                        name_by_id.insert(id.clone(), name.to_string());
                    }
                }
            }
        }
    }

    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&user_data) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(id) = name.to_str() else {
            continue;
        };
        if id != "Default" && !id.starts_with("Profile ") {
            continue;
        }
        // Cookie DB present ⇒ usable profile.
        if !entry.path().join("Network").join("Cookies").is_file()
            && !entry.path().join("Cookies").is_file()
        {
            continue;
        }
        let label = name_by_id
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string());
        out.push(BrowserProfileOption {
            id: id.to_string(),
            label: if label == *id {
                label
            } else {
                format!("{label}（{id}）")
            },
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn chromium_user_data_dir(browser: CookieBrowser) -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    Some(match browser {
        CookieBrowser::Chrome => local.join("Google").join("Chrome").join("User Data"),
        CookieBrowser::Edge => local.join("Microsoft").join("Edge").join("User Data"),
        CookieBrowser::Firefox => return None,
    })
}

fn list_firefox_profiles() -> Result<Vec<BrowserProfileOption>, YtdlError> {
    let Some(ini_path) = firefox_profiles_ini() else {
        return Ok(Vec::new());
    };
    let Ok(raw) = fs::read_to_string(&ini_path) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    let mut current_name: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current_name = None;
            continue;
        }
        if let Some(name) = line.strip_prefix("Name=") {
            current_name = Some(name.to_string());
        }
        if line.starts_with("Path=") {
            if let Some(name) = current_name.take() {
                out.push(BrowserProfileOption {
                    id: name.clone(),
                    label: name,
                });
            }
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(out)
}

fn firefox_profiles_ini() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from)?;
        let path = appdata.join("Mozilla").join("Firefox").join("profiles.ini");
        return path.is_file().then_some(path);
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let path = home.join("Library/Application Support/Firefox/profiles.ini");
        return path.is_file().then_some(path);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let path = home.join(".mozilla/firefox/profiles.ini");
        return path.is_file().then_some(path);
    }
    #[allow(unreachable_code)]
    None
}

/// Probe whether the current cookie settings can be read (and used for a short public URL).
pub fn test_cookies(settings: &CookieSettings) -> Result<YtdlCookieTestResult, YtdlError> {
    if matches!(settings.mode, CookieMode::None) {
        return Ok(YtdlCookieTestResult {
            ok: false,
            message: "尚未选择登录态来源，请先选浏览器配置档案或导入 Cookie 文件".into(),
        });
    }

    let cli = paths::require_cli()?;
    let mut cmd = command(&cli);
    cmd.args([
        "--skip-download",
        "--no-playlist",
        "--no-warnings",
        "--print",
        "%(id)s",
    ]);
    apply_to_command(&mut cmd, settings)?;
    cmd.arg(COOKIE_TEST_URL);

    let output = cmd.output().map_err(|error| {
        tracing::warn!(%error, "cookie test spawn failed");
        YtdlError::resolve_failed(Some(&error.to_string()))
    })?;

    if output.status.success() {
        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let ok = !id.is_empty();
        return Ok(YtdlCookieTestResult {
            ok,
            message: if ok {
                match settings.mode {
                    CookieMode::Browser => format!(
                        "可以读取 {} 登录态（配置档案：{}）",
                        settings.browser.label(),
                        settings
                            .browser_profile
                            .as_deref()
                            .filter(|p| !p.is_empty())
                            .unwrap_or("默认")
                    ),
                    CookieMode::File => "可以读取 Cookie 文件登录态".into(),
                    CookieMode::None => "未使用登录态".into(),
                }
            } else {
                "登录态读取结果异常，请关闭浏览器后重试".into()
            },
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    tracing::warn!(%stderr, "cookie test failed");
    let mut classified = classify_resolve_stderr(&if stderr.is_empty() {
        "cookie test failed".into()
    } else {
        stderr
    });
    if matches!(settings.mode, CookieMode::Browser)
        && browser_process_running(settings.browser)
        && classified.code == crate::ytdl::YtdlErrorCode::LoginRequired
    {
        classified.message = format!(
            "{}（检测到 {} 仍在后台运行，请在任务管理器结束进程）",
            classified.message,
            settings.browser.label()
        );
    }
    Ok(YtdlCookieTestResult {
        ok: false,
        message: classified.message,
    })
}

/// Map resolver stderr into LoginRequired when it looks like auth / cookie failure.
pub fn classify_resolve_stderr(stderr: &str) -> YtdlError {
    let lower = stderr.to_ascii_lowercase();

    // Chrome/Edge App-Bound Encryption — closing the window is not enough.
    if lower.contains("dpapi")
        || lower.contains("failed to decrypt")
        || lower.contains("app-bound")
        || lower.contains("app bound")
    {
        return YtdlError::cookie_encrypted(Some(stderr));
    }

    let cookie_locked = lower.contains("could not copy")
        || lower.contains("browser is open")
        || (lower.contains("database is locked"));
    if cookie_locked {
        return YtdlError::cookie_unavailable(Some(stderr));
    }

    let cookie_read = lower.contains("failed to load cookies")
        || lower.contains("unable to load cookies")
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

/// Best-effort: Chromium often keeps background processes after the window closes.
pub fn browser_process_running(browser: CookieBrowser) -> bool {
    let names: &[&str] = match browser {
        CookieBrowser::Chrome => &["chrome.exe"],
        CookieBrowser::Edge => &["msedge.exe"],
        CookieBrowser::Firefox => &["firefox.exe"],
    };
    names.iter().any(|name| process_running(name))
}

fn process_running(exe_name: &str) -> bool {
    #[cfg(windows)]
    {
        let Ok(output) = command("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {exe_name}"), "/NH"])
            .output()
        else {
            return false;
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        return stdout.contains(&exe_name.to_ascii_lowercase());
    }
    #[cfg(not(windows))]
    {
        let _ = exe_name;
        false
    }
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
    fn browser_spec_includes_profile() {
        let settings = CookieSettings {
            mode: CookieMode::Browser,
            browser: CookieBrowser::Chrome,
            browser_profile: Some("Profile 1".into()),
            file_path: None,
        };
        assert_eq!(browser_cookie_spec(&settings), "chrome:Profile 1");
        let mut cmd = Command::new("yt-dlp");
        apply_to_command(&mut cmd, &settings).unwrap();
        let debug = format!("{cmd:?}");
        assert!(debug.contains("chrome:Profile 1"));
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
        assert!(
            cookie.message.contains("浏览器")
                || cookie.message.contains("登录")
                || cookie.message.contains("任务管理器")
        );

        let encrypted =
            classify_resolve_stderr("ERROR: Failed to decrypt with DPAPI. See issues/10927");
        assert_eq!(encrypted.code, crate::ytdl::YtdlErrorCode::LoginRequired);
        assert!(encrypted.message.contains("加密") || encrypted.message.contains("Cookie"));
    }

    #[test]
    fn apply_browser_args() {
        let mut cmd = Command::new("yt-dlp");
        let settings = CookieSettings {
            mode: CookieMode::Browser,
            browser: CookieBrowser::Edge,
            browser_profile: None,
            file_path: None,
        };
        apply_to_command(&mut cmd, &settings).unwrap();
        let debug = format!("{cmd:?}");
        assert!(debug.contains("cookies-from-browser"));
        assert!(debug.contains("edge"));
    }
}
