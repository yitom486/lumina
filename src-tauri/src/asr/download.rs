//! Optional in-app download of whisper-cli + ggml models (Windows-first).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::asr::error::AsrError;
use crate::asr::model::{AsrInstallEvent, AsrStatus};
use crate::asr::paths::{
    self, find_cli_any, install_root, install_supported, models_install_dir, resolve_catalog_model,
    status,
};

const USER_AGENT: &str = "Lumina/0.2 (optional ASR installer)";
/// Pinned whisper.cpp nightly with Windows CPU binaries.
const CLI_RELEASE_TAG: &str = "b4938";
const CLI_ZIP_URL: &str =
    "https://github.com/ggml-org/whisper.cpp/releases/download/b4938/whisper-bin-x64.zip";
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub fn install_bundle<F>(model_id: &str, mut on_event: F) -> Result<AsrStatus, AsrError>
where
    F: FnMut(AsrInstallEvent),
{
    if !install_supported() {
        return Err(AsrError::invalid(
            "当前系统暂不支持应用内下载转写组件，请按说明手动放置",
        ));
    }

    let catalog = resolve_catalog_model(model_id)?;
    let root = install_root();
    fs::create_dir_all(&root).map_err(|error| {
        AsrError::download_failed(Some(&format!("create install root: {error}")))
    })?;
    let models_dir = models_install_dir();
    fs::create_dir_all(&models_dir).map_err(|error| {
        AsrError::download_failed(Some(&format!("create models dir: {error}")))
    })?;

    if find_cli_any().is_none() {
        on_event(progress(
            "download_cli",
            "正在下载转写引擎…",
            None,
            None,
        ));
        download_and_extract_cli(&root, &mut on_event)?;
        if find_cli_any().is_none() {
            return Err(AsrError::download_failed(Some(
                "zip extracted but whisper-cli.exe not found",
            )));
        }
    } else {
        on_event(progress(
            "download_cli",
            "转写引擎已就绪，跳过下载",
            None,
            None,
        ));
    }

    let model_path = models_dir.join(catalog.file_name);
    if model_path.is_file() && model_path.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false)
    {
        on_event(progress(
            "download_model",
            "所选模型已存在，跳过下载",
            None,
            None,
        ));
    } else {
        let url = format!("{MODEL_BASE_URL}/{}", catalog.file_name);
        on_event(progress(
            "download_model",
            &format!("正在下载模型 {}…", catalog.label),
            Some(0),
            Some(catalog.approx_bytes),
        ));
        download_file(&url, &model_path, catalog.approx_bytes, &mut on_event)?;
    }

    // Keep a small marker for support / debugging (not shown in UI).
    let _ = fs::write(
        root.join("VERSION"),
        format!("cli_tag={CLI_RELEASE_TAG}\nmodel={}\n", catalog.file_name),
    );

    let status = status();
    if !status.available {
        return Err(AsrError::download_failed(Some(
            "download finished but asr_status still unavailable",
        )));
    }
    on_event(AsrInstallEvent::Finished {
        status: status.clone(),
    });
    Ok(status)
}

fn progress(
    stage: &str,
    message: &str,
    bytes_received: Option<u64>,
    bytes_total: Option<u64>,
) -> AsrInstallEvent {
    AsrInstallEvent::Progress {
        stage: stage.into(),
        message: message.into(),
        bytes_received,
        bytes_total,
    }
}

fn download_and_extract_cli<F>(root: &Path, on_event: &mut F) -> Result<(), AsrError>
where
    F: FnMut(AsrInstallEvent),
{
    let zip_path = root.join("whisper-bin-x64.zip.partial");
    download_file(CLI_ZIP_URL, &zip_path, 8_000_000, on_event)?;
    on_event(progress("extract", "正在解压转写引擎…", None, None));
    extract_cli_zip(&zip_path, root)?;
    let _ = fs::remove_file(&zip_path);
    Ok(())
}

