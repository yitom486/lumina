pub mod acp;
pub mod asr;
mod commands;
pub mod library;
pub mod media;
pub mod notes;
pub mod player;
mod state;
pub mod subtitle;

pub use acp::{
    AcpClientSettings, AcpError, AcpErrorCode, AcpEvent, AcpService, AcpStatus, AgentKind,
    AgentProfileInput, AgentProfileStatus, AgentProfilesHint, PermissionMode, PermissionOption,
    SavedSessionHint, ThinkingLevel, VideoPromptContext,
};
pub use asr::{AsrError, AsrErrorCode, AsrEvent, AsrService, AsrStatus};
pub use library::{
    CredentialKind, CredentialSaveInput, CredentialStatus, CredentialValidationConfig,
    CredentialValidationItem, CredentialValidationResult, GroupResolution, LibraryError,
    LibraryErrorCode, LibraryIndex, LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind,
    MediaLibraryService, MediaMetadataContext, MetadataMediaType, MetadataWriteResult,
    PendingMediaGroup, ResolverIntent, ResolverPreview, ResolverRunConfig, ResolverSelection,
    StoredMetadata, StoredMetadataKind, TmdbCandidate, TmdbConfig,
};
pub use media::{
    MediaChapter, MediaError, MediaErrorCode, MediaInfo, MediaInspector, MediaStream, StreamKind,
};
pub use notes::{Note, NoteError, NoteErrorCode, NoteService};
pub use player::{
    PlayerError, PlayerErrorCode, PlayerEvent, PlayerService, PlayerSnapshot, PlayerState,
};
pub use subtitle::{
    Cue, SubtitleChoice, SubtitleError, SubtitleErrorCode, SubtitleService, SubtitleSource,
    Transcript,
};

use commands::acp::{
    acp_cancel, acp_close, acp_connect, acp_prompt, acp_respond_permission, acp_status,
};
use commands::asr::{asr_status, asr_transcribe};
use commands::library::{
    library_apply_tmdb_match, library_context_for_media, library_credential_delete,
    library_credential_status, library_credentials_save, library_credentials_validate,
    library_pending_groups, library_resolve_preview, library_scan_now, library_set_manual_title,
    library_status, library_watch_start, library_watch_stop,
};
use commands::media::{media_inspect, media_list_siblings};
use commands::notes::{
    notes_create, notes_delete, notes_export_markdown, notes_list, notes_update,
};
use commands::player::{
    player_get_state, player_open, player_pause, player_play, player_seek, player_set_audio,
    player_set_rate, player_set_subtitle, player_set_surface_bounds, player_set_volume,
    player_stop, player_subscribe,
};
use commands::subtitle::{subtitle_list_choices, subtitle_load_choice};
use player::mpv::window::{hwnd_from_webview_window, register_surface_app, VideoSurface};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let app = match tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            player_subscribe,
            player_get_state,
            player_open,
            player_play,
            player_pause,
            player_stop,
            player_seek,
            player_set_volume,
            player_set_rate,
            player_set_subtitle,
            player_set_audio,
            player_set_surface_bounds,
            media_inspect,
            media_list_siblings,
            subtitle_list_choices,
            subtitle_load_choice,
            asr_status,
            asr_transcribe,
            library_watch_start,
            library_watch_stop,
            library_status,
            library_scan_now,
            library_pending_groups,
            library_set_manual_title,
            library_resolve_preview,
            library_apply_tmdb_match,
            library_context_for_media,
            library_credential_status,
            library_credentials_save,
            library_credential_delete,
            library_credentials_validate,
            acp_status,
            acp_respond_permission,
            acp_connect,
            acp_prompt,
            acp_cancel,
            acp_close,
            notes_list,
            notes_create,
            notes_update,
            notes_delete,
            notes_export_markdown,
        ])
        .setup(|app| {
            attach_native_surface(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(error) => {
            tracing::error!(%error, "failed to start Tauri");
            std::process::exit(1);
        }
    };

    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            shutdown_backend(app);
        }
    });
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn attach_native_surface(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    register_surface_app(app.handle().clone());

    let window = app
        .get_webview_window("main")
        .ok_or("main webview window is missing")?;
    let parent = hwnd_from_webview_window(&window)?;
    let surface = VideoSurface::create(parent)?;
    let wid = surface.hwnd_i64();

    let state = app.state::<AppState>();
    state.set_surface(surface)?;
    if let Err(error) = state.with_player(|player| player.attach_backend_with_wid(wid)) {
        tracing::error!(
            code = ?error.code,
            message = %error.message,
            "libmpv init with wid failed"
        );
        return Err(error.into());
    }

    tracing::info!(wid, "native mpv surface attached");
    Ok(())
}

fn shutdown_backend(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.mark_shutdown();
        state.acp.request_cancel();
        let _ = state.library.stop();
        let _ = state.with_player(|player| {
            player.shutdown();
            Ok(())
        });
        drop(state.take_surface());
    }
}
