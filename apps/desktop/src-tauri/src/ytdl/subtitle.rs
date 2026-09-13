//! On-demand online subtitle download and cache.

use std::fs;
use std::path::{Path, PathBuf};

use crate::process_util::command;
use crate::subtitle::extract::read_external_subtitle;
use crate::subtitle::parse::parse_subtitle_text;
use crate::subtitle::{SubtitleChoice, SubtitleError, SubtitleSource, Transcript};
use crate::ytdl::{YtdlResolveResult, YtdlSubtitleTrack};

const ONLINE_PREFIX: &str = "online:";

pub fn list_choices(resolved: &YtdlResolveResult) -> Vec<SubtitleChoice> {
    resolved
        .subtitles
        .iter()
        .map(|track| SubtitleChoice {
            id: choice_id(&track.language),
            source: SubtitleSource::Sidecar,
            label: format!(
                "在线 · {} · {}",
                track.language,
                track
                    .name
                    .as_deref()
                    .or(track.ext.as_deref())
                    .unwrap_or("字幕")
            ),
            supported: true,
            stream_index: None,
            external_path: None,
            codec_name: track.ext.clone().or_else(|| Some("vtt".into())),
            language: Some(track.language.clone()),
        })
        .collect()
}

pub fn load_choice(
    page_url: &str,
    resolved: &YtdlResolveResult,
    choice_id: &str,
) -> Result<Transcript, SubtitleError> {
    let language = choice_id
        .strip_prefix(ONLINE_PREFIX)
        .ok_or_else(|| SubtitleError::extract_failed(Some("invalid online subtitle choice")))?;
    let track = resolved
        .subtitles
        .iter()
        .find(|track| track.language == language)
        .ok_or_else(|| SubtitleError::extract_failed(Some("online subtitle choice not found")))?;
    let cached = download_or_cached(page_url, &resolved.media_id, track)?;
    let (content, source_path) = read_external_subtitle(&cached)?;
    let cues = parse_subtitle_text(&content)?;
    Ok(Transcript {
        source_path: source_path.to_string_lossy().to_string(),
        choice_id: choice_id.to_string(),
        stream_index: None,
        language: Some(track.language.clone()),
        codec_name: source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_string),
        cues,
    })
}

fn choice_id(language: &str) -> String {
    format!("{ONLINE_PREFIX}{language}")
}

fn download_or_cached(
    page_url: &str,
    media_id: &str,
    track: &YtdlSubtitleTrack,
) -> Result<PathBuf, SubtitleError> {
    let dir = crate::ytdl::paths::install_root()
        .join("subtitles")
        .join(safe_component(media_id))
        .join(safe_component(&track.language));
    fs::create_dir_all(&dir).map_err(|error| {
        tracing::warn!(%error, path = %dir.display(), "online subtitle cache create failed");
        SubtitleError::internal(Some(&format!("create online subtitle cache: {error}")))
    })?;
    if let Some(path) = find_subtitle(&dir) {
        return Ok(path);
    }

    if let Some(url) = track.url.as_deref() {
        match download_signed_subtitle(url, &dir, track.ext.as_deref()) {
            Ok(path) => return Ok(path),
            Err(error) => tracing::warn!(
                code = ?error.code,
                details = ?error.details,
                "cached signed subtitle URL failed; retrying through resolver"
            ),
        }
    }

    let cli = crate::ytdl::paths::require_cli()
        .map_err(|error| SubtitleError::extract_failed(error.details.as_deref()))?;
    let output_template = dir.join("subtitle.%(ext)s");
    let mut cmd = command(&cli);
    cmd.args([
        "--skip-download",
        "--no-playlist",
        "--no-warnings",
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs",
        &track.language,
        "--sub-format",
        "vtt/best",
        "--convert-subs",
        "vtt",
        "-o",
        &output_template.to_string_lossy(),
    ]);
    crate::ytdl::cookies::apply_to_command(&mut cmd, &crate::ytdl::cookies::load())
        .map_err(|error| SubtitleError::extract_failed(error.details.as_deref()))?;
    cmd.arg(page_url);
    let output = cmd.output().map_err(|error| {
        tracing::warn!(%error, "online subtitle process spawn failed");
        SubtitleError::extract_failed(Some(&format!("online subtitle spawn: {error}")))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::warn!(%stderr, language = %track.language, "online subtitle download failed");
        return Err(SubtitleError::extract_failed(Some(&stderr)));
    }
    find_subtitle(&dir).ok_or_else(|| {
        SubtitleError::extract_failed(Some("online subtitle command produced no supported file"))
    })
}

fn download_signed_subtitle(
    url: &str,
    dir: &Path,
    extension: Option<&str>,
) -> Result<PathBuf, SubtitleError> {
    let ext = extension
        .filter(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "vtt" | "srt" | "ass" | "ssa"
            )
        })
        .unwrap_or("vtt");
    let destination = dir.join(format!("subtitle.{ext}"));
    let body = ureq::get(url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .map_err(|error| {
            SubtitleError::extract_failed(Some(&format!("signed subtitle request: {error}")))
        })?
        .into_body()
        .read_to_string()
        .map_err(|error| {
            SubtitleError::extract_failed(Some(&format!("signed subtitle body: {error}")))
        })?;
    if body.trim().is_empty() {
        return Err(SubtitleError::extract_failed(Some(
            "signed subtitle body empty",
        )));
    }
    fs::write(&destination, body).map_err(|error| {
        SubtitleError::extract_failed(Some(&format!("cache signed subtitle: {error}")))
    })?;
    Ok(destination)
}

fn find_subtitle(dir: &Path) -> Option<PathBuf> {
    let mut found = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "vtt" | "srt" | "ass" | "ssa"
                        )
                    })
        })
        .collect::<Vec<_>>();
    found.sort();
    found.into_iter().next()
}

fn safe_component(value: &str) -> String {
    let safe = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>();
    if safe.is_empty() {
        "remote".into()
    } else {
        safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_choices_do_not_expose_local_paths() {
        let resolved = YtdlResolveResult {
            media_id: "youtube:abc".into(),
            title: Some("Demo".into()),
            duration_ms: Some(1_000),
            webpage_url: None,
            extractor: Some("youtube".into()),
            chapters: vec![],
            formats: vec![],
            subtitles: vec![YtdlSubtitleTrack {
                language: "zh-Hans".into(),
                ext: Some("vtt".into()),
                name: Some("中文".into()),
                url: None,
            }],
            recommended_url: None,
            recommended_format_id: None,
        };
        let choices = list_choices(&resolved);
        assert_eq!(choices[0].id, "online:zh-Hans");
        assert!(choices[0].external_path.is_none());
    }

    #[test]
    #[ignore = "requires network, configured yt-dlp cookies, and LUMINA_YTDL_E2E_URL"]
    fn downloads_and_parses_online_subtitle() {
        let url = std::env::var("LUMINA_YTDL_E2E_URL").expect("set E2E URL");
        let resolved = crate::ytdl::resolve::resolve_url(&url).expect("resolve online metadata");
        let choice = list_choices(&resolved)
            .into_iter()
            .next()
            .expect("online source should expose subtitles");
        let transcript = load_choice(&url, &resolved, &choice.id).expect("download subtitle");
        assert!(!transcript.cues.is_empty());
    }
}
