pub mod acp;
pub mod asr;
mod commands;
pub mod library;
pub mod mcp;
pub mod media;
pub mod notes;
pub mod player;
mod process_util;
mod state;
pub mod subtitle;

pub use acp::{
    AcpClientSettings, AcpError, AcpErrorCode, AcpEvent, AcpService, AcpStatus, AgentKind,
    AgentProfileInput, AgentProfileStatus, AgentProfilesHint, PermissionMode, PermissionOption,
    SavedSessionHint, ThinkingLevel, VideoPromptContext,
};
pub use asr::{
    AsrCatalogModel, AsrError, AsrErrorCode, AsrEvent, AsrInstallEvent, AsrModelInfo, AsrRange,
    AsrService, AsrStatus,
};
pub use library::{
    AgentModelDiscoveryConfig, AgentModelDiscoveryResult, CredentialKind, CredentialSaveInput,
    CredentialStatus, CredentialValidationConfig, CredentialValidationItem,
    CredentialValidationResult, GroupResolution, LibraryError, LibraryErrorCode, LibraryIndex,
    LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind, MediaLibraryService,
    MediaMetadataContext, MergedMediaContext, MetadataMediaType, MetadataWriteResult,
    ModelDiscoveryConfig, ModelDiscoveryResult, PendingMediaGroup, ResolverIntent, ResolverPreview,
    ResolverProviderConfig, ResolverRunConfig, ResolverSelection, StoredMetadata,
    StoredMetadataKind, TmdbCandidate, TmdbConfig, TmdbGroupStatus, WikiEnrichmentCandidate,
    WikiEnrichmentPreview, WikiGroupStatus, WikiMatchMethod, WikiMetadata, WikiWriteResult,
    WikiZhReference, WIKI_STALE_AFTER_MS,
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use commands::acp::{
    acp_cancel, acp_close, acp_connect, acp_new_chat, acp_prompt, acp_respond_permission,
    acp_set_session_model, acp_status, acp_sync_mcp_capabilities,
};
use commands::asr::{asr_install, asr_status, asr_transcribe};
use commands::library::{
    library_agent_models_discover, library_apply_tmdb_match, library_context_for_media,
    library_credential_delete, library_credential_status, library_credentials_save,
    library_credentials_validate, library_list_groups, library_models_discover,
    library_pending_groups, library_resolve_preview, library_scan_now, library_set_manual_title,
    library_status, library_tmdb_credentials_validate, library_tmdb_refresh, library_tmdb_statuses,
    library_watch_start, library_watch_stop, library_wikipedia_apply, library_wikipedia_preview,
    library_wikipedia_refresh, library_wikipedia_statuses,
};
use commands::media::{media_inspect, media_list_siblings};
use commands::notes::{
    notes_create, notes_delete, notes_dismiss_proposal, notes_export_markdown,
    notes_export_markdown_to_file, notes_list, notes_load_latest_proposal, notes_preview_quotes,
    notes_update,
};
use commands::player::{
    player_get_state, player_open, player_pause, player_play, player_seek, player_set_audio,
    player_set_rate, player_set_subtitle, player_set_surface_bounds, player_set_volume,
    player_stop, player_subscribe,
};
use commands::subtitle::{
    subtitle_export_sidecar, subtitle_list_choices, subtitle_load_choice,
    subtitle_translate_track,
};
use player::mpv::window::{parent_handle_from_webview, register_surface_app, VideoSurface};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if mcp::run_if_invoked() {
        return;
    }
    init_tracing();

    let app = match tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                tracing::info!(label = %window.label(), "native window close requested");
                // A JavaScript close listener makes Tauri defer the platform
                // close until that listener resolves. Always request the app
                // exit here so a WebView-side listener cannot trap the native
                // close button or Alt+F4.
                window.app_handle().exit(0);
            }
        })
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
            subtitle_export_sidecar,
            subtitle_translate_track,
            asr_status,
            asr_transcribe,
            asr_install,
            library_watch_start,
            library_watch_stop,
            library_status,
            library_scan_now,
            library_pending_groups,
            library_set_manual_title,
            library_resolve_preview,
            library_apply_tmdb_match,
            library_context_for_media,
            library_list_groups,
            library_wikipedia_preview,
            library_wikipedia_apply,
            library_wikipedia_refresh,
            library_wikipedia_statuses,
            library_tmdb_refresh,
            library_tmdb_statuses,
            library_credential_status,
            library_credentials_save,
            library_credential_delete,
            library_credentials_validate,
            library_tmdb_credentials_validate,
            library_models_discover,
            library_agent_models_discover,
            acp_status,
            acp_respond_permission,
            acp_connect,
            acp_new_chat,
            acp_sync_mcp_capabilities,
            acp_set_session_model,
            acp_prompt,
            acp_cancel,
            acp_close,
            notes_list,
            notes_preview_quotes,
            notes_create,
            notes_update,
            notes_delete,
            notes_load_latest_proposal,
            notes_dismiss_proposal,
            notes_export_markdown,
            notes_export_markdown_to_file,
        ])
        .setup(|app| {
            crate::player::mpv::native_library::ensure_libmpv_loaded(app.handle())?;
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

    app.run(|app, event| match event {
        tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
            handle_app_exit(app);
        }
        _ => {}
    });
}

#[derive(Clone, Copy)]
struct ForceExitConfig {
    grace_ms: u64,
}

const FORCE_EXIT: ForceExitConfig = ForceExitConfig { grace_ms: 1_500 };

static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

fn handle_app_exit(app: &tauri::AppHandle) {
    if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let backend = app.clone();
    let _ = thread::Builder::new()
        .name("lumina-shutdown".into())
        .spawn(move || shutdown_backend(&backend));
    schedule_force_exit();
}

fn schedule_force_exit() {
    let _ = thread::Builder::new()
        .name("lumina-force-exit".into())
        .spawn(move || {
            thread::sleep(Duration::from_millis(FORCE_EXIT.grace_ms));
            tracing::warn!("forcing process exit after shutdown grace period");
            std::process::exit(0);
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
    let parent = parent_handle_from_webview(&window)?;
    let surface = VideoSurface::create(parent)?;
    let wid = surface.wid_i64();

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
        state.acp.close_session_for_shutdown();
        state.library.stop_for_shutdown();
        let _ = state.with_player(|player| {
            player.shutdown();
            Ok(())
        });
        // The native video surface is a child of the Tauri window and is
        // destroyed with its parent on the UI thread. Do not explicitly drop
        // it from this background shutdown worker: Win32 requires a window to
        // be destroyed by the thread that created it.
    }
}
