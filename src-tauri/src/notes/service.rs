//! NoteService — CRUD + Markdown export.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::notes::error::NoteError;
use crate::notes::headings::NotesExportHeadings;
use crate::notes::model::{
    Note, NoteCreate, NoteFrame, NoteFrameData, NotePreviewQuotes, NoteQuote, NoteUpdate,
};
use crate::notes::quotes::{resolve_quotes, QuoteResolveInput};
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

    fn frames_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(|parent| parent.join("note-frames"))
            .unwrap_or_else(std::env::temp_dir)
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

        // Slow ffmpeg capture stays outside the store mutex; the id is minted first.
        let id = new_id();
        let frames = if input.include_frame.unwrap_or(false) {
            capture_note_frame(
                &self.frames_dir(),
                &id,
                &input.media_path,
                input.position_ms,
            )
            .into_iter()
            .collect()
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
            id,
            media_path: input.media_path,
            position_ms: input.position_ms,
            body,
            quotes,
            frames,
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
        // Best-effort frame GC: a missing file is fine, the note is gone either way.
        let _ = std::fs::remove_file(self.frames_dir().join(format!("{id}.jpg")));
        Ok(())
    }

    /// Lazy thumbnail bytes for one note (P7-M3). Missing files degrade to
    /// `Ok(None)` so a moved/deleted frame never breaks the notes list.
    pub fn frame_data(&self, id: &str) -> Result<Option<NoteFrameData>, NoteError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| NoteError::internal(Some("notes mutex poisoned")))?;
        let notes = store::load(&self.path)?;
        let Some(note) = notes.iter().find(|note| note.id == id) else {
            return Err(NoteError::not_found(id));
        };
        if note.frames.is_empty() {
            return Ok(None);
        }
        // Containment: ids come from our own JSON, but a hand-edited file
        // must not turn this into an arbitrary file read.
        if id.contains(['/', '\\']) {
            return Ok(None);
        }
        let frames_dir = self.frames_dir();
        let path = frames_dir.join(format!("{id}.jpg"));
        if path.parent() != Some(frames_dir.as_path()) {
            return Ok(None);
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(%error, note_id = %id, "note frame missing");
                return Ok(None);
            }
        };
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        Ok(Some(NoteFrameData {
            mime: "image/jpeg".into(),
            data: STANDARD.encode(bytes),
        }))
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
                    let line_break = if index + 1 < quote_count {
                        "  \n"
                    } else {
                        "\n"
                    };
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

/// Capture one frame at `at_ms` into the durable frames dir (P7-M3).
/// Best-effort by contract: any failure (no ffmpeg, missing media) only
/// warns and yields `None` — the note itself always saves.
fn capture_note_frame(
    frames_dir: &std::path::Path,
    note_id: &str,
    media_path: &str,
    at_ms: u64,
) -> Option<NoteFrame> {
    use crate::media::frame_capture::capture_frames;

    if note_id.contains(['/', '\\']) {
        return None;
    }
    if std::fs::create_dir_all(frames_dir).is_err() {
        return None;
    }
    // Unique scratch dir: capture_frames names outputs frame-00.jpg, so two
    // concurrent creates must not share a directory.
    let scratch = frames_dir.join(format!("tmp-{note_id}"));
    if std::fs::create_dir_all(&scratch).is_err() {
        return None;
    }
    let time_sec = at_ms as f64 / 1000.0;
    let outputs = match capture_frames(std::path::Path::new(media_path), &[time_sec], &scratch) {
        Ok(outputs) => outputs,
        Err(error) => {
            tracing::warn!(note_id, "note frame capture skipped: {}", error.message);
            let _ = std::fs::remove_dir_all(&scratch);
            return None;
        }
    };
    let captured = outputs.into_iter().next();
    let dest = frames_dir.join(format!("{note_id}.jpg"));
    let renamed = captured.is_some_and(|captured| {
        dest.parent() == Some(frames_dir) && std::fs::rename(&captured, &dest).is_ok()
    });
    let _ = std::fs::remove_dir_all(&scratch);
    if !renamed {
        return None;
    }
    Some(NoteFrame {
        at_ms,
        file: format!("{note_id}.jpg"),
    })
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
                include_frame: None,
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
    fn create_with_frame_attaches_and_delete_cleans_file() {
        if crate::media::tools::resolve_ffmpeg().is_err() {
            eprintln!("SKIP note frame: ffmpeg not vendored on this machine");
            return;
        }
        let dir = std::env::temp_dir().join(format!("lumina-note-frame-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("frame test dir");
        let ffmpeg = crate::media::tools::resolve_ffmpeg().expect("ffmpeg");
        let media = dir.join("clip.mp4");
        let status = crate::process_util::command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=5:size=640x360:rate=30",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-an",
            ])
            .arg(&media)
            .output()
            .expect("spawn ffmpeg");
        assert!(status.status.success(), "frame fixture failed to encode");

        let svc = NoteService::with_path(dir.join("notes.json"));
        let note = svc
            .create(NoteCreate {
                media_path: media.to_string_lossy().into_owned(),
                position_ms: 2_000,
                body: "带图".into(),
                subtitle_choice_id: None,
                anchor_cue_index: None,
                quote_cue_indices: None,
                quote_hint: None,
                include_quotes: Some(false),
                include_frame: Some(true),
            })
            .expect("create with frame");
        assert_eq!(note.frames.len(), 1);
        assert_eq!(note.frames[0].at_ms, 2_000);
        let frame_path = dir.join("note-frames").join(format!("{}.jpg", note.id));
        assert!(frame_path.is_file(), "frame file missing");

        let data = svc.frame_data(&note.id).expect("frame data").expect("some");
        assert_eq!(data.mime, "image/jpeg");
        assert!(data.data.starts_with("/9j/"), "expected JPEG bytes");

        svc.delete(&note.id).expect("delete");
        assert!(!frame_path.exists(), "frame file cleaned");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_degrades_to_frameless_when_capture_fails() {
        let dir = std::env::temp_dir().join(format!("lumina-note-noframe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("frame test dir");
        // Not a real video: capture must fail, the note must still save.
        let media = dir.join("empty.mp4");
        std::fs::write(&media, b"not a video").expect("seed");
        let svc = NoteService::with_path(dir.join("notes.json"));
        let note = svc
            .create(NoteCreate {
                media_path: media.to_string_lossy().into_owned(),
                position_ms: 1_000,
                body: "无图也存".into(),
                subtitle_choice_id: None,
                anchor_cue_index: None,
                quote_cue_indices: None,
                quote_hint: None,
                include_quotes: Some(false),
                include_frame: Some(true),
            })
            .expect("create");
        assert!(note.frames.is_empty(), "failed capture must not attach");
        assert!(svc.frame_data(&note.id).expect("frame query").is_none());
        let _ = std::fs::remove_dir_all(&dir);
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
            quotes: vec![
                NoteQuote {
                    index: 2,
                    start_ms: 88_000,
                    end_ms: 89_000,
                    text: "前一句".into(),
                    anchor: false,
                },
                NoteQuote {
                    index: 3,
                    start_ms: 89_000,
                    end_ms: 90_000,
                    text: "锚点句".into(),
                    anchor: true,
                },
            ],
            frames: Vec::new(),
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
            frames: Vec::new(),
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
