//! Note Tauri commands.

use tauri::AppHandle;
use tauri::Manager;

use crate::notes::headings::resolve_export_headings;
use crate::notes::model::{Note, NoteCreate, NotePreviewQuotes, NoteUpdate};
use crate::notes::{NoteError, NoteFrameData, NoteService};
use crate::state::AppState;

fn with_notes<R, F>(app: AppHandle, work: F) -> Result<R, NoteError>
where
    F: FnOnce(&NoteService) -> Result<R, NoteError> + Send + 'static,
    R: Send + 'static,
{
    let Some(state) = app.try_state::<AppState>() else {
        return Err(NoteError::internal(Some("app state unavailable")));
    };
    work(&state.notes)
}

fn resolve_headings(
    app: &AppHandle,
    media_path: &str,
) -> Result<crate::notes::NotesExportHeadings, NoteError> {
    let Some(state) = app.try_state::<AppState>() else {
        return Err(NoteError::internal(Some("app state unavailable")));
    };
    Ok(resolve_export_headings(media_path, &state.library))
}

#[tauri::command]
pub async fn notes_list(app: AppHandle, media_path: String) -> Result<Vec<Note>, NoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_notes(app, move |notes| notes.list_for_media(&media_path))
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes list join: {error}"))))?
}

#[tauri::command]
pub async fn notes_preview_quotes(
    app: AppHandle,
    input: NotePreviewQuotes,
) -> Result<Vec<crate::notes::model::NoteQuote>, NoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_notes(app, move |notes| notes.preview_quotes(input))
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes preview join: {error}"))))?
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

/// Lazy frame thumbnail bytes (P7-M3). Missing files degrade to `Ok(None)`.
#[tauri::command]
pub async fn notes_get_frame(
    app: AppHandle,
    id: String,
) -> Result<Option<NoteFrameData>, NoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_notes(app, move |notes| notes.frame_data(&id))
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes frame join: {error}"))))?
}

#[tauri::command]
pub async fn notes_dismiss_proposal(workspace: String) -> Result<(), NoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::notes::proposal::dismiss_latest_proposal(std::path::Path::new(&workspace))
            .map_err(|details| NoteError::io(Some(&details)))
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes dismiss proposal join: {error}"))))?
}

#[tauri::command]
pub async fn notes_load_latest_proposal(
    workspace: String,
) -> Result<Option<crate::notes::VideoAnnotationProposal>, NoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::notes::proposal::load_latest_proposal(std::path::Path::new(&workspace))
            .map_err(|details| NoteError::io(Some(&details)))
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes load proposal join: {error}"))))?
}

#[tauri::command]
pub async fn notes_export_markdown(
    app: AppHandle,
    media_path: String,
) -> Result<String, NoteError> {
    let headings = resolve_headings(&app, &media_path)?;
    tauri::async_runtime::spawn_blocking(move || {
        with_notes(app, move |notes| {
            notes.export_markdown(&media_path, &headings)
        })
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes export join: {error}"))))?
}

#[tauri::command]
pub async fn notes_export_markdown_to_file(
    app: AppHandle,
    media_path: String,
    dest_path: String,
) -> Result<(), NoteError> {
    let headings = resolve_headings(&app, &media_path)?;
    tauri::async_runtime::spawn_blocking(move || {
        with_notes(app, move |notes| {
            notes.export_markdown_to_file(&media_path, std::path::Path::new(&dest_path), &headings)
        })
    })
    .await
    .map_err(|error| NoteError::internal(Some(&format!("notes export file join: {error}"))))?
}
