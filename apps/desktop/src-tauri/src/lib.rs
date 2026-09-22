pub mod acp;
pub mod asr;
mod commands;
pub mod library;
pub mod mcp;
pub mod media;
pub mod notes;
pub mod player;
mod state;
pub mod subtitle;
pub mod ytdl;

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
    CredentialValidationResult, EpisodeFile, GroupResolution, LibraryError, LibraryErrorCode,
    LibraryIndex, LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind,
    MediaLibraryService, MediaMetadataContext, MergedMediaContext, MetadataMediaType,
    MetadataWriteResult, ModelDiscoveryConfig, ModelDiscoveryResult, PendingMediaGroup,
    ResolverIntent, ResolverPreview, ResolverProviderConfig, ResolverRunConfig, ResolverSelection,
    SeriesReading, StoredMetadata, StoredMetadataKind, TmdbCandidate, TmdbConfig, TmdbGroupStatus,
    WikiEnrichmentCandidate, WikiEnrichmentPreview, WikiGroupStatus, WikiMatchMethod, WikiMetadata,
    WikiWriteResult, WikiZhReference, WIKI_STALE_AFTER_MS,
};
pub use media::{
    MediaChapter, MediaError, MediaErrorCode, MediaInfo, MediaInspector, MediaStream,
    MediaToolStatus, StreamKind,
};
pub use notes::{Note, NoteError, NoteErrorCode, NoteFrame, NoteFrameData, NoteService};
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

use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};

use commands::acp::{
    acp_cancel, acp_close, acp_connect, acp_delete_session, acp_list_agent_sessions,
    acp_load_session, acp_new_chat, acp_prompt, acp_respond_permission, acp_set_session_config,
    acp_set_session_model, acp_status, acp_switch_session, acp_sync_mcp_capabilities,
    acp_task_contracts, acp_watch_feed,
};
use commands::asr::{asr_install, asr_status, asr_transcribe};
use commands::chapter::{
    chapter_asset_get, chapter_segmentation_start, chapter_segmentation_status,
};
use commands::chat_store::{
    chat_hint_delete, chat_hint_get, chat_hint_upsert, chat_snapshot_delete, chat_snapshot_get,
    chat_snapshot_upsert,
};
use commands::library::{
    library_agent_models_discover, library_apply_tmdb_match, library_context_for_media,
    library_credential_delete, library_credential_status, library_credentials_save,
    library_credentials_validate, library_list_groups, library_models_discover,
    library_parse_filename, library_pending_groups, library_resolve_episode_file,
    library_resolve_preview, library_scan_now, library_search_tmdb, library_series_for_media,
    library_set_manual_title, library_status, library_tmdb_credentials_validate,
    library_tmdb_refresh, library_tmdb_statuses, library_watch_start, library_watch_stop,
    library_wikipedia_apply, library_wikipedia_preview, library_wikipedia_refresh,
    library_wikipedia_statuses,
};
use commands::media::{media_inspect, media_list_siblings, media_tool_status};
use commands::notes::{
    notes_create, notes_delete, notes_dismiss_proposal, notes_export_markdown,
    notes_export_markdown_to_file, notes_export_recap_card, notes_get_frame, notes_list,
    notes_load_latest_proposal, notes_preview_quotes, notes_update,
};
use commands::player::{
    player_get_state, player_list_playback_formats, player_open, player_pause, player_play,
    player_seek, player_set_audio, player_set_playback_format, player_set_rate,
    player_set_subtitle, player_set_surface_bounds, player_set_surface_mode, player_set_volume,
    player_stop, player_subscribe,
};
use commands::subtitle::{
    subtitle_download_candidate, subtitle_export_sidecar, subtitle_list_choices,
    subtitle_load_choice, subtitle_proofread_track, subtitle_provider_status,
    subtitle_search_online, subtitle_set_provider_key, subtitle_translate_track,
    subtitle_validate_provider_key, subtitle_workshop_status,
};
use commands::system::{
    mark_clean_shutdown, set_crash_phase, system_log_dir, system_startup_notice,
};
use commands::ytdl::{
    ytdl_cached_resolve, ytdl_cookie_status, ytdl_install, ytdl_list_browser_profiles,
    ytdl_resolve, ytdl_set_cookies, ytdl_status, ytdl_test_cookies,
};
use player::mpv::window::{parent_handle_from_webview, register_surface_app, VideoSurface};
use state::AppState;
use tauri::Manager;

