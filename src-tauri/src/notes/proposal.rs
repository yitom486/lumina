//! Agent-proposed video annotations — pending user confirmation on disk.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::notes::model::NoteQuote;
use crate::notes::quotes::{QuoteResolveInput, resolve_quotes};
use crate::subtitle::SubtitleService;

pub const LATEST_PROPOSAL_FILE: &str = "latest-annotation-proposal.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VideoAnnotationProposal {
    pub proposal_id: String,
    pub media_path: String,
    pub position_ms: u64,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_choice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_cue_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_cue_indices: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_hint: Option<String>,
    #[serde(default)]
    pub include_quotes: bool,
    #[serde(default)]
    pub quotes: Vec<NoteQuote>,
    pub preview_markdown: String,
    pub created_at_ms: u128,
}

pub fn proposal_path(workspace: &Path) -> PathBuf {
    resolve_workspace_dir(workspace)
        .join(".lumina")
        .join(LATEST_PROPOSAL_FILE)
}

fn resolve_workspace_dir(workspace: &Path) -> PathBuf {
    if workspace.is_dir() {
        return workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    }
    workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf())
}

pub fn save_latest_proposal(workspace: &Path, proposal: &VideoAnnotationProposal) -> Result<(), String> {
    let workspace = resolve_workspace_dir(workspace);
    let path = proposal_path(&workspace);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create proposal dir: {error}"))?;
    }
    let payload = serde_json::to_string_pretty(proposal)
        .map_err(|error| format!("encode proposal: {error}"))?;
    fs::write(&path, payload).map_err(|error| format!("write proposal: {error}"))
}

pub fn load_latest_proposal(workspace: &Path) -> Result<Option<VideoAnnotationProposal>, String> {
    let workspace = resolve_workspace_dir(workspace);
    let path = proposal_path(&workspace);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|error| format!("read proposal: {error}"))?;
    let proposal = serde_json::from_str(&raw).map_err(|error| format!("parse proposal: {error}"))?;
    Ok(Some(proposal))
}

pub fn dismiss_latest_proposal(workspace: &Path) -> Result<(), String> {
    let workspace = resolve_workspace_dir(workspace);
    let path = proposal_path(&workspace);
    if path.is_file() {
        fs::remove_file(&path).map_err(|error| format!("remove proposal: {error}"))?;
    }
    Ok(())
}

pub fn build_proposal(
    media_path: &str,
    position_ms: u64,
    body: &str,
    subtitle_choice_id: Option<String>,
    anchor_cue_index: Option<u32>,
    quote_cue_indices: Option<Vec<u32>>,
    quote_hint: Option<String>,
    include_quotes: bool,
) -> Result<VideoAnnotationProposal, String> {
    let body = body.trim();
    if body.is_empty() {
        return Err("批注内容不能为空".into());
    }
    if media_path.trim().is_empty() {
        return Err("当前没有可绑定的媒体".into());
    }

    let quotes = if include_quotes {
        resolve_proposal_quotes(
            media_path,
            subtitle_choice_id.as_deref(),
            position_ms,
            anchor_cue_index,
            quote_cue_indices.as_deref(),
            quote_hint.as_deref(),
        )
    } else {
        Vec::new()
    };

    let preview_markdown = render_proposal_preview(body, position_ms, &quotes);
    Ok(VideoAnnotationProposal {
        proposal_id: new_proposal_id(),
        media_path: media_path.to_string(),
        position_ms,
        body: body.to_string(),
        subtitle_choice_id,
        anchor_cue_index,
        quote_cue_indices,
        quote_hint,
        include_quotes,
        quotes,
        preview_markdown,
        created_at_ms: now_ms(),
    })
}

fn resolve_proposal_quotes(
    media_path: &str,
    subtitle_choice_id: Option<&str>,
    position_ms: u64,
    anchor_cue_index: Option<u32>,
    quote_cue_indices: Option<&[u32]>,
    quote_hint: Option<&str>,
) -> Vec<NoteQuote> {
    let Some(choice_id) = subtitle_choice_id.filter(|id| !id.trim().is_empty()) else {
        return Vec::new();
    };
    let transcript = match SubtitleService::load_choice(
        std::path::Path::new(media_path),
        choice_id,
    ) {
        Ok(transcript) => transcript,
        Err(error) => {
            tracing::warn!(%error, "failed to load subtitle for annotation proposal");
            return Vec::new();
        }
    };
    resolve_quotes(QuoteResolveInput {
        cues: &transcript.cues,
        position_ms,
        anchor_cue_index,
        quote_cue_indices,
        quote_hint,
    })
}

pub fn render_proposal_preview(body: &str, position_ms: u64, quotes: &[NoteQuote]) -> String {
    let (start_ms, end_ms) = quote_range_ms(position_ms, quotes);
    let mut out = format!("### {}\n\n{body}\n", format_range_heading(start_ms, end_ms));
    if !quotes.is_empty() {
        out.push_str("\n**引用台词**\n\n");
        let quote_count = quotes.len();
        for (index, quote) in quotes.iter().enumerate() {
            let line_break = if index + 1 < quote_count { "  \n" } else { "\n" };
            let line = if quote.anchor {
                format!("> **{}**{line_break}", quote.text.replace('\n', " "))
            } else {
                format!("> {}{line_break}", quote.text.replace('\n', " "))
            };
            out.push_str(&line);
        }
    }
    out
}

fn quote_range_ms(position_ms: u64, quotes: &[NoteQuote]) -> (u64, u64) {
    if quotes.is_empty() {
        return (position_ms, position_ms);
    }
    let start_ms = quotes
        .iter()
        .map(|quote| quote.start_ms)
        .min()
        .unwrap_or(position_ms);
    let end_ms = quotes
        .iter()
        .map(|quote| quote.end_ms)
        .max()
        .unwrap_or(position_ms);
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

fn new_proposal_id() -> String {
    let ms = now_ms();
    format!("proposal-{ms}")
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_latest_proposal_file() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-proposal-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let proposal = VideoAnnotationProposal {
            proposal_id: "proposal-1".into(),
            media_path: r"D:\show\S01E01.mkv".into(),
            position_ms: 90_000,
            body: "这段很打动我".into(),
            subtitle_choice_id: Some("embedded:0".into()),
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: None,
            include_quotes: true,
            quotes: vec![NoteQuote {
                index: 1,
                start_ms: 88_000,
                end_ms: 89_000,
                text: "前一句".into(),
                anchor: false,
            }],
            preview_markdown: "### 1:28\n\n这段很打动我".into(),
            created_at_ms: 1,
        };
        save_latest_proposal(&dir, &proposal).expect("save");
        let loaded = load_latest_proposal(&dir)
            .expect("load")
            .expect("some");
        assert_eq!(loaded, proposal);
        dismiss_latest_proposal(&dir).expect("dismiss");
        assert!(load_latest_proposal(&dir).expect("load").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn preview_uses_hard_line_breaks_in_quotes() {
        let preview = render_proposal_preview(
            "感想",
            90_000,
            &[
                NoteQuote {
                    index: 1,
                    start_ms: 88_000,
                    end_ms: 89_000,
                    text: "第一句".into(),
                    anchor: false,
                },
                NoteQuote {
                    index: 2,
                    start_ms: 89_000,
                    end_ms: 90_000,
                    text: "第二句".into(),
                    anchor: true,
                },
            ],
        );
        assert!(preview.contains("> 第一句  \n> **第二句**"));
    }
}
