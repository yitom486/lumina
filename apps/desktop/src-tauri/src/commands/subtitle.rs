//! Subtitle / transcript Tauri commands.

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use lumina_core::BatchCheckpoint;

use crate::acp::AgentProfilesHint;
use crate::state::AppState;
use crate::subtitle::model::Cue;
use crate::subtitle::translate;
use crate::subtitle::write;
use crate::subtitle::{
    SubtitleChoice, SubtitleError, SubtitleErrorCode, SubtitleService, Transcript,
};

fn is_remote(path: &str) -> bool {
    matches!(
        crate::player::source::MediaSource::parse(path).map(|source| source.kind()),
        Ok(crate::player::source::MediaSourceKind::Remote)
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "PascalCase",
    rename_all_fields = "camelCase",
    tag = "type",
    content = "payload"
)]
pub enum SubtitleTranslateEvent {
    Progress {
        message: String,
        done: Option<usize>,
        total: Option<usize>,
    },
    Finished {
        transcript: Transcript,
    },
    Failed {
        code: String,
        message: String,
    },
}

/// Forward a batch progress update to the UI channel. Load/save steps send
/// `None` counts (indeterminate); batch completions carry real numbers.
fn forward_progress(update: translate::ProgressUpdate) -> SubtitleTranslateEvent {
    SubtitleTranslateEvent::Progress {
        message: update.message,
        done: update.done,
        total: update.total,
    }
}

fn indeterminate_progress(message: String) -> SubtitleTranslateEvent {
    SubtitleTranslateEvent::Progress {
        message,
        done: None,
        total: None,
    }
}

/// Checkpoint store for one workshop job: finished batches persist to the
/// process cache so a later run resumes instead of restarting. Cleared after
/// a fully successful job; failures keep it for resume.
fn workshop_checkpoint(
    path: &str,
    choice_id: &str,
    target: &str,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    cues: &[Cue],
) -> crate::ytdl::provider::TranslationCheckpoint {
    let model_key = format!(
        "{}|{}",
        model_id.unwrap_or("default"),
        reasoning_effort.unwrap_or("default")
    );
    crate::ytdl::provider::translation_checkpoint(
        path,
        choice_id,
        target,
        &model_key,
        translate::TRANSLATE_BATCH_SIZE,
        cues,
    )
}

fn clear_checkpoint(store: &crate::ytdl::provider::TranslationCheckpoint) {
    if let Err(error) = store.clear() {
        tracing::warn!(%error, "checkpoint clear failed");
    }
}

/// Unique id per workshop invocation for log correlation across batches.
/// Metadata only: appears in task labels and tracing fields, never in prompts.
fn workshop_job_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("ws-{}-{}", std::process::id(), nanos)
}

/// Build the per-job workshop pool (P2): fixed slots on reused ACP processes.
/// Slots rotate sessions every batch, the pool owns transport retry, and the
/// caller shuts the pool down explicitly after the job succeeds. `Drop` on
/// the pool is the backstop for error unwinds.
fn workshop_pool(
    profiles: &AgentProfilesHint,
    profile_id: &str,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    job_id: &str,
) -> std::sync::Arc<lumina_acp::WorkshopPool> {
    let model_selection = model_id.filter(|id| !id.trim().is_empty()).map(|model_id| {
        lumina_acp::AcpSessionModelSelection {
            model_id: model_id.to_string(),
            reasoning_effort: reasoning_effort
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
        }
    });
    std::sync::Arc::new(lumina_acp::WorkshopPool::new(
        lumina_acp::PoolConfig {
            size: lumina_acp::DEFAULT_POOL_SIZE,
            profile_id: profile_id.to_string(),
            profiles: profiles.clone(),
            model_selection,
        },
        job_id.to_string(),
    ))
}

