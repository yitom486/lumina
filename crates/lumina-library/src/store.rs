//! `.lumina` store with replace-on-success writes. The media files themselves
//! are never modified and the index directory is excluded from scans.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::error::LibraryError;
use crate::model::LibraryIndex;
use crate::paths::{lumina_groups_dir, lumina_index_path};

/// The index format currently written by the scanner.  Older known shapes
/// are normalized in memory only; no legacy file is rewritten implicitly.
const CURRENT_LIBRARY_INDEX_SCHEMA_VERSION: u32 = 1;

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
    let raw: Value = serde_json::from_str(&text).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("parse {}: {error}", path.display())))
    })?;
    if let Some(schema_version) = index_schema_version(&raw) {
        if schema_version > CURRENT_LIBRARY_INDEX_SCHEMA_VERSION {
            return Err(LibraryError::storage_failed(Some(&format!(
                "unsupported library index schema {schema_version} in {}",
                path.display()
            ))));
        }
    }
    let normalized = normalize_legacy_index(raw);
    serde_json::from_value(normalized)
        .map(Some)
        .map_err(|error| {
            LibraryError::storage_failed(Some(&format!("parse {}: {error}", path.display())))
        })
}

fn index_schema_version(value: &Value) -> Option<u32> {
    value
        .get("schemaVersion")
        .or_else(|| value.get("schema_version"))
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
}

fn normalize_legacy_index(mut value: Value) -> Value {
    let Some(object) = value.as_object_mut() else {
        return value;
    };

    move_key(object, "schemaVersion", "schema_version");
    move_key(object, "updatedAtMs", "updated_at_ms");
    if !object.contains_key("schemaVersion") {
        // A pre-versioned index can only be read as legacy data.  The missing
        // version is kept as 0 in memory so the next explicit save can write
        // the current schema, while the source file remains untouched.
        object.insert("schemaVersion".to_string(), Value::from(0_u32));
    }

    if let Some(files) = object.get_mut("files").and_then(Value::as_array_mut) {
        for file in files {
            let Some(file) = file.as_object_mut() else {
                continue;
            };
            move_key(file, "relativePath", "relative_path");
            move_key(file, "fileName", "file_name");
            move_key(file, "sizeBytes", "size_bytes");
            move_key(file, "modifiedAtMs", "modified_at_ms");
            move_key(file, "groupKey", "group_key");
        }
    }

    if let Some(groups) = object.get_mut("groups").and_then(Value::as_array_mut) {
        for group in groups {
            let Some(group) = group.as_object_mut() else {
                continue;
            };
            move_key(group, "displayName", "display_name");
            move_key(group, "manualTitle", "manual_title");
        }
    }

    value
}

