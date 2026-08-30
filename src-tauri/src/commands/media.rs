//! Media inspection Tauri commands.

use crate::media::{MediaError, MediaInfo, MediaInspector};

#[tauri::command]
pub fn media_inspect(path: String) -> Result<MediaInfo, MediaError> {
    MediaInspector::inspect(path)
}