#[tauri::command]
pub async fn subtitle_list_choices(
    app: AppHandle,
    path: String,
) -> Result<Vec<SubtitleChoice>, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        if is_remote(&path) {
            let state = app
                .try_state::<AppState>()
                .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
            state.ytdl().list_subtitle_choices(&path)
        } else {
            let resource_dir: Option<std::path::PathBuf> = app.path().resource_dir().ok();
            let mut choices = SubtitleService::list_choices_with(&path, resource_dir.as_ref())?;
            // Downloaded process-cache choices are listed, never auto-selected.
            if let Some(state) = app.try_state::<AppState>() {
                choices.extend(state.provider().list_cached(&path));
            }
            Ok(choices)
        }
    })
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
    app: AppHandle,
    path: String,
    choice_id: String,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        if is_remote(&path) {
            let state = app
                .try_state::<AppState>()
                .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
            state.ytdl().load_subtitle_choice(&path, &choice_id)
        } else if crate::ytdl::provider::parse_cache_choice(&choice_id).is_some() {
            let state = app
                .try_state::<AppState>()
                .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
            state.provider().load_cached(&path, &choice_id)
        } else {
            SubtitleService::load_choice(path, choice_id)
        }
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "加载字幕任务异常结束",
            Some(error.to_string()),
        )
    })?
}

#[tauri::command]
pub async fn subtitle_export_sidecar(
    path: String,
    lang_token: String,
    cues: Vec<Cue>,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        write::export_sidecar_srt(std::path::Path::new(&path), &lang_token, &cues)
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "保存字幕任务异常结束",
            Some(error.to_string()),
        )
    })?
}

/// Explicit user action only: translate a cached-download choice and keep the
/// result in the process cache (never beside the media file).
#[allow(clippy::too_many_arguments)]
/// Assemble translation context from the library (best-effort: translation
/// never fails for missing metadata). Synopsis grounds tone; glossary pairs
/// join TMDb cast (Chinese actor, English character) with the zh wiki cast
/// on exact actor matches — conservative by design, no fuzzy joins.
fn translation_context_for_media(
    state: &AppState,
    media_path: &str,
) -> Option<translate::TranslationContext> {
    let context = state
        .library
        .context_for_media(media_path.to_string())
        .ok()??;
    let merged = context.merged.as_ref();
    let synopsis = merged
        .and_then(|merged| merged.synopsis.clone())
        .or_else(|| context.group.overview.clone());
    let wiki_cast = context
        .wiki
        .as_ref()
        .map(|wiki| wiki.zh_cast.clone())
        .unwrap_or_default();
    let mut glossary = Vec::new();
    for member in &context.group.cast {
        let character = member.character.trim();
        if character.is_empty() || character == member.name.trim() {
            continue;
        }
        let Some(zh) = wiki_cast
            .iter()
            .find(|entry| entry.actor.trim() == member.name.trim())
        else {
            continue;
        };
        glossary.push(translate::TranslationGlossaryEntry {
            source: character.to_string(),
            target: zh.name.clone(),
            verified: true,
        });
        if glossary.len() >= 30 {
            break;
        }
    }
    if synopsis
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty())
        || !glossary.is_empty()
    {
        Some(translate::TranslationContext { synopsis, glossary })
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn translate_cached_track(
    state: &AppState,
    path: &str,
    choice_id: &str,
    target_lang: &str,
    backfill: bool,
    review_mode: bool,
    profile_id: &str,
    profiles: AgentProfilesHint,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    on_event: &Channel<SubtitleTranslateEvent>,
    job_id: &str,
) -> Result<Transcript, SubtitleError> {
    let (provider, source_lang) = crate::ytdl::provider::parse_cache_choice(choice_id)
        .ok_or_else(|| SubtitleError::extract_failed(Some("invalid cached subtitle choice")))?;
    let token = write::normalize_lang_token(target_lang)?;
    let pool = workshop_pool(&profiles, profile_id, model_id, reasoning_effort, job_id);
    let invoker =
        crate::acp::adapter::AcpAgentInvoker::with_pool(profiles, std::sync::Arc::clone(&pool));
    let progress = |update: translate::ProgressUpdate| {
        let _ = on_event.send(forward_progress(update));
    };
    let source = state.provider().load_cached(path, choice_id)?;
    let checkpoint = workshop_checkpoint(
        path,
        choice_id,
        &token,
        model_id,
        reasoning_effort,
        &source.cues,
    );
    let context = translation_context_for_media(state, path);
    let mut progress = progress;
    let translated = translate::translate_cues(
        &source,
        &token,
        context.as_ref(),
        profile_id,
        model_id,
        reasoning_effort,
        &invoker,
        &mut progress,
        Some(&checkpoint),
        job_id,
    )?;
    pool.shutdown();
    if let Some(message) = backfill_glossary_names(
        state,
        path,
        &translated
            .reported_names
            .iter()
            .map(|name| (name.source.clone(), name.target.clone()))
            .collect::<Vec<_>>(),
        &token,
        backfill,
        review_mode,
    ) {
        let _ = on_event.send(indeterminate_progress(message));
    }
    let _ = on_event.send(indeterminate_progress(format!(
        "正在保存缓存字幕（{token}）…"
    )));
    let stored = state.provider().store_translation(
        path,
        &provider,
        &source_lang,
        &token,
        &translated.cues,
    )?;
    clear_checkpoint(&checkpoint);
    Ok(stored)
}

#[tauri::command]
pub async fn subtitle_provider_status(
    app: AppHandle,
) -> Result<Vec<crate::ytdl::provider::ProviderStatus>, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
        Ok(state.provider().status())
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "读取字幕来源状态异常结束",
            Some(error.to_string()),
        )
    })?
}