fn download_file<F>(
    url: &str,
    dest: &Path,
    approx_total: u64,
    on_event: &mut F,
) -> Result<(), AsrError>
where
    F: FnMut(AsrInstallEvent),
{
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AsrError::download_failed(Some(&format!("create parent: {error}")))
        })?;
    }
    let partial = dest.with_extension(format!(
        "{}.partial",
        dest.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
    ));
    // Prefer "*.partial" beside final name when extension rewrite is awkward.
    let partial = if dest.extension().is_some() {
        let mut p = dest.as_os_str().to_owned();
        p.push(".partial");
        PathBuf::from(p)
    } else {
        partial
    };

    if partial.exists() {
        let _ = fs::remove_file(&partial);
    }

    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| {
            tracing::warn!(%error, %url, "ASR download request failed");
            AsrError::download_failed(Some(&format!("GET {url}: {error}")))
        })?;

    let status = response.status();
    if !(200..300).contains(&status.as_u16()) {
        return Err(AsrError::download_failed(Some(&format!(
            "GET {url} status {}",
            status
        ))));
    }

    let total = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(approx_total);

    let mut reader = response.into_body().into_reader();
    let mut file = File::create(&partial).map_err(|error| {
        AsrError::download_failed(Some(&format!("create {}: {error}", partial.display())))
    })?;

    let mut buf = [0_u8; 64 * 1024];
    let mut received = 0_u64;
    let mut last_emit = 0_u64;
    loop {
        let n = reader.read(&mut buf).map_err(|error| {
            AsrError::download_failed(Some(&format!("read body: {error}")))
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|error| AsrError::download_failed(Some(&format!("write: {error}"))))?;
        received += n as u64;
        if received == n as u64
            || received - last_emit >= 2 * 1024 * 1024
            || received >= total
        {
            last_emit = received;
            let pct = if total > 0 {
                ((received.min(total) as f64 / total as f64) * 100.0).round() as u64
            } else {
                0
            };
            on_event(progress(
                "download",
                &format!("下载中… {pct}%"),
                Some(received),
                Some(total),
            ));
        }
    }
    file.flush()
        .map_err(|error| AsrError::download_failed(Some(&format!("flush: {error}"))))?;
    drop(file);

    if received < 1_000 {
        let _ = fs::remove_file(&partial);
        return Err(AsrError::download_failed(Some("downloaded file too small")));
    }

    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
    fs::rename(&partial, dest).map_err(|error| {
        AsrError::download_failed(Some(&format!(
            "rename {} -> {}: {error}",
            partial.display(),
            dest.display()
        )))
    })?;
    Ok(())
}

fn extract_cli_zip(zip_path: &Path, root: &Path) -> Result<(), AsrError> {
    let file = File::open(zip_path).map_err(|error| {
        AsrError::download_failed(Some(&format!("open zip: {error}")))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        AsrError::download_failed(Some(&format!("read zip: {error}")))
    })?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|error| {
            AsrError::download_failed(Some(&format!("zip entry: {error}")))
        })?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with('/') {
            continue;
        }
        let file_name = Path::new(&name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let lower = file_name.to_ascii_lowercase();
        let keep = lower.ends_with(".exe")
            || lower.ends_with(".dll")
            || lower == "whisper-cli"
            || lower == "main";
        if !keep {
            continue;
        }
        // Flatten into install root (ignore nested Release/ folders).
        let out = root.join(file_name);
        let mut out_file = File::create(&out).map_err(|error| {
            AsrError::download_failed(Some(&format!("create {}: {error}", out.display())))
        })?;
        std::io::copy(&mut entry, &mut out_file).map_err(|error| {
            AsrError::download_failed(Some(&format!("extract {}: {error}", file_name)))
        })?;
    }

    // Prefer whisper-cli.exe naming if only main.exe exists.
    let cli = root.join("whisper-cli.exe");
    let main = root.join("main.exe");
    if !cli.is_file() && main.is_file() {
        let _ = fs::copy(&main, &cli);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_resolve_used_by_install_guard() {
        assert!(resolve_catalog_model("base").is_ok());
        assert!(paths::install_root().to_string_lossy().contains("whisper"));
    }
}
