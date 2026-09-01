//! Tauri boundary for the local index. These command names double as the
//! application-side tool surface that a future ACP/MCP adapter may expose.

use tauri::State;

use crate::library::{
    credentials, CredentialKind, CredentialSaveInput, CredentialStatus, CredentialValidationConfig,
    CredentialValidationResult, LibraryError, LibraryIndex, LibraryStatus, LibraryWatchConfig,
    MediaMetadataContext, MetadataMediaType, MetadataWriteResult, PendingMediaGroup,
    ResolverPreview, ResolverRunConfig, TmdbConfig,
};
use crate::state::AppState;

#[tauri::command]
pub async fn library_watch_start(
    state: State<'_, AppState>,
    config: LibraryWatchConfig,
) -> Result<LibraryStatus, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.start(config))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library start join: {error}"))))?
}

#[tauri::command]
pub async fn library_watch_stop(state: State<'_, AppState>) -> Result<LibraryStatus, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.stop())
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library stop join: {error}"))))?
}

#[tauri::command]
pub fn library_status(state: State<'_, AppState>) -> Result<LibraryStatus, LibraryError> {
    state.library.status()
}

#[tauri::command]
pub async fn library_scan_now(
    state: State<'_, AppState>,
) -> Result<Vec<LibraryIndex>, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.scan_now())
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library scan join: {error}"))))?
}

#[tauri::command]
pub async fn library_pending_groups(
    state: State<'_, AppState>,
) -> Result<Vec<PendingMediaGroup>, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.pending_groups())
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library pending join: {error}"))))?
}

#[tauri::command]
pub async fn library_set_manual_title(
    state: State<'_, AppState>,
    root: String,
    group_key: String,
    title: String,
) -> Result<PendingMediaGroup, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.set_manual_title(root, group_key, title))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library title join: {error}"))))?
}

#[tauri::command]
pub async fn library_resolve_preview(
    state: State<'_, AppState>,
    root: String,
    group_key: String,
    config: ResolverRunConfig,
) -> Result<ResolverPreview, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.resolve_preview(root, group_key, config))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library resolver join: {error}"))))?
}

#[tauri::command]
pub async fn library_apply_tmdb_match(
    state: State<'_, AppState>,
    root: String,
    group_key: String,
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: TmdbConfig,
) -> Result<MetadataWriteResult, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.apply_tmdb_match(root, group_key, tmdb_id, media_type, tmdb)
    })
    .await
    .map_err(|error| LibraryError::internal(Some(&format!("library metadata join: {error}"))))?
}

#[tauri::command]
pub async fn library_context_for_media(
    state: State<'_, AppState>,
    media_path: String,
) -> Result<Option<MediaMetadataContext>, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.context_for_media(media_path))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library context join: {error}"))))?
}

#[tauri::command]
pub async fn library_credential_status() -> Result<CredentialStatus, LibraryError> {
    tauri::async_runtime::spawn_blocking(credentials::status)
        .await
        .map_err(|error| {
            LibraryError::internal(Some(&format!("credential status join: {error}")))
        })?
}

#[tauri::command]
pub async fn library_credentials_save(
    input: CredentialSaveInput,
) -> Result<CredentialStatus, LibraryError> {
    tauri::async_runtime::spawn_blocking(move || credentials::save(input))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("credential save join: {error}"))))?
}

#[tauri::command]
pub async fn library_credential_delete(
    kind: CredentialKind,
) -> Result<CredentialStatus, LibraryError> {
    tauri::async_runtime::spawn_blocking(move || credentials::delete(kind))
        .await
        .map_err(|error| {
            LibraryError::internal(Some(&format!("credential delete join: {error}")))
        })?
}

#[tauri::command]
pub async fn library_credentials_validate(
    config: CredentialValidationConfig,
) -> Result<CredentialValidationResult, LibraryError> {
    tauri::async_runtime::spawn_blocking(move || crate::library::validate_credentials(config))
        .await
        .map_err(|error| {
            LibraryError::internal(Some(&format!("credential validation join: {error}")))
        })
}