#[tauri::command]
pub async fn subtitle_set_provider_key(
    app: AppHandle,
    provider: String,
    key: String,
) -> Result<Vec<crate::ytdl::provider::ProviderStatus>, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
        state.provider().set_key(&provider, &key)
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "保存字幕来源密钥异常结束",
            Some(error.to_string()),
        )
    })?
}

/// Check a typed provider key without persisting it, so a bad key never
/// overwrites a working one. Failures arrive as a validation verdict, not
/// an error, mirroring the metadata credential checks.
#[tauri::command]
pub async fn subtitle_validate_provider_key(
    app: AppHandle,
    provider: String,
    key: String,
) -> Result<crate::ytdl::provider::ProviderKeyValidation, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
        state.provider().validate_key(&provider, &key)
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "验证字幕来源密钥异常结束",
            Some(error.to_string()),
        )
    })?
}

#[tauri::command]
pub async fn subtitle_search_online(
    app: AppHandle,
    path: String,
    query: crate::ytdl::provider::SubtitleQuery,
    prefer: Vec<String>,
) -> Result<Vec<crate::ytdl::provider::SubtitleCandidate>, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
        // Fill empty query fields from the library index (manual title beats
        // the parsed key, parsed season/episode beat nothing). Read-only.
        let query = match state.library.group_label_for_media(&path) {
            Some((label, season, episode)) => crate::ytdl::provider::SubtitleQuery {
                title: query
                    .title
                    .filter(|title| !title.trim().is_empty())
                    .or_else(|| {
                        let label = label.trim();
                        if label.is_empty() {
                            None
                        } else {
                            Some(label.to_string())
                        }
                    }),
                season: query.season.or(season),
                episode: query.episode.or(episode),
                ..query
            },
            None => query,
        };
        state.provider().search(&path, &query, &prefer)
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "搜索在线字幕异常结束",
            Some(error.to_string()),
        )
    })?
}

/// Explicit user action only: download one candidate into the process cache.
/// Never auto-selects or auto-displays; the caller refreshes the choice list.
#[tauri::command]
pub async fn subtitle_download_candidate(
    app: AppHandle,
    path: String,
    candidate: crate::ytdl::provider::SubtitleCandidate,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| SubtitleError::internal(Some("app state unavailable")))?;
        state.provider().download(&path, &candidate)
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "下载字幕异常结束",
            Some(error.to_string()),
        )
    })?
}

fn finish_translate(
    on_event: &Channel<SubtitleTranslateEvent>,
    result: &Result<Transcript, SubtitleError>,
) {
    match result {
        Ok(transcript) => {
            let _ = on_event.send(SubtitleTranslateEvent::Finished {
                transcript: transcript.clone(),
            });
        }
        Err(error) => {
            // Error spec: UI shows the fixed message only; the full cause
            // (code + details: cue index, batch, os error) belongs in the log.
            // This was missing, so workshop failures died without a trace.
            tracing::warn!(
                code = ?error.code,
                message = %error.message,
                details = ?error.details,
                "subtitle workshop task failed"
            );
            let _ = on_event.send(SubtitleTranslateEvent::Failed {
                code: format!("{:?}", error.code),
                message: error.message.clone(),
            });
        }
    }
}

/// Record model-reported names into the group glossary. Best-effort: never
/// fails the translation, only reports progress. Returns a progress message
/// when anything was admitted, for the caller to surface.
fn backfill_glossary_names(
    state: &AppState,
    media_path: &str,
    reported: &[(String, String)],
    target_lang: &str,
    backfill: bool,
    review_mode: bool,
) -> Option<String> {
    if !backfill || reported.is_empty() {
        return None;
    }
    let origin = format!("translate:{target_lang}");
    match state
        .library
        .record_subtitle_glossary(media_path, reported, origin, review_mode)
    {
        Ok(0) => None,
        Ok(count) => Some(format!("译名表已更新（新增 {count} 个）")),
        Err(error) => {
            tracing::warn!(
                code = ?error.code,
                details = ?error.details,
                "subtitle glossary backfill skipped"
            );
            None
        }
    }
}

