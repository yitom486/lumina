//! Tool detail sanitization for user-visible ACP updates.
//! Split from `wire/protocol.rs` without behavior change.

pub fn sanitize_tool_detail(input: &str) -> String {
    let collapsed = input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("；");
    if collapsed.is_empty() {
        return String::new();
    }
    let redacted = redact_absolute_paths(&collapsed);
    let capped = truncate_chars(&redacted, 220);
    if looks_like_low_level_tool_error(&capped) {
        "工具执行未成功，Agent 将尝试其他方式继续".into()
    } else {
        capped
    }
}

fn redact_absolute_paths(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while !rest.is_empty() {
        if let Some((_, len)) = match_absolute_path_prefix(rest) {
            out.push_str("[路径]");
            rest = &rest[len..];
        } else {
            let Some(ch) = rest.chars().next() else {
                break;
            };
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

fn match_absolute_path_prefix(input: &str) -> Option<(&str, usize)> {
    if input.len() >= 3 {
        let bytes = input.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
            let mut end = 3;
            for (offset, ch) in input[3..].char_indices() {
                if ch.is_whitespace() || matches!(ch, '"' | '\'' | ')' | '(' | ',' | ';') {
                    break;
                }
                end = 3 + offset + ch.len_utf8();
            }
            return Some((&input[..end], end));
        }
    }
    if input.starts_with("\\\\?\\") {
        let end = input
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')' | '(' | ',' | ';'))
            .unwrap_or(input.len());
        return Some((&input[..end], end));
    }
    None
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    format!("{}…", input.chars().take(max_chars).collect::<String>())
}

fn looks_like_low_level_tool_error(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        "error: ",
        "traceback",
        "stack trace",
        "exit code",
        "exit status",
        "serde",
        "jsonrpc",
        "stderr",
        "ffprobe",
        "ffmpeg",
        "whisper-cli",
        "libmpv",
        "panic",
        "thread '",
        "os error",
        "command not found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_tool_detail_redacts_low_level_errors() {
        let sanitized = sanitize_tool_detail("ffprobe: exit code 1 at D:\\movie\\a.mkv");
        assert_eq!(sanitized, "工具执行未成功，Agent 将尝试其他方式继续");
    }
}
