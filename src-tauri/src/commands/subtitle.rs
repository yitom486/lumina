//! Subtitle / transcript Tauri commands.

use crate::subtitle::{
    SubtitleChoice, SubtitleError, SubtitleErrorCode, SubtitleService, Transcript,
};

#[tauri::command]
pub async fn subtitle_list_choices(path: String) -> Result<Vec<SubtitleChoice>, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || SubtitleService::list_choices(path))
        .await
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::InternalError,
                "列出字幕任务异常结束",
                Some(error.to_string()),
            )
        })?
}

/// Heavy ffmpeg extract — must not block the UI thread.
#[tauri::command]
pub async fn subtitle_load_choice(
    path: String,
    choice_id: String,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || SubtitleService::load_choice(path, choice_id))
        .await
        .map_err(|error| {
            SubtitleError::new(
                SubtitleErrorCode::InternalError,
                "加载字幕任务异常结束",
                Some(error.to_string()),
            )
        })?
}
