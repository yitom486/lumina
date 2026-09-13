//! Filename → (series key, season, episode), IO-free.
//!
//! Grouping rule: the series key is everything *before* the episode marker,
//! so release junk after the marker (resolution, group tags, episode titles)
//! can never split a series. Markers are tried in order; the year stays in
//! the key so remakes do not collide.
//!
//! Deliberately conservative: unknown seasons are `None` (never guessed),
//! markers must sit on a non-alphanumeric boundary, and episode digits must
//! not run into longer digit runs. Documented non-goals: 3+ digit episodes,
//! full-width digits, separator-glued markers (`夏天EP01`), multi-season
//! bare-`EP` mixes in one folder.

pub struct ParsedName {
    pub key: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

pub fn parse_filename(file_name: &str) -> ParsedName {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);
    let normalized = stem.replace([' ', '_', '-'], ".");
    let upper = normalized.to_ascii_uppercase();
    if let Some((at, season, episode)) = find_season_episode(&upper) {
        return ParsedName {
            key: clean_group_key(&normalized[..at]),
            season: Some(season),
            episode: Some(episode),
        };
    }
    if let Some((at, season, episode)) = find_x_episode(&upper) {
        return ParsedName {
            key: clean_group_key(&normalized[..at]),
            season: Some(season),
            episode: Some(episode),
        };
    }
    if let Some((at, episode)) = find_ep_episode(&upper) {
        return ParsedName {
            key: clean_group_key(&normalized[..at]),
            season: None,
            episode: Some(episode),
        };
    }
    if let Some((at, episode)) = find_cn_episode(&normalized) {
        return ParsedName {
            key: clean_group_key(&normalized[..at]),
            season: None,
            episode: Some(episode),
        };
    }
    ParsedName {
        key: clean_group_key(&normalized),
        season: None,
        episode: None,
    }
}

/// Byte sits on a marker boundary when at string start or after a separator.
/// Guards against `SHOW.1080x1920` (digit runs) and `SLEEP01` (word interiors).
fn at_boundary(bytes: &[u8], index: usize) -> bool {
    index == 0 || !bytes[index - 1].is_ascii_alphanumeric()
}

/// Parse 1–2 ASCII digits at `index`; the char after must not be a digit
/// (so `THE100` never yields `E10`, and `E021` stays unmatched).
fn take_digits(value: &str, index: usize) -> Option<(u32, usize)> {
    let bytes = value.as_bytes();
    let first = *bytes.get(index)?;
    if !first.is_ascii_digit() {
        return None;
    }
    let second = bytes.get(index + 1).copied().unwrap_or(b'.');
    if second.is_ascii_digit() {
        if bytes.get(index + 2).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        let digits = value.get(index..index + 2)?;
        return Some((digits.parse().ok()?, 2));
    }
    Some((u32::from(first - b'0'), 1))
}

/// `S01E02`, `S1E2`, `S01.E02` — 1–2 digits each side, optional dot.
/// Runs first so `S01E02` wins over the bare `E02` inside it.
fn find_season_episode(value: &str) -> Option<(usize, u32, u32)> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'S' || !at_boundary(bytes, index) {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        let Some((season, used)) = take_digits(value, cursor) else {
            index += 1;
            continue;
        };
        cursor += used;
        if bytes.get(cursor) == Some(&b'.') {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'E') {
            index += 1;
            continue;
        }
        let (episode, _) = take_digits(value, cursor + 1)?;
        return Some((index, season, episode));
    }
    None
}

/// `1x02` — legacy single-digit-season form, kept as-is plus the boundary
/// guard (previously `Show.1080x1920` misparsed as season 0 episode 19).
fn find_x_episode(value: &str) -> Option<(usize, u32, u32)> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index + 3 < bytes.len() {
        if !bytes[index].is_ascii_digit() || !at_boundary(bytes, index) {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'X')
            && bytes.get(index + 2).is_some_and(u8::is_ascii_digit)
            && bytes.get(index + 3).is_some_and(u8::is_ascii_digit)
            && !bytes.get(index + 4).is_some_and(u8::is_ascii_digit)
        {
            let season = u32::from(bytes[index] - b'0');
            let episode: u32 = value.get(index + 2..index + 4)?.parse().ok()?;
            return Some((index, season, episode));
        }
        index += 1;
    }
    None
}

