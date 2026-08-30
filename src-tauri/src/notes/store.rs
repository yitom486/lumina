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
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
    }
}

pub fn load(path: &Path) -> Result<Vec<Note>, NoteError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(|error| {
        NoteError::io("无法读取笔记文件", Some(&error.to_string()))
    })?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed: NotesFile = serde_json::from_str(&raw).map_err(|error| {
        NoteError::io("笔记文件格式无效", Some(&error.to_string()))
    })?;
    Ok(parsed.notes)
}

pub fn save(path: &Path, notes: &[Note]) -> Result<(), NoteError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            NoteError::io("无法创建笔记目录", Some(&error.to_string()))
        })?;
    }
    let payload = NotesFile {
        notes: notes.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&payload).map_err(|error| {
        NoteError::internal("无法序列化笔记", Some(&error.to_string()))
    })?;
    fs::write(path, raw).map_err(|error| {
        NoteError::io("无法写入笔记文件", Some(&error.to_string()))
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
            created_at: "t0".into(),
            updated_at: "t0".into(),
        }];
        save(&path, &notes).expect("save");
        let loaded = load(&path).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].body, "hello");
        let _ = fs::remove_file(&path);
    }
}
