//! List video files in the same directory as a media path (for playlist).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::media::error::MediaError;

const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "m4v", "wmv", "flv", "ts", "m2ts",
];

pub fn list_sibling_videos(file_path: impl AsRef<Path>) -> Result<Vec<String>, MediaError> {
    let path = file_path.as_ref();
    if !path.is_file() {
        return Err(MediaError::file_not_found(&path.to_string_lossy()));
    }
    let parent = path.parent().ok_or_else(|| {
        MediaError::internal(Some(&format!(
            "media path has no parent: {}",
            path.to_string_lossy()
        )))
    })?;

    let mut items: Vec<PathBuf> = std::fs::read_dir(parent)
        .map_err(|error| {
            tracing::warn!(%error, "failed to read media sibling dir");
            MediaError::internal(Some(&format!("read media dir: {error}")))
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && is_video(p))
        .collect();

    items.sort_by(|a, b| compare_file_names(file_name(a), file_name(b)));

    Ok(items
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

/// Series episodes sort by their semantic season/episode numbers, regardless
/// of punctuation in the rest of the filename. Other media uses a natural
/// filename comparison, so `Part 2` still precedes `Part 10`.
fn compare_file_names(left: &str, right: &str) -> Ordering {
    match (episode_order(left), episode_order(right)) {
        (Some(left_episode), Some(right_episode)) => left_episode
            .cmp(&right_episode)
            .then_with(|| natural_cmp(left, right)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => natural_cmp(left, right),
    }
}

fn episode_order(name: &str) -> Option<(u32, u32)> {
    let upper = name.to_ascii_uppercase();
    find_season_episode(&upper)
        .or_else(|| find_x_episode(&upper))
        .or_else(|| find_named_episode(&upper))
        .or_else(|| find_chinese_episode(name))
        .or_else(|| find_standalone_episode(&upper))
}

fn find_season_episode(value: &str) -> Option<(u32, u32)> {
    let bytes = value.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] != b'S' {
            continue;
        }
        let Some((season, after_season)) = parse_ascii_number(bytes, index + 1) else {
            continue;
        };
        let episode_marker = skip_separators(bytes, after_season);
        if bytes.get(episode_marker) != Some(&b'E') {
            continue;
        }
        let Some((episode, _)) = parse_ascii_number(bytes, episode_marker + 1) else {
            continue;
        };
        if season > 0 && episode > 0 {
            return Some((season, episode));
        }
    }
    None
}

fn find_x_episode(value: &str) -> Option<(u32, u32)> {
    let bytes = value.as_bytes();
    for index in 0..bytes.len() {
        let Some((season, after_season)) = parse_ascii_number(bytes, index) else {
            continue;
        };
        if bytes.get(after_season) != Some(&b'X') {
            continue;
        }
        let (episode, _) = parse_ascii_number(bytes, after_season + 1)?;
        if season > 0 && episode > 0 {
            return Some((season, episode));
        }
    }
    None
}

fn find_named_episode(value: &str) -> Option<(u32, u32)> {
    for marker in ["EPISODE", "EP", "E"] {
        let mut offset = 0;
        while let Some(found) = value[offset..].find(marker) {
            let index = offset + found;
            let after_marker = skip_separators(value.as_bytes(), index + marker.len());
            if let Some((episode, _)) = parse_ascii_number(value.as_bytes(), after_marker) {
                if episode > 0 {
                    return Some((1, episode));
                }
            }
            offset = index + marker.len();
        }
    }
    None
}

fn find_chinese_episode(value: &str) -> Option<(u32, u32)> {
    let mut rest = value;
    while let Some(start) = rest.find('第') {
        let after_marker = &rest[start + '第'.len_utf8()..];
        let digits: String = after_marker
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !digits.is_empty() && after_marker[digits.len()..].starts_with('集') {
            if let Ok(episode) = digits.parse::<u32>() {
                if episode > 0 {
                    return Some((1, episode));
                }
            }
        }
        rest = after_marker;
    }
    None
}

/// Handles names such as `Show.02.1080p.mkv` without mistaking common codec
/// and resolution numbers (`1080p`, `x264`, `AAC2.0`) for an episode number.
fn find_standalone_episode(value: &str) -> Option<(u32, u32)> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let Some((number, end)) = parse_ascii_number(bytes, index) else {
            index += 1;
            continue;
        };

        let preceded_by_word = index > 0 && bytes[index - 1].is_ascii_alphanumeric();
        let followed_by_word = bytes.get(end).is_some_and(u8::is_ascii_alphanumeric);
        let is_resolution = bytes
            .get(end)
            .is_some_and(|byte| matches!(byte, b'P' | b'K'));
        if (1..=999).contains(&number) && !preceded_by_word && !followed_by_word && !is_resolution {
            return Some((1, number));
        }
        index = end;
    }
    None
}

fn parse_ascii_number(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    if !bytes.get(start).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let number = std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()?;
    Some((number, end))
}

fn skip_separators(bytes: &[u8], mut index: usize) -> usize {
    while bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b' ' | b'.' | b'_' | b'-'))
    {
        index += 1;
    }
    index
}

fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let (mut left_index, mut right_index) = (0, 0);

    loop {
        left_index = skip_ignored_symbols(left_bytes, left_index);
        right_index = skip_ignored_symbols(right_bytes, right_index);
        if left_index == left_bytes.len() || right_index == right_bytes.len() {
            break;
        }

        let left_digit = left_bytes[left_index].is_ascii_digit();
        let right_digit = right_bytes[right_index].is_ascii_digit();
        if left_digit && right_digit {
            let left_end = number_end(left_bytes, left_index);
            let right_end = number_end(right_bytes, right_index);
            let ordering = compare_number_text(
                &left_bytes[left_index..left_end],
                &right_bytes[right_index..right_end],
            );
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let ordering = left_bytes[left_index].cmp(&right_bytes[right_index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_index += 1;
        right_index += 1;
    }

    match (
        left_index == left_bytes.len(),
        right_index == right_bytes.len(),
    ) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        // Different spellings can normalize to the same text. Keep the output
        // deterministic without letting punctuation affect normal ordering.
        (true, true) => left.cmp(&right),
        (false, false) => Ordering::Equal,
    }
}

fn skip_ignored_symbols(bytes: &[u8], mut index: usize) -> usize {
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii() && !byte.is_ascii_alphanumeric())
    {
        index += 1;
    }
    index
}

fn number_end(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    index
}

fn compare_number_text(left: &[u8], right: &[u8]) -> Ordering {
    let left_trimmed = trim_leading_zeroes(left);
    let right_trimmed = trim_leading_zeroes(right);
    left_trimmed
        .len()
        .cmp(&right_trimmed.len())
        .then_with(|| left_trimmed.cmp(right_trimmed))
        .then_with(|| left.len().cmp(&right.len()))
}

fn trim_leading_zeroes(value: &[u8]) -> &[u8] {
    let non_zero = value.iter().position(|byte| *byte != b'0');
    non_zero.map_or(&value[value.len().saturating_sub(1)..], |index| {
        &value[index..]
    })
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            VIDEO_EXTS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_video_filters_extensions() {
        assert!(is_video(Path::new("a.MP4")));
        assert!(is_video(Path::new("b.mkv")));
        assert!(!is_video(Path::new("c.txt")));
        assert!(!is_video(Path::new("d")));
    }

    #[test]
    fn missing_file_lists_as_not_found() {
        let err = list_sibling_videos(r"Z:\lumina-no-such-file.mp4").expect_err("missing");
        assert_eq!(err.code, crate::media::error::MediaErrorCode::FileNotFound);
    }

    #[test]
    fn sorts_mixed_punctuation_series_by_episode_number() {
        let mut names = vec![
            "We Are All Trying Here S01E06 1080p.mkv",
            "We Are All Trying Here S01E01 1080p.mkv",
            "We.Are.All.Trying.Here.S01E02.1080p.mkv",
            "We.Are.All.Trying.Here.S01E03.1080p.mkv",
            "We Are All Trying Here S01E08 1080p.mkv",
        ];

        names.sort_by(|left, right| compare_file_names(left, right));

        assert_eq!(
            names,
            vec![
                "We Are All Trying Here S01E01 1080p.mkv",
                "We.Are.All.Trying.Here.S01E02.1080p.mkv",
                "We.Are.All.Trying.Here.S01E03.1080p.mkv",
                "We Are All Trying Here S01E06 1080p.mkv",
                "We Are All Trying Here S01E08 1080p.mkv",
            ]
        );
    }

    #[test]
    fn recognizes_common_episode_naming_variants() {
        assert_eq!(episode_order("Show S2 E03.mkv"), Some((2, 3)));
        assert_eq!(episode_order("Show 1x04.mkv"), Some((1, 4)));
        assert_eq!(episode_order("Show - Episode 05.mkv"), Some((1, 5)));
        assert_eq!(episode_order("剧集 第06集.mkv"), Some((1, 6)));
        assert_eq!(episode_order("Show.07.1080p.x264.mkv"), Some((1, 7)));
    }

    #[test]
    fn interleaves_standalone_episode_numbers_with_s_e_names() {
        let mut names = vec!["Show S01E03.mkv", "Show.02.1080p.mkv", "Show S01E01.mkv"];

        names.sort_by(|left, right| compare_file_names(left, right));

        assert_eq!(
            names,
            vec!["Show S01E01.mkv", "Show.02.1080p.mkv", "Show S01E03.mkv"]
        );
    }

    #[test]
    fn naturally_sorts_files_without_episode_markers() {
        let mut names = vec!["Movie Part 10.mkv", "Movie Part 2.mkv", "Movie Part 1.mkv"];

        names.sort_by(|left, right| compare_file_names(left, right));

        assert_eq!(
            names,
            vec!["Movie Part 1.mkv", "Movie Part 2.mkv", "Movie Part 10.mkv"]
        );
    }

    #[test]
    fn ignores_special_symbols_during_natural_sorting() {
        let mut names = vec!["My.Show-10.mkv", "My Show (2).mkv", "My_Show.[1].mkv"];

        names.sort_by(|left, right| compare_file_names(left, right));

        assert_eq!(
            names,
            vec!["My_Show.[1].mkv", "My Show (2).mkv", "My.Show-10.mkv"]
        );
    }
}
