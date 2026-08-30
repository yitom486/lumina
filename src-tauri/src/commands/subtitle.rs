//! Subtitle / transcript Tauri commands.

use crate::subtitle::{SubtitleError, SubtitleService, SubtitleTrackInfo, Transcript};

#[tauri::command]
pub fn subtitle_list_tracks(path: String) -> Result<Vec<SubtitleTrackInfo>, SubtitleError> {
    SubtitleService::list_tracks(path)
}

#[tauri::command]
pub fn subtitle_load_transcript(
    path: String,
    stream_index: u32,
) -> Result<Transcript, SubtitleError> {
    SubtitleService::load_from_media(path, stream_index)
}

#[tauri::command]
pub fn subtitle_load_external(path: String) -> Result<Transcript, SubtitleError> {
    SubtitleService::load_external(path)
}
