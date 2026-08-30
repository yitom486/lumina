//! Parse SRT / WebVTT / ASS dialogue into cues.

use crate::subtitle::error::SubtitleError;
use crate::subtitle::model::Cue;

pub fn parse_subtitle_text(content: &str) -> Result<Vec<Cue>, SubtitleError> {
    let trimmed = content.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(SubtitleError::parse_failed(Some("empty subtitle content")));
    }

    if trimmed.starts_with("WEBVTT") || looks_like_vtt(trimmed) {
        return parse_vtt(trimmed);
    }

    if trimmed.contains("[Script Info]") || trimmed.contains("Dialogue:") {
        return parse_ass(trimmed);
    }

    parse_srt(trimmed)
}

fn looks_like_vtt(content: &str) -> bool {
    content.lines().any(|line| {
        line.contains("-->") && line.contains('.') && !line.contains(',')
    })
}

pub fn parse_srt(content: &str) -> Result<Vec<Cue>, SubtitleError> {
    let mut cues = Vec::new();
    let mut blocks = Vec::new();
    let mut current = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    for block in blocks {
        if let Some(cue) = parse_srt_block(&block)? {
            cues.push(cue);
        }
    }

    if cues.is_empty() {
        return Err(SubtitleError::parse_failed(Some(
            "no valid SRT cues found",
        )));
    }

    renumber(cues)
}

fn parse_srt_block(lines: &[&str]) -> Result<Option<Cue>, SubtitleError> {
    if lines.is_empty() {
        return Ok(None);
    }

    let mut idx = 0;
    // Optional numeric index line
    if lines[0].trim().chars().all(|c| c.is_ascii_digit()) && lines.len() >= 2 {
        idx = 1;
    }
    if idx >= lines.len() {
        return Ok(None);
    }

    let timing = lines[idx].trim();
    let Some((start_raw, end_raw)) = timing.split_once("-->") else {
        return Ok(None);
    };
    let start_ms = parse_srt_time(start_raw.trim()).ok_or_else(|| {
        SubtitleError::parse_failed(Some(start_raw.trim()))
    })?;
    let end_part = end_raw.split_whitespace().next().unwrap_or("");
    let end_ms = parse_srt_time(end_part).ok_or_else(|| {
        SubtitleError::parse_failed(Some(end_part))
    })?;

    let text = lines[idx + 1..]
        .iter()
        .map(|l| strip_tags(l))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if text.is_empty() {
        return Ok(None);
    }

    Ok(Some(Cue {
        index: 0,
        start_ms,
        end_ms,
        text,
    }))
}

fn parse_srt_time(value: &str) -> Option<u64> {
    // 00:01:02,345 or 00:01:02.345
    let value = value.replace(',', ".");
    let (hms, ms_part) = value.split_once('.')?;
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let s: u64 = parts.next()?.parse().ok()?;
    let mut ms_digits = ms_part.chars().take(3).collect::<String>();
    while ms_digits.len() < 3 {
        ms_digits.push('0');
    }
    let ms: u64 = ms_digits.parse().ok()?;
    Some(((h * 60 + m) * 60 + s) * 1000 + ms)
}

pub fn parse_vtt(content: &str) -> Result<Vec<Cue>, SubtitleError> {
    let body = content
        .lines()
        .skip_while(|l| !l.contains("-->"))
        .collect::<Vec<_>>();

    // Re-join from first timing line by reconstructing from original
    let mut cues = Vec::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.starts_with("WEBVTT") || line.starts_with("NOTE") || line.is_empty() {
            continue;
        }
        let timing_line = if line.contains("-->") {
            line
        } else if lines.peek().is_some_and(|l| l.contains("-->")) {
            lines.next().map(str::trim).unwrap_or("")
        } else {
            continue;
        };
        if !timing_line.contains("-->") {
            continue;
        }
        let Some((start_raw, rest)) = timing_line.split_once("-->") else {
            continue;
        };
        let end_raw = rest.split_whitespace().next().unwrap_or("");
        let start_ms = parse_vtt_time(start_raw.trim()).ok_or_else(|| {
            SubtitleError::parse_failed(Some(start_raw.trim()))
        })?;
        let end_ms = parse_vtt_time(end_raw).ok_or_else(|| {
            SubtitleError::parse_failed(Some(end_raw))
        })?;

        let mut text_lines = Vec::new();
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() {
                lines.next();
                break;
            }
            if next.contains("-->") {
                break;
            }
            text_lines.push(strip_tags(lines.next().unwrap_or("")));
        }
        let text = text_lines.join("\n").trim().to_string();
        if text.is_empty() {
            continue;
        }
        cues.push(Cue {
            index: 0,
            start_ms,
            end_ms,
            text,
        });
    }

    // silence unused
    let _ = body;

    if cues.is_empty() {
        return Err(SubtitleError::parse_failed(Some(
            "no valid WebVTT cues found",
        )));
    }
    renumber(cues)
}