/// Proofread source-language cues without translating: typos, OCR artifacts
/// and misheard words, timeline preserved. Shares the workshop pipeline
/// (batching, glossary, progress events) with translation.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn subtitle_proofread_track(
    app: AppHandle,
    path: String,
    choice_id: String,
    strip_sound_tags: Option<bool>,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_id: Option<String>,
    reasoning_effort: Option<String>,
    on_event: Channel<SubtitleTranslateEvent>,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        if profile_id.trim().is_empty() || profiles.profiles.is_empty() {
            return Err(SubtitleError::translate_not_configured(Some(
                "workshop profile selection is empty",
            )));
        }
        let strip = strip_sound_tags.unwrap_or(true);
        let job_id = workshop_job_id();
        tracing::info!(job_id = %job_id, path = %path, "subtitle workshop job started");
        // Cached downloads proofread back into the process cache so the
        // user's media directory stays clean; local tracks keep sidecars.
        if crate::ytdl::provider::parse_cache_choice(&choice_id).is_some() {
            let Some(state) = app.try_state::<AppState>() else {
                return Err(SubtitleError::internal(Some("app state unavailable")));
            };
            let result = proofread_cached_track(
                state.inner(),
                &path,
                &choice_id,
                strip,
                &profile_id,
                profiles,
                model_id.as_deref(),
                reasoning_effort.as_deref(),
                &on_event,
                &job_id,
            );
            finish_translate(&on_event, &result);
            return result;
        }
        let pool = workshop_pool(
            &profiles,
            &profile_id,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            &job_id,
        );
        let invoker =
            crate::acp::adapter::AcpAgentInvoker::with_pool(profiles, std::sync::Arc::clone(&pool));
        let context = app
            .try_state::<AppState>()
            .and_then(|state| translation_context_for_media(&state, &path));
        let source = SubtitleService::load_choice(&path, &choice_id)?;
        let proof_token = proofread_token(&source)?;
        let checkpoint = workshop_checkpoint(
            &path,
            &choice_id,
            &proof_token,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            &source.cues,
        );
        let mut progress = |update: translate::ProgressUpdate| {
            let _ = on_event.send(forward_progress(update));
        };
        let cues = translate::proofread_cues(
            &source,
            context.as_ref(),
            strip,
            &profile_id,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            &invoker,
            &mut progress,
            Some(&checkpoint),
            &job_id,
        )?;
        pool.shutdown();
        let _ = on_event.send(indeterminate_progress(format!(
            "正在保存校对字幕（{proof_token}）…"
        )));
        let result = write::export_sidecar_srt(std::path::Path::new(&path), &proof_token, &cues);
        if result.is_ok() {
            clear_checkpoint(&checkpoint);
        }
        finish_translate(&on_event, &result);
        result
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "校对字幕任务异常结束",
            Some(error.to_string()),
        )
    })?
}

/// Proofread output token: source language suffixed, so the proofread track
/// never collides with the original (`en` → `en-proofread`).
fn proofread_token(source: &Transcript) -> Result<String, SubtitleError> {
    let lang = source
        .language
        .as_deref()
        .filter(|lang| !lang.trim().is_empty())
        .unwrap_or("und");
    write::normalize_lang_token(&format!("{lang}-proofread"))
}

