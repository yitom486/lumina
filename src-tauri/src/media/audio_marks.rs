//! Non-semantic audio signals for P7 (review decision).
//!
//! Deliberately NOT laughter/applause/music recognition: `silencedetect` and
//! loudness statistics can only report silence intervals, level changes, and
//! energy-spike candidates. No semantic labels are produced anywhere here.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::media::tools::resolve_ffmpeg;
use crate::media::MediaError;
use crate::process_util::command;

/// Defaults: -30 dB floor, at least 0.5 s long.
pub const DEFAULT_SILENCE_NOISE_DB: i32 = -30;
pub const DEFAULT_SILENCE_MIN_SEC: f64 = 0.5;
/// Peaks within 3 LU of the window max, merged inside 1 s, at most 20.
pub const PEAK_PROMINENCE_LU: f64 = 3.0;
pub const PEAK_MERGE_SEC: f64 = 1.0;
pub const MAX_PEAKS: usize = 20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SilenceMark {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnergyPeak {
    pub at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioMarks {
    pub silences: Vec<SilenceMark>,
    pub peaks: Vec<EnergyPeak>,
}

/// Silence intervals inside `[from_sec, to_sec)` (input `-ss` keeps the
/// absolute timeline). Missing ffmpeg propagates the tool error.
pub fn detect_silence(
    media_path: &Path,
    from_sec: f64,
    to_sec: f64,
    noise_db: i32,
    min_sec: f64,
) -> Result<Vec<SilenceMark>, MediaError> {
    let ffmpeg = resolve_ffmpeg()?;
    let duration = (to_sec - from_sec).max(0.0);
    let output = command(&ffmpeg)
        .args(["-hide_banner", "-ss", &format!("{from_sec:.3}"), "-i"])
        .arg(media_path)
        .args([
            "-t",
            &format!("{duration:.3}"),
            "-af",
            &format!("silencedetect=noise={noise_db}dB:d={min_sec}"),
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|error| MediaError::internal(Some(&format!("spawn ffmpeg: {error}"))))?;
    if !output.status.success() {
        return Err(MediaError::internal(Some("silence detection failed")));
    }
    Ok(parse_silence_stderr(
        &String::from_utf8_lossy(&output.stderr),
        from_sec,
        to_sec,
    ))
}

/// Energy-spike candidates from EBU R128 momentary loudness: local maxima
/// within `PEAK_PROMINENCE_LU` of the window max, merged inside 1 s.
pub fn detect_loudness_peaks(
    media_path: &Path,
    from_sec: f64,
    to_sec: f64,
) -> Result<Vec<EnergyPeak>, MediaError> {
    let ffmpeg = resolve_ffmpeg()?;
    let duration = (to_sec - from_sec).max(0.0);
    let output = command(&ffmpeg)
        .args(["-hide_banner", "-ss", &format!("{from_sec:.3}"), "-i"])
        .arg(media_path)
        .args([
            "-t",
            &format!("{duration:.3}"),
            "-af",
            "ebur128",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|error| MediaError::internal(Some(&format!("spawn ffmpeg: {error}"))))?;
    if !output.status.success() {
        return Err(MediaError::internal(Some("loudness scan failed")));
    }
    Ok(pick_peaks(
        &parse_loudness_stderr(&String::from_utf8_lossy(&output.stderr)),
        from_sec,
    ))
}

fn parse_silence_stderr(stderr: &str, from_sec: f64, to_sec: f64) -> Vec<SilenceMark> {
    let mut marks = Vec::new();
    let mut start: Option<f64> = None;
    for line in stderr.lines() {
        if let Some(rest) = line.split("silence_start: ").nth(1) {
            start = rest.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(rest) = line.split("silence_end: ").nth(1) {
            let end: Option<f64> = rest.split([' ', '|']).next().and_then(|v| v.parse().ok());
            if let (Some(begin), Some(finish)) = (start.take(), end) {
                let start_ms = (begin.max(from_sec) * 1000.0) as u64;
                let end_ms = (finish.min(to_sec) * 1000.0) as u64;
                if end_ms > start_ms {
                    marks.push(SilenceMark { start_ms, end_ms });
                }
            }
        }
    }
    marks
}

/// `(time_sec, momentary_lufs)` samples from ebur128 `M:` readings.
/// Real lines separate key and value (`t: 1.0`, tab or space); tolerate both.
fn parse_loudness_stderr(stderr: &str) -> Vec<(f64, f64)> {
    let mut samples = Vec::new();
    for line in stderr.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let mut time: Option<f64> = None;
        let mut momentary: Option<f64> = None;
        let mut index = 0;
        while index < tokens.len() {
            let token = tokens[index];
            if token == "t:" || token == "M:" {
                let value = tokens.get(index + 1).and_then(|v| v.parse().ok());
                if token == "t:" {
                    time = value;
                } else {
                    momentary = value;
                }
                index += 2;
            } else if let Some(value) = token.strip_prefix("t:") {
                if !value.is_empty() {
                    time = value.parse().ok();
                }
                index += 1;
            } else if let Some(value) = token.strip_prefix("M:") {
                if !value.is_empty() {
                    momentary = value.parse().ok();
                }
                index += 1;
            } else {
                index += 1;
            }
        }
        if let (Some(time), Some(momentary)) = (time, momentary) {
            if time.is_finite() && momentary.is_finite() {
                samples.push((time, momentary));
            }
        }
    }
    samples
}

fn pick_peaks(samples: &[(f64, f64)], from_sec: f64) -> Vec<EnergyPeak> {
    let max = samples
        .iter()
        .map(|(_, loud)| *loud)
        .fold(f64::NEG_INFINITY, f64::max);
    if !max.is_finite() {
        return Vec::new();
    }
    let floor = max - PEAK_PROMINENCE_LU;
    let mut peaks: Vec<EnergyPeak> = Vec::new();
    for (index, (time, loud)) in samples.iter().enumerate() {
        if *loud < floor {
            continue;
        }
        let prev = samples.get(index.wrapping_sub(1)).map(|(_, value)| *value);
        let next = samples.get(index + 1).map(|(_, value)| *value);
        let is_peak =
            prev.is_none_or(|value| *loud >= value) && next.is_none_or(|value| *loud >= value);
        if !is_peak {
            continue;
        }
        let at_ms = ((from_sec + time) * 1000.0) as u64;
        if peaks.last().is_some_and(|last: &EnergyPeak| {
            at_ms.saturating_sub(last.at_ms) < (PEAK_MERGE_SEC * 1000.0) as u64
        }) {
            continue;
        }
        peaks.push(EnergyPeak { at_ms });
        if peaks.len() >= MAX_PEAKS {
            break;
        }
    }
    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_parser_pairs_starts_and_ends() {
        let stderr = "\
[silencedetect @ 0x1] silence_start: 1.250
[silencedetect @ 0x1] silence_end: 4.500 | silence_duration: 3.250
[silencedetect @ 0x1] silence_start: 9.000
";
        // Dangling start without end is dropped.
        assert_eq!(
            parse_silence_stderr(stderr, 0.0, 60.0),
            vec![SilenceMark {
                start_ms: 1250,
                end_ms: 4500
            }]
        );
    }

    #[test]
    fn silence_parser_clamps_to_window() {
        let stderr = "[silencedetect @ 0x1] silence_end: 4.500 | silence_duration: 3.250\n";
        // No start seen (window cut it): nothing emitted, no panic.
        assert!(parse_silence_stderr(stderr, 2.0, 60.0).is_empty());
    }

    #[test]
    fn loudness_parser_reads_momentary_values() {
        let stderr = "\
[Parsed_ebur128_0 @ 0x1] t: 1.0\tM: -30.5\tS: -30.1\tI: -30.3 LUFS\tLRA: 1.0\n\
[Parsed_ebur128_0 @ 0x1] t: 1.1\tM: -12.0\tS: -20.0\tI: -25.0 LUFS\tLRA: 2.0\n\
garbage line without readings\n";
        assert_eq!(
            parse_loudness_stderr(stderr),
            vec![(1.0, -30.5), (1.1, -12.0)]
        );
    }

    #[test]
    fn peak_picker_keeps_prominent_separated_maxima() {
        // Plateau at -10 with a single top; -30 floor elsewhere.
        let samples = vec![
            (0.0, -30.0),
            (0.1, -11.0),
            (0.2, -10.0),
            (0.3, -10.0),
            (0.4, -25.0),
            (0.5, -30.0),
            (2.0, -9.5),
            (2.1, -30.0),
        ];
        let peaks = pick_peaks(&samples, 100.0);
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[0].at_ms, 100_200);
        assert_eq!(peaks[1].at_ms, 102_000);
    }

    #[test]
    fn peak_picker_merges_close_maxima_and_caps_count() {
        let samples: Vec<(f64, f64)> = (0..100).map(|i| (i as f64 * 0.1, -10.0)).collect();
        let peaks = pick_peaks(&samples, 0.0);
        // Flat line: only the first sample is a (non-strict) maximum, rest merge.
        assert!(peaks.len() <= MAX_PEAKS);
    }

    fn make_fixture(dir: &std::path::Path, name: &str, audio: &[&str]) -> std::path::PathBuf {
        let ffmpeg = resolve_ffmpeg().expect("spike needs ffmpeg");
        let out = dir.join(name);
        let mut cmd = command(&ffmpeg);
        cmd.arg("-y");
        for input in audio {
            cmd.args(["-f", "lavfi", "-i", input]);
        }
        cmd.args([
            "-filter_complex",
            &format!(
                "{inputs}concat=n={count}:v=0:a=1",
                inputs = audio
                    .iter()
                    .enumerate()
                    .map(|(index, _)| format!("[{index}:a]"))
                    .collect::<Vec<_>>()
                    .join(""),
                count = audio.len()
            ),
            "-c:a",
            "aac",
            "-vn",
        ])
        .arg(&out);
        let status = cmd.output().expect("spawn ffmpeg");
        assert!(status.status.success(), "fixture failed: {name}");
        out
    }

    /// End-to-end on synthetic audio (sine / silence / sine).
    /// Needs ffmpeg; SKIP otherwise.
    #[test]
    fn detects_silence_in_synthetic_fixture() {
        let ffmpeg_available = resolve_ffmpeg().is_ok();
        if !ffmpeg_available {
            eprintln!("SKIP silence e2e: ffmpeg not vendored on this machine");
            return;
        }
        let dir = std::env::temp_dir().join(format!("lumina-audio-gap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("audio temp dir");
        let media = make_fixture(
            &dir,
            "gap.m4a",
            &[
                "sine=frequency=440:duration=2",
                "anullsrc=r=44100:cl=stereo:d=2",
                "sine=frequency=440:duration=2",
            ],
        );
        let marks = detect_silence(&media, 0.0, 6.0, -30, 0.5).expect("detect");
        assert_eq!(marks.len(), 1, "marks: {marks:?}");
        assert!(
            (marks[0].start_ms as i64 - 2000).abs() < 400,
            "marks: {marks:?}"
        );
        assert!(
            (marks[0].end_ms as i64 - 4000).abs() < 400,
            "marks: {marks:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Loud burst between quiet beds: exactly one peak near the burst.
    #[test]
    fn detects_burst_in_synthetic_fixture() {
        if resolve_ffmpeg().is_err() {
            eprintln!("SKIP burst e2e: ffmpeg not vendored on this machine");
            return;
        }
        let dir = std::env::temp_dir().join(format!("lumina-audio-burst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("audio temp dir");
        let media = make_fixture(
            &dir,
            "burst.m4a",
            &[
                "sine=frequency=440:duration=2,volume=0.05",
                "sine=frequency=440:duration=1,volume=1.5",
                "sine=frequency=440:duration=2,volume=0.05",
            ],
        );
        let peaks = detect_loudness_peaks(&media, 0.0, 5.0).expect("peaks");
        assert_eq!(peaks.len(), 1, "peaks: {peaks:?}");
        assert!(
            (peaks[0].at_ms as i64 - 2500).abs() < 600,
            "peaks: {peaks:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