fn parse_vtt_time(value: &str) -> Option<u64> {
    // 00:01:02.345 or 01:02.345
    let value = value.trim();
    let (hms, ms_part) = value.split_once('.')?;
    let parts: Vec<&str> = hms.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minute_part, second_part] => {
            let minutes: u64 = minute_part.parse().ok()?;
            let seconds: u64 = second_part.parse().ok()?;
            (0_u64, minutes, seconds)
        }
        [hour_part, minute_part, second_part] => {
            let hours: u64 = hour_part.parse().ok()?;
            let minutes: u64 = minute_part.parse().ok()?;
            let seconds: u64 = second_part.parse().ok()?;
            (hours, minutes, seconds)
        }
        _ => return None,
    };
    let mut ms_digits = ms_part.chars().take(3).collect::<String>();
    while ms_digits.len() < 3 {
        ms_digits.push('0');
    }
    let ms: u64 = ms_digits.parse().ok()?;
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + ms)
}

pub fn parse_ass(content: &str) -> Result<Vec<Cue>, SubtitleError> {
    let mut cues = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("Dialogue:") {
            continue;
        }
        let rest = line.trim_start_matches("Dialogue:").trim();
        // Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
        let parts: Vec<&str> = split_ass_fields(rest, 10);
        if parts.len() < 10 {
            continue;
        }
        let start_ms = parse_ass_time(parts[1]).ok_or_else(|| {
            SubtitleError::parse_failed(Some(parts[1]))
        })?;
        let end_ms = parse_ass_time(parts[2]).ok_or_else(|| {
            SubtitleError::parse_failed(Some(parts[2]))
        })?;
        let text = strip_ass_overrides(parts[9..].join(",").trim());
        if text.is_empty() {
            continue;
        }
        cues.push(Cue {
            index: 0,
            start_ms,
            end_ms,
            text,
        });
    }

    if cues.is_empty() {
        return Err(SubtitleError::parse_failed(Some(
            "no ASS Dialogue entries found",
        )));
    }
    renumber(cues)
}

fn split_ass_fields(line: &str, min_fields: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    for _ in 0..(min_fields - 1) {
        if let Some((field, next)) = rest.split_once(',') {
            out.push(field.trim());
            rest = next;
        } else {
            break;
        }
    }
    out.push(rest);
    out
}

fn parse_ass_time(value: &str) -> Option<u64> {
    // H:MM:SS.cs (centiseconds)
    let value = value.trim();
    let (hms, frac) = value.split_once('.')?;
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let s: u64 = parts.next()?.parse().ok()?;
    let mut cs = frac.chars().take(2).collect::<String>();
    while cs.len() < 2 {
        cs.push('0');
    }
    let cs: u64 = cs.parse().ok()?;
    Some(((h * 60 + m) * 60 + s) * 1000 + cs * 10)
}

fn strip_ass_overrides(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            for n in chars.by_ref() {
                if n == '}' {
                    break;
                }
            }
            continue;
        }
        if c == '\\'
            && (chars.peek() == Some(&'N')
                || chars.peek() == Some(&'n')
                || chars.peek() == Some(&'h'))
        {
            chars.next();
            out.push('\n');
            continue;
        }
        out.push(c);
    }
    out.replace("\\N", "\n")
        .replace("\\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_tags(line: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in line.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn renumber(mut cues: Vec<Cue>) -> Result<Vec<Cue>, SubtitleError> {
    for cue in &mut cues {
        cue.text = clean_cue_text(&cue.text);
    }
    cues.retain(|c| !c.text.is_empty());
    cues.sort_by_key(|c| c.start_ms);
    for (i, cue) in cues.iter_mut().enumerate() {
        cue.index = i as u32;
    }
    if cues.is_empty() {
        return Err(SubtitleError::parse_failed(Some(
            "no cues left after cleanup",
        )));
    }
    Ok(cues)
}

fn clean_cue_text(text: &str) -> String {
    strip_ass_overrides(&strip_tags(text))
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_srt() {
        let raw = "1\n00:00:01,000 --> 00:00:02,500\nHello world\n\n2\n00:00:03,000 --> 00:00:04,000\nSecond\n";
        let cues = parse_srt(raw).expect("srt");
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 2500);
        assert_eq!(cues[0].text, "Hello world");
    }

    #[test]
    fn parse_ass_dialogue() {
        let raw = "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.50,Default,,0,0,0,,Hello {\\i1}world{\\i0}\n";
        let cues = parse_ass(raw).expect("ass");
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].text, "Hello world");
    }

    #[test]
    fn empty_content_is_chinese_parse_error() {
        let err = parse_subtitle_text("").expect_err("empty");
        assert_eq!(err.code, crate::subtitle::error::SubtitleErrorCode::ParseFailed);
        assert!(err.message.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
    }

    #[test]
    fn parse_simple_vtt() {
        let raw = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi\n";
        let cues = parse_subtitle_text(raw).expect("vtt");
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].text, "Hi");
    }

    #[test]
    fn invalid_srt_body_is_parse_failed() {
        let err = parse_srt("not a subtitle").expect_err("bad");
        assert_eq!(err.message, "无法解析字幕");
        assert!(err.details.as_deref().is_some_and(|d| d.contains("SRT") || d.contains("cue")));
    }
}