/// Process start for startup telemetry (white-screen diagnosis).
static STARTUP_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn startup_ms() -> u128 {
    STARTUP_START
        .get()
        .map(|start| start.elapsed().as_millis())
        .unwrap_or(0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    STARTUP_START.get_or_init(std::time::Instant::now);
    init_tracing();
    tracing::info!(elapsed_ms = startup_ms(), "startup: tracing initialized");

    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    #[cfg(not(test))]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    let app = match builder
        .on_page_load(|_webview, payload| {
            tracing::info!(
                elapsed_ms = startup_ms(),
                url = %payload.url(),
                event = ?payload.event(),
                "startup: page load"
            );
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            player_subscribe,
            player_get_state,
            player_open,
            player_list_playback_formats,
            player_set_playback_format,
            player_play,
            player_pause,
            player_stop,
            player_seek,
            player_set_volume,
            player_set_rate,
            player_set_subtitle,
            player_set_audio,
            player_set_surface_bounds,
            player_set_surface_mode,
            media_inspect,
            media_list_siblings,
            media_tool_status,
            system_log_dir,
            system_startup_notice,
            subtitle_list_choices,
            subtitle_load_choice,
            subtitle_export_sidecar,
            subtitle_translate_track,
            subtitle_proofread_track,
            subtitle_workshop_status,
            subtitle_provider_status,
            subtitle_set_provider_key,
            subtitle_search_online,
            subtitle_validate_provider_key,
            subtitle_download_candidate,
            asr_status,
            asr_transcribe,
            asr_install,
            chapter_segmentation_start,
            chapter_segmentation_status,
            chapter_asset_get,
            ytdl_status,
            ytdl_cookie_status,
            ytdl_set_cookies,
            ytdl_list_browser_profiles,
            ytdl_test_cookies,
            ytdl_install,
            ytdl_resolve,
            ytdl_cached_resolve,
            library_watch_start,
            library_watch_stop,
            library_status,
            library_scan_now,
            library_parse_filename,
            library_pending_groups,
            library_set_manual_title,
            library_resolve_episode_file,
            library_resolve_preview,
            library_search_tmdb,
            library_apply_tmdb_match,
            library_context_for_media,
            library_list_groups,
            library_series_for_media,
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
            acp_task_contracts,
            acp_watch_feed,
            acp_respond_permission,
            acp_connect,
            acp_list_agent_sessions,
            acp_load_session,
            acp_delete_session,
            acp_new_chat,
            acp_switch_session,
            acp_sync_mcp_capabilities,
            acp_set_session_model,
            acp_set_session_config,
            acp_prompt,
            acp_cancel,
            acp_close,
            notes_list,
            notes_preview_quotes,
            notes_get_frame,
            notes_create,
            notes_update,
            notes_delete,
            notes_load_latest_proposal,
            notes_dismiss_proposal,
            notes_export_markdown,
            notes_export_markdown_to_file,
            notes_export_recap_card,
            chat_snapshot_get,
            chat_snapshot_upsert,
            chat_snapshot_delete,
            chat_hint_get,
            chat_hint_upsert,
            chat_hint_delete,
        ])
        .setup(|app| {
            commands::chapter::recover_interrupted_tasks();
            if let Ok(resource_dir) = app.path().resource_dir() {
                std::env::set_var(
                    lumina_media::tools::RESOURCE_DIR_ENV,
                    resource_dir.to_string_lossy().as_ref(),
                );
            } else {
                tracing::warn!("failed to resolve Tauri resource directory");
            }
            crate::player::mpv::native_library::ensure_libmpv_loaded(app.handle())?;
            attach_native_surface(app)?;
            crate::acp::mcp_control::start_control_watcher(app.handle().clone());
            tracing::info!(elapsed_ms = startup_ms(), "startup: setup done");
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

    tracing::info!(elapsed_ms = startup_ms(), "startup: app built");
    app.run(|app, event| match event {
        tauri::RunEvent::Ready => {
            tracing::info!(elapsed_ms = startup_ms(), "startup: ready");
        }
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

/// File-appender guard: dropping it would silently stop file logging.
static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let dir = commands::system::log_dir();
    let _ = std::fs::create_dir_all(&dir);

    // `rolling::daily` panics when its directory cannot be opened. Startup
    // diagnostics must never be able to take down the player, so use the
    // fallible builder and degrade to a temp directory, then stdout only.
    let build_appender = |directory: &std::path::Path| {
        tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("lumina.log")
            .build(directory)
    };
    let file_target = match build_appender(&dir) {
        Ok(appender) => Some((appender, dir.clone())),
        Err(primary_error) => {
            let fallback_dir = std::env::temp_dir().join("lumina").join("logs");
            let _ = std::fs::create_dir_all(&fallback_dir);
            match build_appender(&fallback_dir) {
                Ok(appender) => {
                    eprintln!(
                        "Lumina file logging fell back to {} after {} was unavailable: {}",
                        fallback_dir.display(),
                        dir.display(),
                        primary_error
                    );
                    Some((appender, fallback_dir))
                }
                Err(fallback_error) => {
                    eprintln!(
                        "Lumina file logging unavailable for {} and {}: {}; {}",
                        dir.display(),
                        fallback_dir.display(),
                        primary_error,
                        fallback_error
                    );
                    None
                }
            }
        }
    };

    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    if let Some((file_appender, active_dir)) = file_target {
        // Daily-rotated `lumina.log.<date>` next to a live stdout layer; ANSI off in files.
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        let _ = LOG_GUARD.set(guard);
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(file_writer)
            .with_ansi(false);
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(stdout_layer)
            .with(file_layer)
            .try_init();
        tracing::info!(log_dir = %active_dir.display(), "file logging initialized");
    } else {
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(stdout_layer)
            .try_init();
        tracing::warn!(
            log_dir = %dir.display(),
            "file logging unavailable; continuing with stdout logging"
        );
    }
    commands::system::initialize_crash_diagnostics();
    std::panic::set_hook(Box::new(|info| {
        tracing::error!(panic = %info, "application panicked");
    }));
}

fn attach_native_surface(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    set_crash_phase("player_attach");
    register_surface_app(app.handle().clone());

    let window = app
        .get_webview_window("main")
        .ok_or("main webview window is missing")?;
    let parent = parent_handle_from_webview(&window)?;
    let surface = VideoSurface::create(parent)?;
    let wid = surface.wid_i64();

    let state = app.state::<AppState>();
    state.set_surface(surface)?;
    let script_path: Option<String> =
        crate::player::mpv::native_library::lumina_osc_script(app.handle())
            .map(|path| path.to_string_lossy().into_owned());
    if script_path.is_none() {
        tracing::warn!("custom OSC skin missing; continuing without skin");
    }
    if let Err(error) =
        state.with_player(|player| player.attach_backend_with_wid(wid, script_path.as_deref()))
    {
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
        tracing::info!("backend shutdown started");
        set_crash_phase("shutdown");
        state.mark_shutdown();
        state.acp.request_cancel();
        state.acp.close_session_for_shutdown();
        crate::acp::adapter::reset_prompt_snapshot_state(&state.prompt_snapshots);
        state.library.stop_for_shutdown();
        let _ = state.with_player(|player| {
            player.shutdown();
            Ok(())
        });
        tracing::info!("backend shutdown finished");
        mark_clean_shutdown();
        // The native video surface is a child of the Tauri window and is
        // destroyed with its parent on the UI thread. Do not explicitly drop
        // it from this background shutdown worker: Win32 requires a window to
        // be destroyed by the thread that created it.
    }
}
