//! NoteService — CRUD + Markdown export.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::notes::error::NoteError;
use crate::notes::headings::NotesExportHeadings;
use crate::notes::model::{Note, NoteCreate, NotePreviewQuotes, NoteQuote, NoteUpdate};
use crate::notes::quotes::{QuoteResolveInput, resolve_quotes};
use crate::notes::store::{self, default_store_path};
use crate::subtitle::service::SubtitleService;

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
            .map_err(|_| NoteError::internal(Some("notes mutex poisoned")))?;
        let mut notes = store::load(&self.path)?;
        notes.retain(|n| n.media_path == media_path);
        notes.sort_by_key(|n| n.position_ms);
        Ok(notes)
    }

    pub fn preview_quotes(&self, input: NotePreviewQuotes) -> Result<Vec<NoteQuote>, NoteError> {
        if input.media_path.trim().is_empty() {
            return Err(NoteError::invalid("媒体路径不能为空"));
        }
        resolve_quotes_for_input(
            &input.media_path,
            input.subtitle_choice_id.as_deref(),
            input.position_ms,
            input.anchor_cue_index,
            input.quote_cue_indices.as_deref(),
            input.quote_hint.as_deref(),
        )
    }

    pub fn create(&self, input: NoteCreate) -> Result<Note, NoteError> {
        let body = input.body.trim().to_string();
        if body.is_empty() {
            return Err(NoteError::invalid("笔记内容不能为空"));
        }
        if input.media_path.trim().is_empty() {
            return Err(NoteError::invalid("媒体路径不能为空"));
        }

        let include_quotes = input.include_quotes.unwrap_or(true);
        let quotes = if include_quotes {
            resolve_quotes_for_input(
                &input.media_path,
                input.subtitle_choice_id.as_deref(),
                input.position_ms,
                input.anchor_cue_index,
                input.quote_cue_indices.as_deref(),
                input.quote_hint.as_deref(),
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };

        let _guard = self
            .lock
            .lock()
            .map_err(|_| NoteError::internal(Some("notes mutex poisoned")))?;
        let mut notes = store::load(&self.path)?;
        let now = now_iso();
        let note = Note {
            id: new_id(),
            media_path: input.media_path,
            position_ms: input.position_ms,
            body,
            quotes,
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
            .map_err(|_| NoteError::internal(Some("notes mutex poisoned")))?;
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
            .map_err(|_| NoteError::internal(Some("notes mutex poisoned")))?;
        let mut notes = store::load(&self.path)?;
        let before = notes.len();
        notes.retain(|n| n.id != id);
        if notes.len() == before {
            return Err(NoteError::not_found(id));
        }
        store::save(&self.path, &notes)?;
        Ok(())
    }

    pub fn export_markdown(
        &self,
        media_path: &str,
        headings: &NotesExportHeadings,
    ) -> Result<String, NoteError> {
        let mut notes = self.list_for_media(media_path)?;
        notes.sort_by_key(|note| note_range_ms(note).0);

        let mut out = format!(
            "# {}\n\n## {}\n\n",
            headings.document_title, headings.episode_heading
        );
        if notes.is_empty() {
            out.push_str("_（暂无笔记）_\n");
            return Ok(out);
        }
        for note in notes {
            let (start_ms, end_ms) = note_range_ms(&note);
            out.push_str(&format!(
                "### {}\n\n",
                format_range_heading(start_ms, end_ms)
            ));
            out.push_str(&note.body);
            out.push('\n');
            if !note.quotes.is_empty() {
                out.push_str("\n**引用台词**\n\n");
                let quote_count = note.quotes.len();
                for (index, quote) in note.quotes.iter().enumerate() {
                    let text = quote.text.replace('\n', " ");
                    // Trailing two spaces = hard line break inside one blockquote paragraph.
                    let line_break = if index + 1 < quote_count { "  \n" } else { "\n" };
                    let line = if quote.anchor {
                        format!("> **{text}**{line_break}")
                    } else {
                        format!("> {text}{line_break}")
                    };
                    out.push_str(&line);
                }
            }
            out.push('\n');
        }
        Ok(out)
    }

    pub fn export_markdown_to_file(
        &self,
        media_path: &str,
        dest_path: &std::path::Path,
        headings: &NotesExportHeadings,
    ) -> Result<(), NoteError> {
        let markdown = self.export_markdown(media_path, headings)?;
        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    tracing::warn!(%error, "failed to create notes export dir");
                    NoteError::io(Some(&format!("create export dir: {error}")))
                })?;
            }
        }
        std::fs::write(dest_path, markdown).map_err(|error| {
            tracing::warn!(%error, "failed to write notes export file");
            NoteError::io(Some(&format!("write export file: {error}")))
        })?;
        Ok(())
    }
}