fn move_key(object: &mut serde_json::Map<String, Value>, current: &str, legacy: &str) {
    if object.contains_key(current) {
        return;
    }
    if let Some(value) = object.remove(legacy) {
        object.insert(current.to_string(), value);
    }
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
    fs::create_dir_all(parent).map_err(|error| storage_write_error("mkdir", parent, &error))?;
    let encoded = serde_json::to_vec_pretty(index)
        .map_err(|error| LibraryError::storage_failed(Some(&format!("encode index: {error}"))))?;
    let temp = parent.join(format!(".index-{}.tmp", unique_suffix()));
    // A failed write must not leave its tmp behind either: the periodic
    // sweeper would otherwise report `swept=1` forever with no error logged.
    if let Err(error) = fs::write(&temp, &encoded) {
        let _ = fs::remove_file(&temp);
        return Err(storage_write_error("write", &temp, &error));
    }
    if let Err(error) = replace_file(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
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

/// OS errors worth a short retry on replace. 32/33 are classic transient
/// locks; 5 (ERROR_ACCESS_DENIED) is retried too because Windows reports
/// STATUS_CANNOT_DELETE — the file momentarily held open without
/// FILE_SHARE_DELETE (Explorer preview/security dialog, AV scan on the
/// fresh tmp) — as 5, not 32. A genuine ACL denial simply exhausts the same
/// bounded attempts (~300ms) and then maps to the actionable message.
const RETRYABLE_REPLACE_OS_ERRORS: [i32; 3] = [32, 33, 5];
const REPLACE_ATTEMPTS: u32 = 3;
const REPLACE_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Map a filesystem failure to the permission-specific fixed message when
/// the OS refused access (ERROR_ACCESS_DENIED = 5: read-only destination,
/// folder ACL, elevation mismatch); everything else keeps the generic
/// message. Only the step label and the OS error enter `details`.
fn storage_write_error(step: &str, path: &Path, error: &io::Error) -> LibraryError {
    let details = format!("{step} {}: {error}", path.display());
    if error.raw_os_error() == Some(5) {
        return LibraryError::storage_access_denied(Some(&details));
    }
    LibraryError::storage_failed(Some(&details))
}

fn replace_file(temp: &Path, destination: &Path) -> Result<(), LibraryError> {
    with_replace_retry(|| replace_once(temp, destination))
        .map_err(|error| storage_write_error("replace", destination, &error))
}

fn with_replace_retry<F>(mut attempt: F) -> io::Result<()>
where
    F: FnMut() -> io::Result<()>,
{
    let mut tries = 0;
    loop {
        tries += 1;
        match attempt() {
            Ok(()) => return Ok(()),
            Err(error) if tries < REPLACE_ATTEMPTS && is_retryable_replace_error(&error) => {
                thread::sleep(REPLACE_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_retryable_replace_error(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| RETRYABLE_REPLACE_OS_ERRORS.contains(&code))
}

fn replace_once(temp: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::GetLastError;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        // Our own cache files sometimes arrive read-only (copied media
        // roots, sync tools). A replace onto a read-only destination always
        // fails with ERROR_ACCESS_DENIED, so clear the flag best-effort.
        clear_readonly_flag(destination);
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
        // Read GetLastError directly so the raw Win32 code (32/33 vs 5)
        // survives into the retry decision instead of an HRESULT wrapper.
        unsafe {
            if MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .is_err()
            {
                return Err(io::Error::from_raw_os_error(GetLastError().0 as i32));
            }
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        fs::rename(temp, destination)
    }
}

#[cfg(windows)]
// Windows-only: clearing the read-only bit here never makes the file
// world-writable (that caveat is Unix-specific), it only unblocks replacing
// our own cache file.
#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly_flag(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
}

/// Remove leftover atomic-write temp files (`.index-*.tmp`, `.*.tmp`) under
/// `<root>/.lumina`. Best-effort: any failure is ignored so a dirty cache dir
/// can never break a scan. Returns the removed count (logs/tests only).
pub fn cleanup_stale_tmps(root: &Path) -> usize {
    fn visit(dir: &Path, removed: &mut usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, removed);
                continue;
            }
            let is_stale_tmp = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.') && name.ends_with(".tmp"));
            if is_stale_tmp && fs::remove_file(&path).is_ok() {
                *removed += 1;
            }
        }
    }

    let mut removed = 0;
    visit(&root.join(crate::paths::LUMINA_DIR_NAME), &mut removed);
    removed
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
    fs::create_dir_all(&dir).map_err(|error| storage_write_error("mkdir", &dir, &error))?;
    let destination = dir.join(file_name);
    let temp = dir.join(format!(".{file_name}-{}.tmp", unique_suffix()));
    let encoded = serde_json::to_vec_pretty(value).map_err(|error| {
        LibraryError::storage_failed(Some(&format!("encode group JSON: {error}")))
    })?;
    if let Err(error) = fs::write(&temp, &encoded) {
        let _ = fs::remove_file(&temp);
        return Err(storage_write_error("write", &temp, &error));
    }
    if let Err(error) = replace_file(&temp, &destination) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
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
    use crate::model::{GroupResolution, MediaGroupKind};

    fn test_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn load_normalizes_known_legacy_index_keys_without_rewriting_source() {
        let root = test_root("lumina-store-legacy-index");
        let path = index_path(&root);
        let parent = match path.parent() {
            Some(parent) => parent,
            None => panic!("index path has no parent"),
        };
        fs::create_dir_all(parent).expect("mkdir");
        fs::write(
            &path,
            r#"{
                "schema_version": 1,
                "root": "D:/media",
                "updated_at_ms": 1,
                "files": [{
                    "relative_path": "Example.Show/S01E02.mkv",
                    "file_name": "S01E02.mkv",
                    "size_bytes": 1,
                    "modified_at_ms": 1,
                    "group_key": "Example.Show",
                    "season": 1,
                    "episode": 2
                }],
                "groups": [{
                    "key": "Example.Show",
                    "display_name": "Example Show",
                    "kind": "series",
                    "files": ["Example.Show/S01E02.mkv"],
                    "resolution": {
                        "state": "matched",
                        "tmdb_id": 42,
                        "media_type": "tv"
                    }
                }]
            }"#,
        )
        .expect("seed legacy index");

        let index = load(&root)
            .expect("legacy index should be readable")
            .expect("legacy index should exist");
        assert_eq!(index.schema_version, 1);
        assert_eq!(index.files[0].relative_path, "Example.Show/S01E02.mkv");
        assert_eq!(
            index.groups[0].resolution,
            GroupResolution::Matched {
                tmdb_id: 42,
                media_type: crate::model::MetadataMediaType::Tv,
            }
        );
        let original = fs::read_to_string(&path).expect("read source");
        assert!(original.contains("schema_version"));
        assert!(!original.contains("schemaVersion"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn future_index_schema_is_rejected_without_overwriting_the_file() {
        let root = test_root("lumina-store-future-index");
        let path = index_path(&root);
        let parent = match path.parent() {
            Some(parent) => parent,
            None => panic!("index path has no parent"),
        };
        fs::create_dir_all(parent).expect("mkdir");
        let original = r#"{
            "schemaVersion": 99,
            "root": "D:/media",
            "updatedAtMs": 1,
            "files": [],
            "groups": []
        }"#;
        fs::write(&path, original).expect("seed future index");

        let error = load(&root).expect_err("future schema should not be accepted");
        assert_eq!(error.code, crate::error::LibraryErrorCode::StorageFailed);
        assert_eq!(error.message, "媒体索引保存失败，请重试");
        assert_eq!(fs::read_to_string(&path).expect("read source"), original);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_removes_only_stale_tmps() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-store-tmp-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let base = dir.join(".lumina").join("groups").join("g");
        fs::create_dir_all(&base).expect("mkdir");
        fs::write(base.join(".index-1.tmp"), b"x").expect("seed tmp");
        fs::write(base.join(".data-2.tmp"), b"x").expect("seed tmp");
        fs::write(base.join("index.json"), b"{}").expect("seed real");
        fs::write(dir.join(".lumina").join(".top-3.tmp"), b"x").expect("seed top");
        assert_eq!(cleanup_stale_tmps(&dir), 3);
        assert!(base.join("index.json").is_file());
        assert!(!base.join(".index-1.tmp").exists());
        assert!(!dir.join(".lumina").join(".top-3.tmp").exists());
        // Missing .lumina dir is not an error.
        assert_eq!(cleanup_stale_tmps(&dir.join("absent")), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_retry_covers_sharing_locks_and_cannot_delete() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // ERROR_SHARING_VIOLATION twice, then success: all three attempts run.
        let calls = AtomicUsize::new(0);
        let result = with_replace_retry(|| {
            let attempt = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt < REPLACE_ATTEMPTS as usize {
                Err(io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), REPLACE_ATTEMPTS as usize);
        // ERROR_ACCESS_DENIED is retried as well: Windows reports
        // STATUS_CANNOT_DELETE (file briefly held without FILE_SHARE_DELETE)
        // as 5, not 32. The raw code survives for the access-denied mapping.
        let calls = AtomicUsize::new(0);
        let result = with_replace_retry(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(io::Error::from_raw_os_error(5))
        });
        let error = result.expect_err("persistent denial must still fail");
        assert_eq!(error.raw_os_error(), Some(5));
        assert_eq!(calls.load(Ordering::SeqCst), REPLACE_ATTEMPTS as usize);
        // Anything else (e.g. file not found) still fails fast.
        let calls = AtomicUsize::new(0);
        let result = with_replace_retry(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(io::Error::from_raw_os_error(2))
        });
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn access_denied_maps_to_actionable_fixed_message() {
        let error = storage_write_error(
            "replace",
            Path::new("index.json"),
            &io::Error::from_raw_os_error(5),
        );
        assert_eq!(error.code, crate::error::LibraryErrorCode::StorageFailed);
        assert_eq!(
            error.message,
            "媒体库文件无法写入，可能被其他程序占用，请关闭相关程序后重试"
        );
        let other = storage_write_error(
            "replace",
            Path::new("index.json"),
            &io::Error::from_raw_os_error(2),
        );
        assert_eq!(other.message, "媒体索引保存失败，请重试");
    }

    #[test]
    fn failed_group_save_leaves_no_stale_tmp() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-store-fail-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let root = dir.join("media");
        let group_key = "Locked.Group";
        // Block the destination with a non-empty dir so the replace fails on
        // every platform; the write tmp must still be cleaned up.
        let blocker = group_dir(&root, group_key).join("movie.json");
        fs::create_dir_all(blocker.join("child")).expect("seed blocker dir");
        let error = save_group_json(&root, group_key, "movie.json", &serde_json::json!({"a": 1}))
            .expect_err("replace onto a non-empty dir must fail");
        assert_eq!(error.code, crate::error::LibraryErrorCode::StorageFailed);
        let leftovers: Vec<_> = fs::read_dir(group_dir(&root, group_key))
            .expect("read group dir")
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with('.') && name.ends_with(".tmp"))
            })
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn carries_confirmed_resolution_into_a_new_scan() {
        let old = LibraryIndex {
            schema_version: 1,
            root: "D:/media".into(),
            updated_at_ms: 1,
            files: Vec::new(),
            groups: vec![crate::model::MediaGroup {
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
            groups: vec![crate::model::MediaGroup {
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
