//! Subtitle / transcript Tauri commands.

use crate::subtitle::{SubtitleChoice, SubtitleError, SubtitleService, Transcript};

#[tauri::command]
pub fn subtitle_list_choices(path: String) -> Result<Vec<SubtitleChoice>, SubtitleError> {
    SubtitleService::list_choices(path)
}

#[tauri::command]
pub fn subtitle_load_choice(path: String, choice_id: String) -> Result<Transcript, SubtitleError> {
    SubtitleService::load_choice(path, choice_id)
}
