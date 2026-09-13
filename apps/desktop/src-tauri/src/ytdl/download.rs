//! Windows-first download of standalone yt-dlp.exe into app data.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::process_util::command;
use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlInstallEvent, YtdlStatus};
use crate::ytdl::paths::{
    cli_version, find_cli_any, install_root, install_supported, status, CLI_NAME,
};

const USER_AGENT: &str = "Lumina/0.2 (optional yt-dlp installer)";
/// Stable channel rather than a pinned tag: extractors must keep pace with sites.
const CLI_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const CHECKSUM_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";

pub fn install_cli<F>(mut on_event: F) -> Result<YtdlStatus, YtdlError>
where
    F: FnMut(YtdlInstallEvent),
{
    if !install_supported() {
        return Err(YtdlError::invalid(
            "当前系统暂不支持应用内下载在线解析组件，请按说明手动放置",
        ));
    }

    let root = install_root();
    fs::create_dir_all(&root).map_err(|error| {
        YtdlError::download_failed(Some(&format!("create install root: {error}")))
    })?;

    let previous = find_cli_any().and_then(|path| cli_version(&path));
    let dest = root.join(CLI_NAME);
    let action = if previous.is_some() {
        "更新"
    } else {
        "下载"
    };
    on_event(progress(
        "download_cli",
        &format!("正在{action}在线解析组件…"),
        Some(0),
        None,
    ));
    let expected_sha256 = fetch_expected_sha256()?;
    let staged = root.join("yt-dlp.download.exe");
    let actual_sha256 = download_file(CLI_URL, &staged, action, &mut on_event)?;
    if !actual_sha256.eq_ignore_ascii_case(&expected_sha256) {
        let _ = fs::remove_file(&staged);
        return Err(YtdlError::download_failed(Some(&format!(
            "yt-dlp sha256 mismatch: expected {expected_sha256}, got {actual_sha256}"
        ))));
    }
    let downloaded_version = validate_download(&staged)?;
    replace_cli(&staged, &dest)?;

    if !dest.is_file() {
        return Err(YtdlError::download_failed(Some(
            "download finished but yt-dlp.exe not found",
        )));
    }

    tracing::info!(
        previous_version = previous.as_deref().unwrap_or("none"),
        version = %downloaded_version,
        path = %dest.display(),
        "yt-dlp installed or updated"
    );
    let st = status();
    on_event(YtdlInstallEvent::Finished { status: st.clone() });
    Ok(st)
}

fn progress(
    stage: &str,
    message: &str,
    downloaded: Option<u64>,
    total: Option<u64>,
) -> YtdlInstallEvent {
    YtdlInstallEvent::Progress {
        stage: stage.into(),
        message: message.into(),
        downloaded,
        total,
    }
}

fn fetch_expected_sha256() -> Result<String, YtdlError> {
    let response = ureq::get(CHECKSUM_URL)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| YtdlError::download_failed(Some(&format!("GET checksum: {error}"))))?;
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|error| YtdlError::download_failed(Some(&format!("read checksum: {error}"))))?;
    parse_expected_sha256(&body)
        .ok_or_else(|| YtdlError::download_failed(Some("yt-dlp.exe checksum missing or invalid")))
}

fn parse_expected_sha256(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == CLI_NAME && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn download_file<F>(
    url: &str,
    dest: &Path,
    action: &str,
    on_event: &mut F,
) -> Result<String, YtdlError>
where
    F: FnMut(YtdlInstallEvent),
{
    let _ = fs::remove_file(dest);

    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| YtdlError::download_failed(Some(&format!("GET {url}: {error}"))))?;

    let status = response.status();
    if !(200..300).contains(&status.as_u16()) {
        return Err(YtdlError::download_failed(Some(&format!(
            "GET {url} status {status}"
        ))));
    }

    let total = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let mut reader = response.into_body().into_reader();
    let mut file = File::create(dest)
        .map_err(|error| YtdlError::download_failed(Some(&format!("create staged: {error}"))))?;
    let mut sha256 = Sha256::new();

    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|error| YtdlError::download_failed(Some(&format!("read body: {error}"))))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|error| YtdlError::download_failed(Some(&format!("write body: {error}"))))?;
        sha256.update(&buf[..n]);
        downloaded += n as u64;
        if downloaded == n as u64 || downloaded % (512 * 1024) < n as u64 {
            on_event(progress(
                "download_cli",
                &format!("正在{action}在线解析组件…"),
                Some(downloaded),
                total,
            ));
        }
    }
    file.sync_all()
        .map_err(|error| YtdlError::download_failed(Some(&format!("sync: {error}"))))?;
    drop(file);

    if downloaded < 1_000_000 {
        let _ = fs::remove_file(dest);
        return Err(YtdlError::download_failed(Some(&format!(
            "downloaded file too small ({downloaded} bytes)"
        ))));
    }

    Ok(format!("{:x}", sha256.finalize()))
}

fn validate_download(path: &Path) -> Result<String, YtdlError> {
    let output = command(path).arg("--version").output().map_err(|error| {
        YtdlError::download_failed(Some(&format!("validate downloaded executable: {error}")))
    })?;
    if !output.status.success() {
        return Err(YtdlError::download_failed(Some(&format!(
            "downloaded executable validation exit: {}",
            output.status
        ))));
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| YtdlError::download_failed(Some("downloaded executable version missing")))
}

fn replace_cli(staged: &Path, dest: &Path) -> Result<(), YtdlError> {
    let backup = dest.with_file_name("yt-dlp.previous.exe");
    let _ = fs::remove_file(&backup);
    let had_dest = dest.is_file();
    if had_dest {
        fs::rename(dest, &backup).map_err(|error| {
            YtdlError::download_failed(Some(&format!("backup existing executable: {error}")))
        })?;
    }

    if let Err(error) = fs::rename(staged, dest) {
        if had_dest {
            if let Err(restore_error) = fs::rename(&backup, dest) {
                tracing::error!(%restore_error, "failed to restore previous yt-dlp executable");
            }
        }
        return Err(YtdlError::download_failed(Some(&format!(
            "replace executable: {error}"
        ))));
    }

    if let Err(error) = fs::remove_file(&backup) {
        if backup.exists() {
            tracing::warn!(%error, "failed to remove previous yt-dlp executable backup");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_checksum_line() {
        let body = format!(
            "abc  other\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  {CLI_NAME}\n"
        );
        assert_eq!(
            parse_expected_sha256(&body).as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn rejects_invalid_checksum() {
        assert!(parse_expected_sha256("not-a-hash  yt-dlp.exe").is_none());
    }
}