impl Default for NoteService {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_quotes_for_input(
    media_path: &str,
    subtitle_choice_id: Option<&str>,
    position_ms: u64,
    anchor_cue_index: Option<u32>,
    quote_cue_indices: Option<&[u32]>,
    quote_hint: Option<&str>,
) -> Result<Vec<NoteQuote>, NoteError> {
    let Some(choice_id) = subtitle_choice_id.filter(|id| !id.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let transcript = match SubtitleService::load_choice(media_path, choice_id) {
        Ok(transcript) => transcript,
        Err(error) => {
            tracing::warn!(%error, "failed to load subtitle for note quotes");
            return Ok(Vec::new());
        }
    };
    Ok(resolve_quotes(QuoteResolveInput {
        cues: &transcript.cues,
        position_ms,
        anchor_cue_index,
        quote_cue_indices,
        quote_hint,
    }))
}

fn new_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("note-{ms}-{}", simple_rand())
}

fn simple_rand() -> u32 {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1)
        .wrapping_mul(2654435761))
        % 1_000_000
}

fn now_iso() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{ms}")
}

fn format_timestamp(ms: u64) -> String {
    let total = ms / 1000;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn note_range_ms(note: &Note) -> (u64, u64) {
    if note.quotes.is_empty() {
        return (note.position_ms, note.position_ms);
    }
    let start_ms = note
        .quotes
        .iter()
        .map(|quote| quote.start_ms)
        .min()
        .unwrap_or(note.position_ms);
    let end_ms = note
        .quotes
        .iter()
        .map(|quote| quote.end_ms)
        .max()
        .unwrap_or(note.position_ms);
    (start_ms, end_ms)
}

fn format_range_heading(start_ms: u64, end_ms: u64) -> String {
    if start_ms == end_ms {
        return format_timestamp(start_ms);
    }
    format!(
        "{} – {}",
        format_timestamp(start_ms),
        format_timestamp(end_ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::headings::NotesExportHeadings;
    use crate::notes::model::NoteQuote;

    #[test]
    fn create_list_export_delete() {
        let path = std::env::temp_dir().join("lumina-notes-service-test.json");
        let _ = std::fs::remove_file(&path);
        let svc = NoteService::with_path(path.clone());
        let note = svc
            .create(NoteCreate {
                media_path: r"C:\movies\a.mp4".into(),
                position_ms: 65_000,
                body: "重点".into(),
                subtitle_choice_id: None,
                anchor_cue_index: None,
                quote_cue_indices: None,
                quote_hint: None,
                include_quotes: Some(false),
            })
            .expect("create");
        let list = svc.list_for_media(r"C:\movies\a.mp4").expect("list");
        assert_eq!(list.len(), 1);
        let headings = NotesExportHeadings {
            document_title: "movies".into(),
            episode_heading: "a.mp4".into(),
        };
        let md = svc
            .export_markdown(r"C:\movies\a.mp4", &headings)
            .expect("md");
        assert!(md.contains("### 1:05"));
        assert!(md.contains("重点"));
        assert!(md.starts_with("# movies"));
        assert!(md.contains("## a.mp4"));
        svc.delete(&note.id).expect("delete");
        assert!(svc.list_for_media(r"C:\movies\a.mp4").unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_renders_quote_block() {
        let path = std::env::temp_dir().join("lumina-notes-export-quotes-test.json");
        let _ = std::fs::remove_file(&path);
        let svc = NoteService::with_path(path.clone());
        let mut notes = store::load(&path).unwrap_or_default();
        notes.push(Note {
            id: "n1".into(),
            media_path: r"D:\movie\ShowName\clip.mkv".into(),
            position_ms: 90_000,
            body: "这段很打动我".into(),
            quotes: vec![NoteQuote {
                index: 2,
                start_ms: 88_000,
                end_ms: 89_000,
                text: "前一句".into(),
                anchor: false,
            }, NoteQuote {
                index: 3,
                start_ms: 89_000,
                end_ms: 90_000,
                text: "锚点句".into(),
                anchor: true,
            }],
            created_at: "1".into(),
            updated_at: "1".into(),
        });
        store::save(&path, &notes).expect("seed");
        let headings = NotesExportHeadings {
            document_title: "ShowName".into(),
            episode_heading: "S01E01 — 第一集".into(),
        };
        let md = svc
            .export_markdown(r"D:\movie\ShowName\clip.mkv", &headings)
            .expect("md");
        assert!(md.starts_with("# ShowName"));
        assert!(md.contains("## S01E01 — 第一集"));
        assert!(md.contains("### 1:28 – 1:30"));
        assert!(md.contains("这段很打动我"));
        assert!(md.contains("**锚点句**"));
        assert!(md.contains("> 前一句  \n> **锚点句**"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_writes_markdown_file() {
        let path = std::env::temp_dir().join("lumina-notes-export-file-test.json");
        let out = std::env::temp_dir().join("lumina-notes-export-file-test.md");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);
        let svc = NoteService::with_path(path.clone());
        let mut notes = store::load(&path).unwrap_or_default();
        notes.push(Note {
            id: "n1".into(),
            media_path: r"D:\movie\ShowName\clip.mkv".into(),
            position_ms: 90_000,
            body: "导出测试".into(),
            quotes: Vec::new(),
            created_at: "1".into(),
            updated_at: "1".into(),
        });
        store::save(&path, &notes).expect("seed");
        let headings = NotesExportHeadings {
            document_title: "ShowName".into(),
            episode_heading: "clip.mkv".into(),
        };
        svc.export_markdown_to_file(r"D:\movie\ShowName\clip.mkv", &out, &headings)
            .expect("export file");
        let md = std::fs::read_to_string(&out).expect("read export");
        assert!(md.starts_with("# ShowName"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);
    }
}
