//! JSON file store for notes (global file under app data / temp-friendly path).

use std::fs;
use std::path::{Path, PathBuf};

use crate::notes::error::NoteError;
use crate::notes::model::Note;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct NotesFile {
    notes: Vec<Note>,
}

pub fn default_store_path() -> PathBuf {
    if let Some(dir) = dirs_next_data() {
        return dir.join("lumina").join("notes.json");
    }
    std::env::temp_dir().join("lumina-notes.json")
}

fn dirs_next_data() -> Option<PathBuf> {
    // Avoid extra dependency: use common env locations.
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
    }
}

pub fn load(path: &Path) -> Result<Vec<Note>, NoteError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(|error| {
        tracing::warn!(%error, "failed to read notes file");
        NoteError::io(Some(&format!("read notes file: {error}")))
    })?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed: NotesFile = serde_json::from_str(&raw).map_err(|error| {
        tracing::warn!(%error, "invalid notes json");
        NoteError::io(Some(&format!("parse notes file: {error}")))
    })?;
    Ok(parsed.notes)
}

pub fn save(path: &Path, notes: &[Note]) -> Result<(), NoteError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            tracing::warn!(%error, "failed to create notes dir");
            NoteError::io(Some(&format!("create notes dir: {error}")))
        })?;
    }
    let payload = NotesFile {
        notes: notes.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&payload)
        .map_err(|error| NoteError::internal(Some(&format!("serialize notes: {error}"))))?;
    fs::write(path, raw).map_err(|error| {
        tracing::warn!(%error, "failed to write notes file");
        NoteError::io(Some(&format!("write notes file: {error}")))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::model::Note;

    #[test]
    fn roundtrip_temp_file() {
        let path = std::env::temp_dir().join("lumina-notes-test-roundtrip.json");
        let _ = fs::remove_file(&path);
        let notes = vec![Note {
            id: "1".into(),
            media_path: "/a.mp4".into(),
            position_ms: 1200,
            body: "hello".into(),
            quotes: Vec::new(),
            frames: Vec::new(),
            created_at: "t0".into(),
            updated_at: "t0".into(),
        }];
        save(&path, &notes).expect("save");
        let loaded = load(&path).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].body, "hello");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn legacy_json_without_frames_still_loads() {
        let path = std::env::temp_dir().join("lumina-notes-test-legacy.json");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            r#"{"notes": [{"id": "9", "mediaPath": "/a.mp4", "positionMs": 5, "body": "old", "quotes": [], "createdAt": "t", "updatedAt": "t"}]}"#,
        )
        .expect("seed legacy");
        let loaded = load(&path).expect("load legacy");
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].frames.is_empty());
        let _ = fs::remove_file(&path);
    }
}
