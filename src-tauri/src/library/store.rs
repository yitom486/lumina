//! `.lumina` store with replace-on-success writes. The media files themselves
//! are never modified and the index directory is excluded from scans.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::library::error::LibraryError;
use crate::library::model::LibraryIndex;
use crate::library::paths::{lumina_groups_dir, lumina_index_path};

pub fn index_path(root: &Path) -> PathBuf {
    lumina_index_path(root)
}

pub fn load(root: &Path) -> Result<Option<LibraryIndex>, LibraryError> {
    let path = index_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("read {}: {error}", path.display())))
    })?;
    serde_json::from_str(&text).map(Some).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("parse {}: {error}", path.display())))
    })
}

pub fn save_if_changed(root: &Path, index: &LibraryIndex) -> Result<bool, LibraryError> {
    let previous = load(root)?;
    if previous
        .as_ref()
        .is_some_and(|old| same_content(old, index))
    {
        return Ok(false);
    }
    let path = index_path(root);
    let parent = path
        .parent()
        .ok_or_else(|| LibraryError::storage_failed(Some("index path has no parent")))?;
    fs::create_dir_all(parent).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("mkdir {}: {error}", parent.display())))
    })?;
    let encoded = serde_json::to_vec_pretty(index)
        .map_err(|error| LibraryError::storage_failed(Some(&format!("encode index: {error}"))))?;
    let temp = parent.join(format!(".index-{}.tmp", unique_suffix()));
    fs::write(&temp, encoded).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("write {}: {error}", temp.display())))
    })?;
    replace_file(&temp, &path)?;
    Ok(true)
}

pub fn preserve_resolutions(
    mut next: LibraryIndex,
    previous: Option<&LibraryIndex>,
) -> LibraryIndex {
    let Some(previous) = previous else {
        return next;
    };
    for group in &mut next.groups {
        if let Some(old) = previous.groups.iter().find(|old| old.key == group.key) {
            group.resolution = old.resolution.clone();
            group.manual_title = old.manual_title.clone();
        }
    }
    next
}

fn replace_file(temp: &Path, destination: &Path) -> Result<(), LibraryError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let from: Vec<u16> = temp
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // Same-directory write + replace is an atomic NTFS metadata operation.
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|error| {
            LibraryError::storage_failed(Some(&format!(
                "replace {}: {error}",
                destination.display()
            )))
        })?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        fs::rename(temp, destination).map_err(|error| {
            LibraryError::storage_failed(Some(&format!(
                "rename {}: {error}",
                destination.display()
            )))
        })
    }
}

pub fn group_dir(root: &Path, group_key: &str) -> PathBuf {
    let readable: String = group_key
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            control if control.is_control() => '_',
            other => other,
        })
        .collect();
    let readable = readable.trim_matches(['.', ' ']);
    let readable = if readable.is_empty() {
        "untitled"
    } else {
        readable
    };
    lumina_groups_dir(root).join(format!("{readable}-{:08x}", stable_hash(group_key)))
}

pub fn save_group_json<T: serde::Serialize>(
    root: &Path,
    group_key: &str,
    file_name: &str,
    value: &T,
) -> Result<PathBuf, LibraryError> {
    let dir = group_dir(root, group_key);
    fs::create_dir_all(&dir).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("mkdir {}: {error}", dir.display())))
    })?;
    let destination = dir.join(file_name);
    let temp = dir.join(format!(".{file_name}-{}.tmp", unique_suffix()));
    let encoded = serde_json::to_vec_pretty(value).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("encode group JSON: {error}")))
    })?;
    fs::write(&temp, encoded).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("write {}: {error}", temp.display())))
    })?;
    replace_file(&temp, &destination)?;
    Ok(destination)
}

pub fn load_group_json<T: serde::de::DeserializeOwned>(
    root: &Path,
    group_key: &str,
    file_name: &str,
) -> Result<Option<T>, LibraryError> {
    let path = group_dir(root, group_key).join(file_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("read {}: {error}", path.display())))
    })?;
    serde_json::from_str(&text).map(Some).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("parse {}: {error}", path.display())))
    })
}

fn stable_hash(value: &str) -> u32 {
    value.bytes().fold(0x811c9dc5, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

fn same_content(left: &LibraryIndex, right: &LibraryIndex) -> bool {
    left.root == right.root && left.files == right.files && left.groups == right.groups
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::model::{GroupResolution, MediaGroupKind};

    #[test]
    fn carries_confirmed_resolution_into_a_new_scan() {
        let old = LibraryIndex {
            schema_version: 1,
            root: "D:/media".into(),
            updated_at_ms: 1,
            files: Vec::new(),
            groups: vec![crate::library::model::MediaGroup {
                key: "Example.Show".into(),
                display_name: "Example Show".into(),
                kind: MediaGroupKind::Series,
                files: Vec::new(),
                manual_title: None,
                resolution: GroupResolution::Ignored,
            }],
        };
        let next = LibraryIndex {
            schema_version: 1,
            root: "D:/media".into(),
            updated_at_ms: 2,
            files: Vec::new(),
            groups: vec![crate::library::model::MediaGroup {
                key: "Example.Show".into(),
                display_name: "Example Show".into(),
                kind: MediaGroupKind::Series,
                files: Vec::new(),
                manual_title: None,
                resolution: GroupResolution::Pending,
            }],
        };
        let merged = preserve_resolutions(next, Some(&old));
        assert_eq!(merged.groups[0].resolution, GroupResolution::Ignored);
    }
}
