//! Windows-first download of standalone yt-dlp.exe into app data.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlInstallEvent, YtdlStatus};
use crate::ytdl::paths::{find_cli_any, install_root, install_supported, status, CLI_NAME};

const USER_AGENT: &str = "Lumina/0.2 (optional yt-dlp installer)";
/// Pinned release; bump when extractors break often.
const CLI_RELEASE_TAG: &str = "2025.10.14";
const CLI_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/download/2025.10.14/yt-dlp.exe";

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

    if find_cli_any().is_some() {
        on_event(progress(
            "download_cli",
            "在线解析组件已就绪，跳过下载",
            None,
            None,
        ));
        let st = status();
        on_event(YtdlInstallEvent::Finished { status: st.clone() });
        return Ok(st);
    }

    let dest = root.join(CLI_NAME);
    on_event(progress(
        "download_cli",
        "正在下载在线解析组件…",
        Some(0),
        None,
    ));
    download_file(CLI_URL, &dest, &mut on_event)?;

    if find_cli_any().is_none() {
        return Err(YtdlError::download_failed(Some(
            "download finished but yt-dlp.exe not found",
        )));
    }

    tracing::info!(tag = CLI_RELEASE_TAG, path = %dest.display(), "yt-dlp installed");
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

fn download_file<F>(url: &str, dest: &Path, on_event: &mut F) -> Result<(), YtdlError>
where
    F: FnMut(YtdlInstallEvent),
{
    let partial = dest.with_extension("exe.partial");
    let _ = fs::remove_file(&partial);

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
    let mut file = File::create(&partial)
        .map_err(|error| YtdlError::download_failed(Some(&format!("create partial: {error}"))))?;

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
        downloaded += n as u64;
        if downloaded == n as u64 || downloaded % (512 * 1024) < n as u64 {
            on_event(progress(
                "download_cli",
                "正在下载在线解析组件…",
                Some(downloaded),
                total,
            ));
        }
    }
    file.sync_all()
        .map_err(|error| YtdlError::download_failed(Some(&format!("sync: {error}"))))?;
    drop(file);

    if downloaded < 1_000_000 {
        let _ = fs::remove_file(&partial);
        return Err(YtdlError::download_failed(Some(&format!(
            "downloaded file too small ({downloaded} bytes)"
        ))));
    }

    fs::rename(&partial, dest).map_err(|error| {
        YtdlError::download_failed(Some(&format!("rename into place: {error}")))
    })?;
    Ok(())
}
