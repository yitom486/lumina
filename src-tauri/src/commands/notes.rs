//! Note Tauri commands.

use tauri::AppHandle;
use tauri::Manager;

use crate::notes::model::{Note, NoteCreate, NoteUpdate};
use crate::notes::{NoteError, NoteService};
use crate::state::AppState;

fn with_notes<R, F>(app: AppHandle, work: F) -> Result<R, NoteError>
where
    F: FnOnce(&NoteService) -> Result<R, NoteError> + Send + 'static,
    R: Send + 'static,
{
    // NoteService is behind AppState; use blocking path from async commands.
    let Some(state) = app.try_state::<AppState>() else {
        return Err(NoteError::internal(Some("app state unavailable")));
    };
    work(&state.notes)
}

#[tauri::command]
pub async fn notes_list(app: AppHandle, media_path: String) -> Result<Vec<Note>, NoteError> {
    tauri::async_runtime::spawn_blocking(move || with_notes(app, move |notes| notes.list_for_media(&media_path)))
        .await
        .map_err(|error| NoteError::internal(Some(&format!("notes list join: {error}"))))?
}

#[tauri::command]
pub async fn notes_create(app: AppHandle, input: NoteCreate) -> Result<Note, NoteError> {
    tauri::async_runtime::spawn_blocking(move || with_notes(app, move |notes| notes.create(input)))
        .await
        .map_err(|error| NoteError::internal(Some(&format!("notes create join: {error}"))))?
}

#[tauri::command]
pub async fn notes_update(app: AppHandle, input: NoteUpdate) -> Result<Note, NoteError> {
    tauri::async_runtime::spawn_blocking(move || with_notes(app, move |notes| notes.update(input)))
        .await
        .map_err(|error| NoteError::internal(Some(&format!("notes update join: {error}"))))?
}

#[tauri::command]
pub async fn notes_delete(app: AppHandle, id: String) -> Result<(), NoteError> {
    tauri::async_runtime::spawn_blocking(move || with_notes(app, move |notes| notes.delete(&id)))
        .await
        .map_err(|error| NoteError::internal(Some(&format!("notes delete join: {error}"))))?
}

#[tauri::command]
pub async fn notes_export_markdown(
    app: AppHandle,
    media_path: String,
) -> Result<String, NoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_notes(app, move |notes| notes.export_markdown(&media_path))
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes export join: {error}"))))?
}
