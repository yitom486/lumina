//! Run whisper-cli on demand (spawn process; no in-process model preload).

use std::path::Path;
use std::process::Command;

use crate::asr::error::AsrError;
use crate::asr::paths::AsrPaths;
use crate::subtitle::parse::parse_subtitle_text;
use crate::subtitle::model::Transcript;

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

    let output = Command::new(&paths.cli)
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
            AsrError::transcribe_failed("无法启动 whisper-cli", Some(&error.to_string()))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(AsrError::transcribe_failed(
            "whisper-cli 执行失败",
            Some(format!("{stderr}\n{stdout}").trim()),
        ));
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
        .find_map(|p| std::fs::read_to_string(p).ok().filter(|c| !c.trim().is_empty()))
        .ok_or_else(|| {
            AsrError::transcribe_failed(
                "转写完成但未找到 SRT 输出",
                Some("expected asr.srt in the temp job directory"),
            )
        })?;

    let cues = parse_subtitle_text(&content).map_err(|error| {
        AsrError::transcribe_failed("无法解析 ASR 生成的 SRT", Some(&error.to_string()))
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
