//! Resolve Markdown export headings from app-assembled media metadata
//! with filesystem fallbacks. The app resolves library metadata into
//! [`NotesMediaMetadata`]; this crate never touches the library (M7).

use std::path::Path;

/// Minimal media metadata assembled by the app (from the media library).
/// Same fallback rules as before: empty group title falls back to the
/// parent directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesMediaMetadata {
    /// Series / show name when known (empty means unknown).
    pub group_title: String,
    pub episode_title: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesExportHeadings {
    /// `#` document title — series / show name when known.
    pub document_title: String,
    /// `##` episode section — `S01E01 — {episode title}` when metadata allows.
    pub episode_heading: String,
}

pub fn resolve_export_headings(
    media_path: &str,
    metadata: Option<NotesMediaMetadata>,
) -> NotesExportHeadings {
    let path = Path::new(media_path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(media_path);
    let directory_fallback = directory_title_from_media_path(media_path);

    match metadata {
        Some(meta) => headings_from_notes_metadata(&meta, file_name, &directory_fallback),
        None => NotesExportHeadings {
            document_title: directory_fallback,
            episode_heading: file_name.to_string(),
        },
    }
}

fn headings_from_notes_metadata(
    metadata: &NotesMediaMetadata,
    file_name: &str,
    directory_fallback: &str,
) -> NotesExportHeadings {
    let group_title = metadata.group_title.trim();
    let document_title = if group_title.is_empty() {
        directory_fallback.to_string()
    } else {
        group_title.to_string()
    };

    let episode_title = metadata
        .episode_title
        .as_ref()
        .map(|title| title.trim())
        .filter(|title| !title.is_empty())
        .map(str::to_string);
    let (season, episode) = (metadata.season, metadata.episode);

    NotesExportHeadings {
        document_title,
        episode_heading: format_episode_heading(episode_title, season, episode, file_name),
    }
}

pub fn directory_title_from_media_path(media_path: &str) -> String {
    let path = Path::new(media_path);
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .or_else(|| path.file_name().and_then(|name| name.to_str()))
        .unwrap_or(media_path)
        .to_string()
}

pub fn format_episode_heading(
    episode_title: Option<String>,
    season: Option<u32>,
    episode: Option<u32>,
    file_name: &str,
) -> String {
    let code = match (season, episode) {
        (Some(season), Some(episode)) => Some(format!("S{season:02}E{episode:02}")),
        _ => None,
    };
    let title = episode_title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match (code.as_deref(), title.as_deref()) {
        (Some(code), Some(title)) => format!("{code} — {title}"),
        (Some(code), None) => format!("{code} — {file_name}"),
        (None, Some(title)) => format!("{title} — {file_name}"),
        (None, None) => file_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_heading_uses_season_episode_and_title() {
        assert_eq!(
            format_episode_heading(
                Some("可以把你的剧本给我看吗".into()),
                Some(1),
                Some(1),
                "show.S01E01.mkv",
            ),
            "S01E01 — 可以把你的剧本给我看吗",
        );
    }

    #[test]
    fn episode_heading_falls_back_to_season_episode() {
        assert_eq!(
            format_episode_heading(None, Some(1), Some(2), "show.S01E02.mkv"),
            "S01E02 — show.S01E02.mkv",
        );
    }

    #[test]
    fn episode_heading_falls_back_to_file_name() {
        assert_eq!(
            format_episode_heading(None, None, None, "movie.mkv"),
            "movie.mkv",
        );
    }
}
