//! Serialize cues to SRT / VTT sidecar files next to media.

use std::fs;
use std::path::{Path, PathBuf};

use crate::subtitle::error::SubtitleError;
use crate::subtitle::model::{Cue, Transcript};

/// Token between media stem and extension: `video.asr.srt`.
pub const ASR_SIDECAR_TOKEN: &str = "asr";

pub fn format_srt_time(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

pub fn format_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let index = if cue.index == 0 {
            i + 1
        } else {
            cue.index as usize
        };
        out.push_str(&format!(
            "{index}\n{} --> {}\n{}\n",
            format_srt_time(cue.start_ms),
            format_srt_time(cue.end_ms),
            cue.text.trim()
        ));
    }
    out
}

pub fn format_vtt_time(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

pub fn format_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_vtt_time(cue.start_ms),
            format_vtt_time(cue.end_ms),
            cue.text.trim()
        ));
    }
    out
}

/// Sanitize a filename language/token segment (`en`, `zh`, `asr`).
pub fn normalize_lang_token(token: &str) -> Result<String, SubtitleError> {
    let trimmed = token.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return Err(SubtitleError::export_failed(Some("empty language token")));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SubtitleError::export_failed(Some(
            "language token must be ascii alphanumeric / - / _",
        )));
    }
    if trimmed.len() > 24 {
        return Err(SubtitleError::export_failed(Some(
            "language token too long",
        )));
    }
    Ok(trimmed)
}

pub fn sidecar_path(media_path: &Path, lang_token: &str) -> Result<PathBuf, SubtitleError> {
    let token = normalize_lang_token(lang_token)?;
    let parent = media_path.parent().ok_or_else(|| {
        SubtitleError::export_failed(Some("media path has no parent directory"))
    })?;
    let stem = media_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SubtitleError::export_failed(Some("media path has no file stem")))?;
    Ok(parent.join(format!("{stem}.{token}.srt")))
}

pub fn asr_sidecar_path(media_path: &Path) -> Result<PathBuf, SubtitleError> {
    sidecar_path(media_path, ASR_SIDECAR_TOKEN)
}

/// Write `{stem}.{lang}.srt` beside the media and return a transcript pointing at it.
pub fn export_sidecar_srt(
    media_path: &Path,
    lang_token: &str,
    cues: &[Cue],
) -> Result<Transcript, SubtitleError> {
    if cues.is_empty() {
        return Err(SubtitleError::export_failed(Some("no cues to export")));
    }
    let token = normalize_lang_token(lang_token)?;
    let path = sidecar_path(media_path, &token)?;
    let body = format_srt(cues);
    fs::write(&path, body).map_err(|error| {
        SubtitleError::export_failed(Some(&format!("write {}: {error}", path.display())))
    })?;
    tracing::info!(
        path = %path.display(),
        cues = cues.len(),
        token = %token,
        "sidecar subtitle written"
    );
    let path_str = path.to_string_lossy().to_string();
    Ok(Transcript {
        source_path: path_str.clone(),
        choice_id: format!("sidecar:{path_str}"),
        stream_index: None,
        language: Some(token),
        codec_name: Some("srt".into()),
        cues: cues.to_vec(),
    })
}

pub fn export_asr_sidecar(
    media_path: &Path,
    transcript: &Transcript,
) -> Result<Transcript, SubtitleError> {
    export_sidecar_srt(media_path, ASR_SIDECAR_TOKEN, &transcript.cues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtitle::parse::parse_srt;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn format_srt_roundtrips_through_parser() {
        let cues = vec![
            Cue {
                index: 1,
                start_ms: 1_500,
                end_ms: 3_000,
                text: "你好".into(),
            },
            Cue {
                index: 2,
                start_ms: 65_000,
                end_ms: 66_500,
                text: "world".into(),
            },
        ];
        let text = format_srt(&cues);
        let parsed = parse_srt(&text).expect("parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].start_ms, 1_500);
        assert_eq!(parsed[0].text, "你好");
        assert_eq!(parsed[1].start_ms, 65_000);
    }

    #[test]
    fn export_sidecar_writes_lang_token_file() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("lumina-sub-export-{suffix}"));
        fs::create_dir_all(&dir).expect("mkdir");
        let media = dir.join("demo.mkv");
        fs::write(&media, b"x").expect("touch media");

        let cues = vec![Cue {
            index: 1,
            start_ms: 0,
            end_ms: 1000,
            text: "hello".into(),
        }];
        let exported = export_sidecar_srt(&media, "en", &cues).expect("export");
        let expected = dir.join("demo.en.srt");
        assert!(expected.is_file());
        assert!(exported.choice_id.contains("demo.en.srt"));
        assert_eq!(exported.language.as_deref(), Some("en"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_lang_token_rejects_path_chars() {
        assert!(normalize_lang_token("../x").is_err());
        assert!(normalize_lang_token("zh").is_ok());
    }
}
