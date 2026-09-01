//! Shared path rules for the local media library and Lumina data dirs.
//!
//! All code that writes or reads `.lumina/**` or compares media paths against
//! library roots should go through this module so scan, index lookup, MCP tools,
//! and metadata I/O stay aligned.

use std::path::{Path, PathBuf};

use crate::library::error::LibraryError;

pub const LUMINA_DIR_NAME: &str = ".lumina";

pub fn lumina_dir(root: &Path) -> PathBuf {
    root.join(LUMINA_DIR_NAME)
}

pub fn lumina_index_path(root: &Path) -> PathBuf {
    lumina_dir(root).join("index.json")
}

pub fn lumina_groups_dir(root: &Path) -> PathBuf {
    lumina_dir(root).join("groups")
}

pub fn lumina_agent_context_path(cwd: &Path) -> PathBuf {
    lumina_dir(cwd).join("agent-context.json")
}

pub fn lumina_tmp_dir(cwd: &Path) -> PathBuf {
    lumina_dir(cwd).join("tmp")
}

pub fn is_lumina_data_dir(name: &str) -> bool {
    name == LUMINA_DIR_NAME
}

/// Normalize a path string to `/` separators and trim trailing slashes.
pub fn normalize_slashes(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized
}

/// Return `media_path` relative to `library_root` using `/` separators.
pub fn relativize_under_root(root: &Path, media_path: &Path) -> Result<String, LibraryError> {
    let root_norm = normalize_slashes(root);
    let media_norm = normalize_slashes(media_path);
    if root_norm.is_empty() || media_norm.is_empty() {
        return Err(LibraryError::invalid_input("当前媒体不在已启用目录中"));
    }
    if !is_under_root(&root_norm, &media_norm) {
        return Err(LibraryError::invalid_input("当前媒体不在已启用目录中"));
    }
    let relative = media_norm
        .get(root_norm.len() + 1..)
        .ok_or_else(|| LibraryError::invalid_input("当前媒体不在已启用目录中"))?;
    if relative.is_empty() {
        return Err(LibraryError::invalid_input("当前媒体不在已启用目录中"));
    }
    Ok(relative.to_string())
}

/// Best-effort display path relative to `root`; falls back to normalized absolute path.
pub fn display_relative_path(root: &Path, path: &Path) -> String {
    relativize_under_root(root, path).unwrap_or_else(|_| normalize_slashes(path))
}

/// Walk parents of `media_path` looking for `.lumina/index.json`.
pub fn discover_library_root_for_media(media_path: &Path) -> Option<PathBuf> {
    let mut current = media_path.parent()?;
    loop {
        if lumina_index_path(current).is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// Best-effort library root: configured roots should call this after their own lookup.
pub fn library_root_for_media_path(
    configured_roots: impl IntoIterator<Item = PathBuf>,
    media_path: &Path,
) -> Option<PathBuf> {
    configured_roots
        .into_iter()
        .filter(|root| is_media_under_root(root, media_path))
        .max_by_key(|root| root.as_os_str().len())
        .or_else(|| discover_library_root_for_media(media_path))
}

/// Whether `media_path` lives under a configured library root.
pub fn is_media_under_root(root: &Path, media_path: &Path) -> bool {
    let root_norm = normalize_slashes(root);
    let media_norm = normalize_slashes(media_path);
    is_under_root(&root_norm, &media_norm)
}

fn is_under_root(root: &str, media: &str) -> bool {
    if media == root {
        return false;
    }
    #[cfg(windows)]
    {
        media
            .to_ascii_lowercase()
            .starts_with(&format!("{}/", root.to_ascii_lowercase()))
    }
    #[cfg(not(windows))]
    {
        media.starts_with(&format!("{root}/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn lumina_paths_share_one_dir_name() {
        let root = PathBuf::from(r"D:\movie\Show");
        assert_eq!(
            lumina_index_path(&root),
            PathBuf::from(r"D:\movie\Show\.lumina\index.json")
        );
        assert_eq!(
            lumina_agent_context_path(&root),
            PathBuf::from(r"D:\movie\Show\.lumina\agent-context.json")
        );
    }

    #[test]
    fn relativize_nested_media_on_windows_style_paths() {
        let root = PathBuf::from(r"D:\movie\Show Folder");
        let media = PathBuf::from(r"D:\movie\Show Folder\S01E01.mkv");
        assert_eq!(
            relativize_under_root(&root, &media).expect("relative"),
            "S01E01.mkv"
        );
    }

    #[test]
    fn relativize_is_case_insensitive_on_windows() {
        let root = PathBuf::from(r"D:\Movie\Show");
        let media = PathBuf::from(r"d:\movie\show\ep.mkv");
        assert_eq!(
            relativize_under_root(&root, &media).expect("relative"),
            "ep.mkv"
        );
    }

    #[test]
    fn rejects_media_outside_root() {
        let root = PathBuf::from(r"D:\movie\A");
        let media = PathBuf::from(r"D:\movie\B\file.mkv");
        assert!(relativize_under_root(&root, &media).is_err());
    }
}
