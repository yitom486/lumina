//! NoteService — CRUD + Markdown export.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::notes::error::NoteError;
use crate::notes::model::{Note, NoteCreate, NoteUpdate};
use crate::notes::store::{self, default_store_path};

pub struct NoteService {
    path: PathBuf,
    lock: Mutex<()>,
}

impl NoteService {
    pub fn new() -> Self {
        Self {
            path: default_store_path(),
            lock: Mutex::new(()),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn list_for_media(&self, media_path: &str) -> Result<Vec<Note>, NoteError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| NoteError::internal("笔记锁异常", None))?;
        let mut notes = store::load(&self.path)?;
        notes.retain(|n| n.media_path == media_path);
        notes.sort_by_key(|n| n.position_ms);
        Ok(notes)
    }

    pub fn create(&self, input: NoteCreate) -> Result<Note, NoteError> {
        let body = input.body.trim().to_string();
        if body.is_empty() {
            return Err(NoteError::invalid("笔记内容不能为空"));
        }
        if input.media_path.trim().is_empty() {
            return Err(NoteError::invalid("媒体路径不能为空"));
        }

        let _guard = self
            .lock
            .lock()
            .map_err(|_| NoteError::internal("笔记锁异常", None))?;
        let mut notes = store::load(&self.path)?;
        let now = now_iso();
        let note = Note {
            id: new_id(),
            media_path: input.media_path,
            position_ms: input.position_ms,
            body,
            created_at: now.clone(),
            updated_at: now,
        };
        notes.push(note.clone());
        store::save(&self.path, &notes)?;
        Ok(note)
    }

    pub fn update(&self, input: NoteUpdate) -> Result<Note, NoteError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| NoteError::internal("笔记锁异常", None))?;
        let mut notes = store::load(&self.path)?;
        let note = notes
            .iter_mut()
            .find(|n| n.id == input.id)
            .ok_or_else(|| NoteError::not_found(&input.id))?;
        if let Some(body) = input.body {
            let trimmed = body.trim().to_string();
            if trimmed.is_empty() {
                return Err(NoteError::invalid("笔记内容不能为空"));
            }
            note.body = trimmed;
        }
        if let Some(position_ms) = input.position_ms {
            note.position_ms = position_ms;
        }
        note.updated_at = now_iso();
        let cloned = note.clone();
        store::save(&self.path, &notes)?;
        Ok(cloned)
    }

    pub fn delete(&self, id: &str) -> Result<(), NoteError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| NoteError::internal("笔记锁异常", None))?;
        let mut notes = store::load(&self.path)?;
        let before = notes.len();
        notes.retain(|n| n.id != id);
        if notes.len() == before {
            return Err(NoteError::not_found(id));
        }
        store::save(&self.path, &notes)?;
        Ok(())
    }

    pub fn export_markdown(&self, media_path: &str) -> Result<String, NoteError> {
        let notes = self.list_for_media(media_path)?;
        let title = std::path::Path::new(media_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(media_path);
        let mut out = format!("# Notes — {title}\n\n");
        if notes.is_empty() {
            out.push_str("_（暂无笔记）_\n");
            return Ok(out);
        }
        for note in notes {
            out.push_str(&format!(
                "- [{}] {}\n",
                format_mmss(note.position_ms),
                note.body.replace('\n', " ")
            ));
        }
        Ok(out)
    }
}

impl Default for NoteService {
    fn default() -> Self {
        Self::new()
    }
}

fn new_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("note-{ms}-{}", simple_rand())
}

fn simple_rand() -> u32 {
    // Cheap uniqueness without rand crate.
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1)
        .wrapping_mul(2654435761))
        % 1_000_000
}

fn now_iso() -> String {
    // Stable sortable-ish stamp without chrono crate.
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{ms}")
}

fn format_mmss(ms: u64) -> String {
    let total = ms / 1000;
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_export_delete() {
        let path = std::env::temp_dir().join("lumina-notes-service-test.json");
        let _ = std::fs::remove_file(&path);
        let svc = NoteService::with_path(path.clone());
        let note = svc
            .create(NoteCreate {
                media_path: r"C:\a.mp4".into(),
                position_ms: 65_000,
                body: "重点".into(),
            })
            .expect("create");
        let list = svc.list_for_media(r"C:\a.mp4").expect("list");
        assert_eq!(list.len(), 1);
        let md = svc.export_markdown(r"C:\a.mp4").expect("md");
        assert!(md.contains("[1:05]"));
        assert!(md.contains("重点"));
        svc.delete(&note.id).expect("delete");
        assert!(svc.list_for_media(r"C:\a.mp4").unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