#[allow(clippy::too_many_arguments)]
fn proofread_cached_track(
    state: &AppState,
    path: &str,
    choice_id: &str,
    strip_sound_tags: bool,
    profile_id: &str,
    profiles: AgentProfilesHint,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    on_event: &Channel<SubtitleTranslateEvent>,
    job_id: &str,
) -> Result<Transcript, SubtitleError> {
    let (provider, source_lang) = crate::ytdl::provider::parse_cache_choice(choice_id)
        .ok_or_else(|| SubtitleError::extract_failed(Some("invalid cached subtitle choice")))?;
    let source = state.provider().load_cached(path, choice_id)?;
    let token = proofread_token(&source)?;
    let checkpoint = workshop_checkpoint(
        path,
        choice_id,
        &token,
        model_id,
        reasoning_effort,
        &source.cues,
    );
    let context = translation_context_for_media(state, path);
    let pool = workshop_pool(&profiles, profile_id, model_id, reasoning_effort, job_id);
    let invoker =
        crate::acp::adapter::AcpAgentInvoker::with_pool(profiles, std::sync::Arc::clone(&pool));
    let mut progress = |update: translate::ProgressUpdate| {
        let _ = on_event.send(forward_progress(update));
    };
    let cues = translate::proofread_cues(
        &source,
        context.as_ref(),
        strip_sound_tags,
        profile_id,
        model_id,
        reasoning_effort,
        &invoker,
        &mut progress,
        Some(&checkpoint),
        job_id,
    )?;
    pool.shutdown();
    let _ = on_event.send(indeterminate_progress(format!(
        "正在保存缓存字幕（{token}）…"
    )));
    let stored =
        state
            .provider()
            .store_translation(path, &provider, &source_lang, &token, &cues)?;
    clear_checkpoint(&checkpoint);
    Ok(stored)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn subtitle_translate_track(
    app: AppHandle,
    path: String,
    choice_id: String,
    target_lang: String,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_id: Option<String>,
    reasoning_effort: Option<String>,
    glossary_backfill: Option<bool>,
    glossary_review_mode: Option<bool>,
    on_event: Channel<SubtitleTranslateEvent>,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        if profile_id.trim().is_empty() || profiles.profiles.is_empty() {
            return Err(SubtitleError::translate_not_configured(Some(
                "workshop profile selection is empty",
            )));
        }
        let backfill = glossary_backfill.unwrap_or(true);
        let review_mode = glossary_review_mode.unwrap_or(false);
        let job_id = workshop_job_id();
        tracing::info!(job_id = %job_id, path = %path, "subtitle workshop job started");
        // Cached downloads translate back into the process cache so the
        // user's media directory stays clean; local tracks keep sidecars.
        if crate::ytdl::provider::parse_cache_choice(&choice_id).is_some() {
            let Some(state) = app.try_state::<AppState>() else {
                return Err(SubtitleError::internal(Some("app state unavailable")));
            };
            let result = translate_cached_track(
                state.inner(),
                &path,
                &choice_id,
                &target_lang,
                backfill,
                review_mode,
                &profile_id,
                profiles,
                model_id.as_deref(),
                reasoning_effort.as_deref(),
                &on_event,
                &job_id,
            );
            finish_translate(&on_event, &result);
            return result;
        }
        let pool = workshop_pool(
            &profiles,
            &profile_id,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            &job_id,
        );
        let invoker =
            crate::acp::adapter::AcpAgentInvoker::with_pool(profiles, std::sync::Arc::clone(&pool));
        let context = app
            .try_state::<AppState>()
            .and_then(|state| translation_context_for_media(&state, &path));
        // Checkpoint factory: the wrapper loads the source, then asks here
        // for a store keyed by the loaded cues (fingerprint inside).
        let checkpoint_factory =
            |source: &Transcript, token: &str, model_key: &str| -> Box<dyn BatchCheckpoint> {
                Box::new(crate::ytdl::provider::translation_checkpoint(
                    &path,
                    &choice_id,
                    token,
                    model_key,
                    translate::TRANSLATE_BATCH_SIZE,
                    &source.cues,
                ))
            };
        let translated = translate::translate_and_export_track(
            &path,
            &choice_id,
            &target_lang,
            context.as_ref(),
            &profile_id,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            &invoker,
            |update| {
                let _ = on_event.send(forward_progress(update));
            },
            Some(&checkpoint_factory),
            &job_id,
        )?;
        pool.shutdown();
        if let Some(state) = app.try_state::<AppState>() {
            if let Some(message) = backfill_glossary_names(
                &state,
                &path,
                &translated
                    .reported_names
                    .iter()
                    .map(|name| (name.source.clone(), name.target.clone()))
                    .collect::<Vec<_>>(),
                &target_lang,
                backfill,
                review_mode,
            ) {
                let _ = on_event.send(indeterminate_progress(message));
            }
        }
        let result = Ok(translated.transcript);
        finish_translate(&on_event, &result);
        result
    })
    .await
    .map_err(|error| {
        SubtitleError::new(
            SubtitleErrorCode::InternalError,
            "翻译字幕任务异常结束",
            Some(error.to_string()),
        )
    })?
}
