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
            SubtitleService::list_choices_with(path, resource_dir.as_ref())
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

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn subtitle_translate_track(
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
        let result = translate::translate_and_export_track(
            &path,
            &choice_id,
            &target_lang,
            &profile_id,
            &profiles,
            model_id.as_deref(),
            reasoning_effort.as_deref(),
            |message| {
                let _ = on_event.send(SubtitleTranslateEvent::Progress { message });
            },
        );
        match &result {
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
