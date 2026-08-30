//! List video files in the same directory as a media path (for playlist).

use std::path::{Path, PathBuf};

use crate::media::error::MediaError;

const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "m4v", "wmv", "flv", "ts", "m2ts"];

pub fn list_sibling_videos(file_path: impl AsRef<Path>) -> Result<Vec<String>, MediaError> {
    let path = file_path.as_ref();
    if !path.is_file() {
        return Err(MediaError::file_not_found(&path.to_string_lossy()));
    }
    let parent = path.parent().ok_or_else(|| {
        MediaError::internal("media path has no parent directory", Some(&path.to_string_lossy()))
    })?;

    let mut items: Vec<PathBuf> = std::fs::read_dir(parent)
        .map_err(|error| {
            MediaError::internal(
                "failed to read media directory",
                Some(&error.to_string()),
            )
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && is_video(p))
        .collect();

    items.sort_by(|a, b| {
        let an = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let bn = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
        an.to_ascii_lowercase()
            .cmp(&bn.to_ascii_lowercase())
    });

    Ok(items
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
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