/// `EP01` / `E07` without a season (`Our.Beloved.Summer.2021.EP01…`).
/// The boundary guard is load-bearing here: `SLEEP01` must not yield `EP01`.
fn find_ep_episode(value: &str) -> Option<(usize, u32)> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'E' || !at_boundary(bytes, index) {
            index += 1;
            continue;
        }
        let digit_at = if bytes.get(index + 1) == Some(&b'P') {
            index + 2
        } else {
            index + 1
        };
        if let Some((episode, _)) = take_digits(value, digit_at) {
            return Some((index, episode));
        }
        index += 1;
    }
    None
}

/// `第01集` / `第1集` — ASCII digits only in v1 (no full-width, no 3+ digits).
fn find_cn_episode(value: &str) -> Option<(usize, u32)> {
    for (at, _) in value.char_indices() {
        if !value[at..].starts_with('第') {
            continue;
        }
        let rest = &value[at + '第'.len_utf8()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !(1..=2).contains(&digits.len()) {
            continue;
        }
        if rest[digits.len()..].starts_with('集') {
            return digits.parse().ok().map(|episode| (at, episode));
        }
    }
    None
}

fn clean_group_key(value: &str) -> String {
    let trimmed = value.trim_matches('.');
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_ep_only_names_share_one_key() {
        // Real-world case: `Our.Beloved.Summer.2021.EP{01..16}…` must group.
        let names = [
            (
                "Our.Beloved.Summer.2021.EP01.HD1080P.X264.AAC.Korean.CHS.Mp4er.mp4",
                1,
            ),
            (
                "Our.Beloved.Summer.2021.EP02.HD1080P.X264.AAC.Korean.CHS.Mp4er.mp4",
                2,
            ),
            (
                "Our.Beloved.Summer.2021.EP16.HD1080P.X264.AAC.Korean.CHS.Mp4er.mp4",
                16,
            ),
        ];
        let mut keys = std::collections::BTreeSet::new();
        for (name, episode) in names {
            let parsed = parse_filename(name);
            keys.insert(parsed.key.clone());
            assert_eq!(parsed.season, None, "{name}");
            assert_eq!(parsed.episode, Some(episode), "{name}");
        }
        assert_eq!(keys.len(), 1);
        assert!(keys.contains("Our.Beloved.Summer.2021"));
    }

    #[test]
    fn single_digit_season_episode() {
        let parsed = parse_filename("Show.S1E2.1080p.mkv");
        assert_eq!(parsed.key, "Show");
        assert_eq!((parsed.season, parsed.episode), (Some(1), Some(2)));
    }

    #[test]
    fn dotted_season_episode() {
        let parsed = parse_filename("Show.S01.E02.720p.mkv");
        assert_eq!(parsed.key, "Show");
        assert_eq!((parsed.season, parsed.episode), (Some(1), Some(2)));
    }

    #[test]
    fn bare_e_episode() {
        let parsed = parse_filename("Show.E07.720p.mkv");
        assert_eq!(parsed.key, "Show");
        assert_eq!(parsed.season, None);
        assert_eq!(parsed.episode, Some(7));
    }

    #[test]
    fn chinese_episode_markers() {
        let parsed = parse_filename("剧名.第01集.1080p.mkv");
        assert_eq!(parsed.key, "剧名");
        assert_eq!(parsed.episode, Some(1));
        let glued = parse_filename("剧名第1集.mkv");
        assert_eq!(glued.key, "剧名");
        assert_eq!(glued.episode, Some(1));
    }

    #[test]
    fn movies_stay_ungrouped() {
        let parsed = parse_filename("Inception.2010.1080p.mkv");
        assert_eq!(parsed.season, None);
        assert_eq!(parsed.episode, None);
    }

    #[test]
    fn similar_titles_do_not_merge() {
        let left = parse_filename("Show.S01E01.1080p.mkv");
        let right = parse_filename("Showtime.S01E01.1080p.mkv");
        assert_ne!(left.key, right.key);
    }

    #[test]
    fn resolution_strings_do_not_parse() {
        let parsed = parse_filename("Show.1080x1920.mkv");
        assert_eq!(parsed.season, None);
        assert_eq!(parsed.episode, None);
    }

    #[test]
    fn word_interiors_do_not_parse() {
        assert_eq!(parse_filename("SLEEP01.1080p.mkv").episode, None);
        assert_eq!(parse_filename("THE100.1080p.mkv").episode, None);
    }
}
