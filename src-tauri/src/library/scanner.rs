//! Dependency-free scanner and filename grouping. The watcher periodically
//! invokes this scanner, so changes are detected without a platform-specific
//! filesystem watcher or an external process.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::library::error::LibraryError;
use crate::library::model::{
    GroupResolution, IndexedMediaFile, LibraryIndex, MediaGroup, MediaGroupKind,
};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "webm", "m4v", "ts", "wmv", "flv",
];

pub fn scan_root(root: &Path) -> Result<LibraryIndex, LibraryError> {
    if !root.is_dir() {
        return Err(LibraryError::invalid_directory(Some(
            &root.display().to_string(),
        )));
    }

    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let groups = build_groups(&files);
    Ok(LibraryIndex {
        schema_version: 1,
        root: root.to_string_lossy().to_string(),
        updated_at_ms: now_ms(),
        files,
        groups,
    })
}

fn visit(root: &Path, dir: &Path, output: &mut Vec<IndexedMediaFile>) -> Result<(), LibraryError> {
    let entries = fs::read_dir(dir).map_err(|error| {
        LibraryError::scan_failed(Some(&format!("read {}: {error}", dir.display())))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| LibraryError::scan_failed(Some(&error.to_string())))?;
        let path = entry.path();
        if path.file_name().and_then(|v| v.to_str()) == Some(".lumina") {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| LibraryError::scan_failed(Some(&error.to_string())))?;
        if file_type.is_dir() {
            visit(root, &path, output)?;
        } else if file_type.is_file() && is_video(&path) {
            output.push(index_file(root, &path)?);
        }
    }
    Ok(())
}

fn index_file(root: &Path, path: &Path) -> Result<IndexedMediaFile, LibraryError> {
    let metadata = fs::metadata(path).map_err(|error| {
        LibraryError::scan_failed(Some(&format!("metadata {}: {error}", path.display())))
    })?;
    let relative = path.strip_prefix(root).map_err(|error| {
        LibraryError::scan_failed(Some(&format!("relative path {}: {error}", path.display())))
    })?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let parsed = parse_filename(&file_name);
    Ok(IndexedMediaFile {
        relative_path: normalize_path(relative),
        file_name,
        size_bytes: metadata.len(),
        modified_at_ms: metadata
            .modified()
            .ok()
            .map(system_time_ms)
            .unwrap_or_default(),
        group_key: parsed.key,
        season: parsed.season,
        episode: parsed.episode,
    })
}

fn build_groups(files: &[IndexedMediaFile]) -> Vec<MediaGroup> {
    let mut by_key: BTreeMap<String, Vec<&IndexedMediaFile>> = BTreeMap::new();
    for file in files {
        by_key.entry(file.group_key.clone()).or_default().push(file);
    }
    by_key
        .into_iter()
        .map(|(key, files)| {
            let has_episode = files.iter().any(|file| file.episode.is_some());
            MediaGroup {
                display_name: key.replace('.', " "),
                key,
                kind: if has_episode {
                    MediaGroupKind::Series
                } else {
                    MediaGroupKind::Movie
                },
                files: files
                    .into_iter()
                    .map(|file| file.relative_path.clone())
                    .collect(),
                manual_title: None,
                resolution: GroupResolution::Pending,
            }
        })
        .collect()
}

struct ParsedFilename {
    key: String,
    season: Option<u32>,
    episode: Option<u32>,
}

fn parse_filename(file_name: &str) -> ParsedFilename {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);
    let normalized = stem.replace([' ', '_', '-'], ".");
    let upper = normalized.to_ascii_uppercase();
    if let Some((at, season, episode)) = find_season_episode(&upper) {
        return ParsedFilename {
            key: clean_group_key(&normalized[..at]),
            season: Some(season),
            episode: Some(episode),
        };
    }
    if let Some((at, season, episode)) = find_x_episode(&upper) {
        return ParsedFilename {
            key: clean_group_key(&normalized[..at]),
            season: Some(season),
            episode: Some(episode),
        };
    }
    ParsedFilename {
        key: clean_group_key(&normalized),
        season: None,
        episode: None,
    }
}

fn find_season_episode(value: &str) -> Option<(usize, u32, u32)> {
    let bytes = value.as_bytes();
    for index in 0..bytes.len().saturating_sub(5) {
        if bytes[index] != b'S' {
            continue;
        }
        let season_end = index + 3;
        let episode_mark = index + 3;
        let episode_end = index + 6;
        if bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
            && bytes.get(index + 2).is_some_and(u8::is_ascii_digit)
            && bytes.get(episode_mark) == Some(&b'E')
            && bytes.get(index + 4).is_some_and(u8::is_ascii_digit)
            && bytes.get(index + 5).is_some_and(u8::is_ascii_digit)
        {
            let season = value.get(index + 1..season_end)?.parse().ok()?;
            let episode = value.get(index + 4..episode_end)?.parse().ok()?;
            return Some((index, season, episode));
        }
    }
    None
}

fn find_x_episode(value: &str) -> Option<(usize, u32, u32)> {
    let bytes = value.as_bytes();
    for index in 0..bytes.len().saturating_sub(3) {
        if !bytes[index].is_ascii_digit() {
            continue;
        }
        let season_end = index + 1;
        let marker = index + 1;
        let episode_end = index + 4;
        if bytes.get(marker) == Some(&b'X')
            && bytes.get(index + 2).is_some_and(u8::is_ascii_digit)
            && bytes.get(index + 3).is_some_and(u8::is_ascii_digit)
        {
            let season = value.get(index..season_end)?.parse().ok()?;
            let episode = value.get(index + 2..episode_end)?.parse().ok()?;
            return Some((index, season, episode));
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

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn system_time_ms(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_tv_episode_names() {
        let parsed = parse_filename("We.Are.All.Trying.Here.S01E02.1080p.mkv");
        assert_eq!(parsed.key, "We.Are.All.Trying.Here");
        assert_eq!(parsed.season, Some(1));
        assert_eq!(parsed.episode, Some(2));
    }

    #[test]
    fn groups_movies_without_episode_syntax() {
        let parsed = parse_filename("Inception.2010.1080p.mkv");
        assert_eq!(parsed.key, "Inception.2010.1080p");
        assert_eq!(parsed.season, None);
    }

    #[test]
    fn parses_x_style_episode_names() {
        let parsed = parse_filename("Example.Show.1x02.720p.mkv");
        assert_eq!(parsed.key, "Example.Show");
        assert_eq!(parsed.season, Some(1));
        assert_eq!(parsed.episode, Some(2));
    }
}
