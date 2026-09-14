//! Subtitle / transcript Tauri commands.

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

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
    Progress { message: String },
    Finished { transcript: Transcript },
    Failed { code: String, message: String },
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
fn translate_cached_track(
    state: &AppState,
    path: &str,
    choice_id: &str,
    target_lang: &str,
    profile_id: &str,
    profiles: AgentProfilesHint,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    on_event: &Channel<SubtitleTranslateEvent>,
) -> Result<Transcript, SubtitleError> {
    let (provider, source_lang) = crate::ytdl::provider::parse_cache_choice(choice_id)
        .ok_or_else(|| SubtitleError::extract_failed(Some("invalid cached subtitle choice")))?;
    let token = write::normalize_lang_token(target_lang)?;
    let invoker = crate::acp::adapter::AcpAgentInvoker::new(profiles);
    let progress = |message: String| {
        let _ = on_event.send(SubtitleTranslateEvent::Progress { message });
    };
    let source = state.provider().load_cached(path, choice_id)?;
    let mut progress = progress;
    let cues = translate::translate_cues(
        &source,
        &token,
        profile_id,
        model_id,
        reasoning_effort,
        &invoker,
        &mut progress,
    )?;
    let _ = on_event.send(SubtitleTranslateEvent::Progress {
        message: format!("正在保存缓存字幕（{token}）…"),
    });
    state
        .provider()
        .store_translation(path, &provider, &source_lang, &token, &cues)
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
            let _ = on_event.send(SubtitleTranslateEvent::Failed {
                code: format!("{:?}", error.code),
                message: error.message.clone(),
            });
        }
    }
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
    on_event: Channel<SubtitleTranslateEvent>,
) -> Result<Transcript, SubtitleError> {
    tauri::async_runtime::spawn_blocking(move || {
        if profile_id.trim().is_empty() || profiles.profiles.is_empty() {
            return Err(SubtitleError::translate_not_configured(Some(
                "workshop profile selection is empty",
            )));
        }
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
                &profile_id,
                profiles,
                model_id.as_deref(),
                reasoning_effort.as_deref(),
                &on_event,
            );
            finish_translate(&on_event, &result);
            return result;
        }
        let invoker = crate::acp::adapter::AcpAgentInvoker::new(profiles);
        let result = translate::translate_and_export_track(
            &path,
            &choice_id,
            &target_lang,
            &profile_id,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            &invoker,
            |message| {
                let _ = on_event.send(SubtitleTranslateEvent::Progress { message });
            },
        );
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
