//! Run whisper-cli on demand (spawn process; no in-process model preload).

use std::path::Path;

use crate::asr::error::AsrError;
use crate::asr::paths::AsrPaths;
use crate::process_util::command;
use crate::subtitle::model::Transcript;
use crate::subtitle::parse::parse_subtitle_text;

pub fn transcribe_wav(
    paths: &AsrPaths,
    wav_path: &Path,
    work_dir: &Path,
    media_path: &Path,
) -> Result<Transcript, AsrError> {
    let output_prefix = work_dir.join("asr");
    // whisper.cpp writes `${prefix}.srt` when `-osrt` is set.
    let srt_path = work_dir.join("asr.srt");
    if srt_path.exists() {
        let _ = std::fs::remove_file(&srt_path);
    }

    tracing::info!(
        cli = %paths.cli.display(),
        model = %paths.model.display(),
        wav = %wav_path.display(),
        "starting on-demand whisper-cli"
    );

    let output = command(&paths.cli)
        .args([
            "-m",
            &paths.model.to_string_lossy(),
            "-f",
            &wav_path.to_string_lossy(),
            "-osrt",
            "-of",
            &output_prefix.to_string_lossy(),
            "-l",
            "auto",
            "-np",
        ])
        .output()
        .map_err(|error| {
            tracing::warn!(%error, "whisper-cli spawn failed");
            AsrError::transcribe_failed(Some(&format!("whisper-cli spawn: {error}")))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = format!("{stderr}\n{stdout}");
        tracing::warn!(%details, "whisper-cli exited with error");
        return Err(AsrError::transcribe_failed(Some(details.trim())));
    }

    // Some builds write next to wav or use different suffix — probe common paths.
    let candidates = [
        srt_path.clone(),
        work_dir.join("asr.srt"),
        wav_path.with_extension("srt"),
        output_prefix.with_extension("srt"),
    ];
    let content = candidates
        .iter()
        .find_map(|p| {
            std::fs::read_to_string(p)
                .ok()
                .filter(|c| !c.trim().is_empty())
        })
        .ok_or_else(|| {
            AsrError::transcribe_failed(Some("expected asr.srt in the temp job directory"))
        })?;

    let cues = parse_subtitle_text(&content).map_err(|error| {
        tracing::warn!(%error, "failed to parse ASR SRT");
        AsrError::transcribe_failed(Some(&format!("parse ASR SRT: {error}")))
    })?;

    Ok(Transcript {
        source_path: media_path.to_string_lossy().to_string(),
        choice_id: "asr:on-demand".into(),
        stream_index: None,
        language: Some("auto".into()),
        codec_name: Some("whisper".into()),
        cues,
    })
}
