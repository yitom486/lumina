//! On-demand ACP commands. Never runs unless the frontend invokes them.

use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::str::FromStr;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::acp::adapter;
use crate::acp::{
    AcpError, AcpEvent, AcpStatus, AgentProfilesHint, AgentSessionListResult, PromptImage,
    SavedSessionHint, VideoPromptContext,
};
use crate::mcp::OnlineMediaSnapshot;
use crate::state::AppState;
use lumina_acp::agent::workspace::resolve_session_cwd;
use lumina_acp::AcpClientSettings;
use lumina_ai::prompts::{
    compose_prompt, ComposedPrompt, EpisodeContext, MediaContext, PromptRepository, PromptSlots,
    SpoilerBoundary, TaskId, ViewingContext,
};
use lumina_ai::validate_task_output;
use lumina_library::{
    ChapterAssetRecord, ChapterRecord, ChapterRevisionRecord, EpisodeRecord, MediaMetadataContext,
    QuestionCandidateRecord, Repository, StoredMetadataKind, WatchFeedItemRecord,
};

/// Read-only business projection for the AI watch-feed tab.
///
/// The DTO intentionally contains only domain data.  It never exposes a
/// SQLite connection, native resource handle, stderr, or a local asset path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpWatchFeedResponse {
    pub source: AcpWatchFeedSource,
    pub items: Vec<AcpWatchFeedItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AcpWatchFeedSource {
    Sqlite,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpWatchFeedItem {
    pub id: i64,
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub revision_id: Option<i64>,
    pub task_id: Option<i64>,
    pub item_type: String,
    pub source: String,
    pub content: String,
    pub spoiler_level: String,
    pub content_version: String,
    pub published_at_ms: Option<i64>,
    pub chapter: Option<AcpWatchFeedChapter>,
    pub revision: Option<AcpWatchFeedRevision>,
    pub question_candidate: Option<AcpWatchFeedQuestionCandidate>,
    /// Opaque resource references. Local filesystem paths never cross this boundary.
    pub screenshot_refs: Vec<String>,
    pub cover_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpWatchFeedChapter {
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub spoiler_level: String,
    pub title: Option<String>,
    pub mainline: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpWatchFeedRevision {
    pub id: i64,
    pub revision_number: i64,
    pub revision_type: String,
    pub content: String,
    pub source: String,
    pub prompt_version: String,
    pub validation_report: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpWatchFeedQuestionCandidate {
    pub id: i64,
    pub question: String,
    pub source: String,
    pub spoiler_level: String,
    pub batch_key: Option<String>,
    pub is_user_defined: bool,
    pub selected_at_ms: Option<i64>,
}

#[tauri::command]
pub async fn acp_status(
    app: AppHandle,
    profiles: AgentProfilesHint,
) -> Result<AcpStatus, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        Ok(state.acp.status(&profiles))
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp status join: {error}"))))?
}

/// Read the durable watch-feed projection without touching the live ACP chat.
#[tauri::command]
pub async fn acp_watch_feed(app: AppHandle) -> Result<AcpWatchFeedResponse, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("watch feed app state unavailable")));
        };

        let media_path = state
            .with_player(|player| Ok(player.snapshot().current_file))
            .map_err(|error| {
                AcpError::internal(Some(&format!("watch feed player snapshot: {error}")))
            })?;
        let Some(media_path) = media_path.filter(|path| !path.trim().is_empty()) else {
            return Ok(empty_watch_feed_response());
        };

        let library_context = match state.library.context_for_media(media_path.clone()) {
            Ok(context) => context,
            Err(error) => {
                // Metadata is an identity enhancement, not a playback or chat
                // prerequisite.  Keep the legacy path lookup available when a
                // library index is absent, stale, or unreadable.
                tracing::warn!(code = ?error.code, "watch feed library identity unavailable");
                None
            }
        };
        let identity_plan = watch_feed_identity_plan(&media_path, library_context.as_ref());

        let database = super::chapter::open_database().map_err(|error| {
            tracing::warn!(code = %error.code, details = ?error.details, "watch feed database unavailable");
            AcpError::internal(Some("watch feed database unavailable"))
        })?;
        let repository = database.repository();
        let episodes = resolve_watch_feed_episodes(&repository, &identity_plan)?;
        let items = load_watch_feed_projection(&repository, &episodes)?;

        if items.is_empty() {
            return Ok(empty_watch_feed_response());
        }

        Ok(AcpWatchFeedResponse {
            source: AcpWatchFeedSource::Sqlite,
            items,
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("watch feed query join: {error}"))))?
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchFeedIdentity {
    series_stable_id: String,
    episode_stable_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchFeedIdentityPlan {
    authoritative: Option<WatchFeedIdentity>,
    legacy_series_stable_id: String,
    legacy_episode_stable_id: String,
}

#[derive(Debug, Clone)]
struct WatchFeedEpisodeSource {
    episode: EpisodeRecord,
}

struct WatchFeedEpisodeProjection {
    items: Vec<WatchFeedItemRecord>,
    chapters_by_id: HashMap<i64, ChapterRecord>,
    revisions_by_id: HashMap<i64, ChapterRevisionRecord>,
    latest_revisions_by_chapter: HashMap<i64, ChapterRevisionRecord>,
    question_candidates: Vec<QuestionCandidateRecord>,
    assets_by_chapter: HashMap<i64, Vec<ChapterAssetRecord>>,
}

fn watch_feed_identity_plan(
    media_path: &str,
    context: Option<&MediaMetadataContext>,
) -> WatchFeedIdentityPlan {
    let authoritative = context.and_then(|context| {
        let item = context.item.as_ref()?;
        let valid_episode = context.group.kind == StoredMetadataKind::Series
            && context.group.tmdb_id > 0
            && item.kind == StoredMetadataKind::Episode
            && item.tmdb_id > 0
            && item
                .series_tmdb_id
                .map(|series_tmdb_id| series_tmdb_id == context.group.tmdb_id)
                .unwrap_or(true)
            && item.season.is_some_and(|season| season > 0)
            && item.episode.is_some_and(|episode| episode > 0);
        if !valid_episode {
            return None;
        }

        let season = item.season?;
        let episode = item.episode?;
        Some(WatchFeedIdentity {
            series_stable_id: format!("tmdb:tv:{}", context.group.tmdb_id),
            episode_stable_id: format!("s{season:02}e{episode:02}"),
        })
    });

    WatchFeedIdentityPlan {
        authoritative,
        legacy_series_stable_id: format!("media-series:{media_path}"),
        legacy_episode_stable_id: media_path.to_string(),
    }
}

fn resolve_watch_feed_episodes(
    repository: &Repository,
    plan: &WatchFeedIdentityPlan,
) -> Result<Vec<WatchFeedEpisodeSource>, AcpError> {
    let mut sources = Vec::with_capacity(2);

    if let Some(identity) = &plan.authoritative {
        let series = repository
            .get_series_by_stable_id(&identity.series_stable_id)
            .map_err(|error| watch_feed_database_error("read watch feed series", &error))?;
        if let Some(series) = series {
            if let Some(episode) = repository
                .get_episode_by_stable_id(series.id, &identity.episode_stable_id)
                .map_err(|error| watch_feed_database_error("read watch feed episode", &error))?
            {
                sources.push(WatchFeedEpisodeSource { episode });
            }
        }
    }

    let legacy_series = repository
        .get_series_by_stable_id(&plan.legacy_series_stable_id)
        .map_err(|error| watch_feed_database_error("read legacy watch feed series", &error))?;
    if let Some(series) = legacy_series {
        if let Some(episode) = repository
            .get_episode_by_stable_id(series.id, &plan.legacy_episode_stable_id)
            .map_err(|error| watch_feed_database_error("read legacy watch feed episode", &error))?
        {
            if sources.iter().all(|source| source.episode.id != episode.id) {
                sources.push(WatchFeedEpisodeSource { episode });
            }
        }
    }

    Ok(sources)
}

fn load_watch_feed_episode_projection(
    repository: &Repository,
    episode_id: i64,
) -> Result<WatchFeedEpisodeProjection, AcpError> {
    let items = repository
        .list_watch_feed_items_by_episode(episode_id)
        .map_err(|error| watch_feed_database_error("list watch feed items", &error))?;
    let chapters = repository
        .list_chapters_by_episode(episode_id)
        .map_err(|error| watch_feed_database_error("list watch feed chapters", &error))?;
    let question_candidates = repository
        .list_question_candidates_by_episode(episode_id)
        .map_err(|error| watch_feed_database_error("list watch feed questions", &error))?;

    let chapters_by_id: HashMap<_, _> = chapters
        .into_iter()
        .map(|chapter| (chapter.id, chapter))
        .collect();
    let mut revisions_by_id = HashMap::new();
    let mut latest_revisions_by_chapter = HashMap::new();
    let mut assets_by_chapter = HashMap::new();

    for chapter in chapters_by_id.values() {
        if let Some(revision) = repository
            .get_latest_chapter_revision(chapter.id)
            .map_err(|error| watch_feed_database_error("read latest chapter revision", &error))?
        {
            latest_revisions_by_chapter.insert(chapter.id, revision.clone());
            revisions_by_id.insert(revision.id, revision);
        }
        let assets = repository
            .list_chapter_assets_by_chapter(chapter.id)
            .map_err(|error| watch_feed_database_error("list chapter assets", &error))?;
        assets_by_chapter.insert(chapter.id, assets);
    }

    for item in &items {
        if let Some(revision_id) = item.revision_id {
            if let Entry::Vacant(entry) = revisions_by_id.entry(revision_id) {
                if let Some(revision) = repository
                    .get_chapter_revision(revision_id)
                    .map_err(|error| watch_feed_database_error("read chapter revision", &error))?
                {
                    entry.insert(revision);
                }
            }
        }
    }

    Ok(WatchFeedEpisodeProjection {
        items,
        chapters_by_id,
        revisions_by_id,
        latest_revisions_by_chapter,
        question_candidates,
        assets_by_chapter,
    })
}

fn watch_feed_item_dedupe_key(item: &WatchFeedItemRecord) -> String {
    if !item.dedupe_key.trim().is_empty() {
        return format!("dedupe:{}", item.dedupe_key);
    }
    format!(
        "fallback:{}:{}:{}:{}:{}:{}",
        item.item_type,
        item.chapter_id.unwrap_or_default(),
        item.task_id.unwrap_or_default(),
        item.content_version,
        item.spoiler_level,
        item.content
    )
}

fn load_watch_feed_projection(
    repository: &Repository,
    sources: &[WatchFeedEpisodeSource],
) -> Result<Vec<AcpWatchFeedItem>, AcpError> {
    let projections = sources
        .iter()
        .map(|source| load_watch_feed_episode_projection(repository, source.episode.id))
        .collect::<Result<Vec<_>, _>>()?;
    let source_items = projections
        .iter()
        .map(|projection| projection.items.clone())
        .collect::<Vec<_>>();
    let selected = merge_watch_feed_item_records(&source_items);
    let mut mapped = Vec::with_capacity(selected.len());

    for (source_index, item) in selected {
        let projection = &projections[source_index];
        let chapter = item
            .chapter_id
            .and_then(|id| projection.chapters_by_id.get(&id));
        let revision = item
            .revision_id
            .and_then(|id| projection.revisions_by_id.get(&id))
            .or_else(|| {
                item.chapter_id
                    .and_then(|id| projection.latest_revisions_by_chapter.get(&id))
            });
        let question_candidate = projection.question_candidates.iter().find(|candidate| {
            item.item_type == "question"
                && candidate.episode_id == item.episode_id
                && candidate.chapter_id == item.chapter_id
                && candidate.task_id == item.task_id
                && candidate.question == item.content
        });
        let assets = item
            .chapter_id
            .and_then(|id| projection.assets_by_chapter.get(&id))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let (screenshot_refs, cover_ref) = opaque_asset_refs(assets);
        mapped.push(map_watch_feed_item(
            &item,
            chapter,
            revision,
            question_candidate,
            screenshot_refs,
            cover_ref,
        ));
    }

    mapped.sort_by(|left, right| {
        left.published_at_ms
            .unwrap_or_default()
            .cmp(&right.published_at_ms.unwrap_or_default())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(mapped)
}

/// Merge sources in caller order. The caller supplies authoritative data
/// before legacy data, so the first equivalent row wins deterministically.
fn merge_watch_feed_item_records(
    sources: &[Vec<WatchFeedItemRecord>],
) -> Vec<(usize, WatchFeedItemRecord)> {
    let mut selected = Vec::new();
    let mut seen_dedupe_keys = HashSet::new();
    let mut seen_ids = HashSet::new();

    for (source_index, source) in sources.iter().enumerate() {
        for item in source {
            if !seen_ids.insert(item.id)
                || !seen_dedupe_keys.insert(watch_feed_item_dedupe_key(item))
            {
                continue;
            }
            selected.push((source_index, item.clone()));
        }
    }
    selected
}

fn empty_watch_feed_response() -> AcpWatchFeedResponse {
    AcpWatchFeedResponse {
        source: AcpWatchFeedSource::Empty,
        items: Vec::new(),
    }
}

fn watch_feed_database_error(
    operation: &'static str,
    error: &lumina_library::DatabaseError,
) -> AcpError {
    tracing::warn!(
        code = ?error.code,
        details = ?error.details,
        operation,
        "watch feed database read failed"
    );
    AcpError::internal(Some(operation))
}

fn map_watch_feed_item(
    item: &WatchFeedItemRecord,
    chapter: Option<&ChapterRecord>,
    revision: Option<&ChapterRevisionRecord>,
    question_candidate: Option<&QuestionCandidateRecord>,
    screenshot_refs: Vec<String>,
    cover_ref: Option<String>,
) -> AcpWatchFeedItem {
    AcpWatchFeedItem {
        id: item.id,
        episode_id: item.episode_id,
        chapter_id: item.chapter_id,
        revision_id: item.revision_id,
        task_id: item.task_id,
        item_type: item.item_type.clone(),
        source: item.source.clone(),
        content: item.content.clone(),
        spoiler_level: item.spoiler_level.clone(),
        content_version: item.content_version.clone(),
        published_at_ms: item.published_at_ms,
        chapter: chapter.map(|chapter| AcpWatchFeedChapter {
            id: chapter.id,
            start_ms: chapter.start_ms,
            end_ms: chapter.end_ms,
            spoiler_level: chapter.spoiler_level.clone(),
            title: chapter.title.clone(),
            mainline: chapter.mainline.clone(),
            status: chapter.status.clone(),
        }),
        revision: revision.map(|revision| AcpWatchFeedRevision {
            id: revision.id,
            revision_number: revision.revision_number,
            revision_type: revision.revision_type.clone(),
            content: revision.content.clone(),
            source: revision.source.clone(),
            prompt_version: revision.prompt_version.clone(),
            validation_report: revision.validation_report.clone(),
            status: revision.status.clone(),
        }),
        question_candidate: question_candidate.map(|candidate| AcpWatchFeedQuestionCandidate {
            id: candidate.id,
            question: candidate.question.clone(),
            source: candidate.source.clone(),
            spoiler_level: candidate.spoiler_level.clone(),
            batch_key: candidate.batch_key.clone(),
            is_user_defined: candidate.is_user_defined,
            selected_at_ms: candidate.selected_at_ms,
        }),
        screenshot_refs,
        cover_ref,
    }
}

fn opaque_asset_refs(assets: &[ChapterAssetRecord]) -> (Vec<String>, Option<String>) {
    let screenshot_refs = assets
        .iter()
        .filter(|asset| asset.asset_type.eq_ignore_ascii_case("screenshot"))
        .map(opaque_asset_ref)
        .collect();
    let cover_ref = assets
        .iter()
        .find(|asset| asset.asset_type.eq_ignore_ascii_case("cover"))
        .map(opaque_asset_ref);
    (screenshot_refs, cover_ref)
}

/// Asset paths are native resources and never cross the Tauri business DTO.
/// The ID is enough for a later resource bridge to resolve them safely.
fn opaque_asset_ref(asset: &ChapterAssetRecord) -> String {
    format!("chapter-asset:{}", asset.id)
}

#[tauri::command]
pub async fn acp_respond_permission(
    app: AppHandle,
    request_id: String,
    option_id: Option<String>,
) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.respond_permission(&request_id, option_id)
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp respond permission join: {error}"))))?
}

#[tauri::command]
pub async fn acp_connect(
    state: State<'_, AppState>,
    cwd: Option<String>,
    profile_id: Option<String>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        acp.connect(
            cwd,
            profile_id,
            saved_session,
            settings,
            profiles,
            |event| {
                if let Err(error) = on_event.send(event) {
                    tracing::warn!(%error, "failed to send ACP connect event");
                }
            },
        )
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp connect join: {error}"))))?
}

#[tauri::command]
pub async fn acp_list_agent_sessions(
    state: State<'_, AppState>,
    profile_id: Option<String>,
    cwd: Option<String>,
) -> Result<AgentSessionListResult, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || {
        acp.list_agent_sessions(profile_id.as_deref(), cwd.as_deref())
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp list sessions join: {error}"))))?
}

#[tauri::command]
pub async fn acp_load_session(
    state: State<'_, AppState>,
    profile_id: Option<String>,
    session_id: String,
    cwd: Option<String>,
) -> Result<Vec<lumina_acp::runtime::service::LoadedTurn>, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || {
        acp.load_session_transcript(profile_id, session_id, cwd)
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp load session join: {error}"))))?
}

#[tauri::command]
pub async fn acp_delete_session(
    state: State<'_, AppState>,
    profile_id: Option<String>,
    session_id: String,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || acp.delete_session(profile_id, session_id))
        .await
        .map_err(|error| AcpError::internal(Some(&format!("acp delete session join: {error}"))))?
}

#[tauri::command]
pub async fn acp_sync_mcp_capabilities(
    _state: State<'_, AppState>,
    cwd: Option<String>,
    client_settings: Option<AcpClientSettings>,
) -> Result<(), AcpError> {
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        adapter::sync_mcp_capabilities(cwd.as_deref(), settings.vision_capable)
    })
    .await
    .map_err(|error| {
        AcpError::internal(Some(&format!("acp sync mcp capabilities join: {error}")))
    })??;
    Ok(())
}

#[tauri::command]
// Tauri maps IPC payload fields to command arguments directly.
#[allow(clippy::too_many_arguments)]
pub async fn acp_prompt(
    state: State<'_, AppState>,
    text: String,
    cwd: Option<String>,
    profile_id: Option<String>,
    context: Option<VideoPromptContext>,
    images: Option<Vec<PromptImage>>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
    task_id: Option<String>,
) -> Result<String, AcpError> {
    let acp = state.acp.clone();
    let library = state.library.clone();
    let snapshots = state.prompt_snapshots.clone();
    let ytdl = state.ytdl.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        let session_cwd = resolve_session_cwd(cwd.as_deref())?;
        let (mut snapshot, media_changed) = adapter::build_prompt_snapshot(
            &snapshots,
            &library,
            context.as_ref(),
            settings.vision_capable,
        )?;
        if let Some(page_url) = context
            .as_ref()
            .and_then(|value| value.media_path.as_deref())
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
        {
            match ytdl.cached_resolve(page_url) {
                Ok(Some(resolved)) => {
                    let choices = crate::ytdl::subtitle::list_choices(&resolved);
                    let selected = context
                        .as_ref()
                        .and_then(|value| value.subtitle_choice_id.as_deref())
                        .filter(|id| choices.iter().any(|choice| choice.id == *id))
                        .map(str::to_string)
                        .or_else(|| choices.first().map(|choice| choice.id.clone()));
                    let transcript = selected.as_deref().and_then(|choice_id| {
                        match ytdl.load_subtitle_choice(page_url, choice_id) {
                            Ok(transcript) => Some(transcript),
                            Err(error) => {
                                tracing::warn!(
                                    code = ?error.code,
                                    "online transcript unavailable for ACP snapshot"
                                );
                                None
                            }
                        }
                    });
                    if let Some(anchor) = snapshot.anchor.as_mut() {
                        anchor.subtitle_choice_id = selected;
                        if anchor
                            .media_title
                            .as_ref()
                            .is_none_or(|s| s.trim().is_empty())
                        {
                            anchor.media_title = resolved.title.clone();
                        }
                        if anchor.duration_ms.is_none() {
                            anchor.duration_ms = resolved.duration_ms;
                        }
                    }
                    snapshot.online = Some(OnlineMediaSnapshot {
                        media_id: resolved.media_id,
                        title: resolved.title,
                        duration_ms: resolved.duration_ms,
                        webpage_url: resolved.webpage_url,
                        extractor: resolved.extractor,
                        chapters: resolved.chapters,
                        subtitles: choices,
                        transcript,
                    });
                }
                Ok(None) => tracing::warn!("online resolve cache unavailable for ACP snapshot"),
                Err(error) => {
                    tracing::warn!(code = ?error.code, "online resolve cache failed")
                }
            }
        }
        if let Some(anchor) = snapshot.anchor.as_ref() {
            tracing::info!(
                position_ms = anchor.position_ms,
                media_changed,
                turn = snapshot.session.as_ref().map(|session| session.turn),
                "ACP 提问锚点已写入 snapshot"
            );
        }
        adapter::write_prompt_snapshot(&session_cwd, &snapshot)?;
        let context = enrich_prompt_context(context, &snapshot, media_changed);
        let images = images.unwrap_or_default();
        if let Some(task_id) = task_id.as_deref() {
            let composed = compose_task_composed(task_id, text.as_str(), context.as_ref())?;
            let shortcut_task = composed.task_id();
            if task_needs_validation(shortcut_task) {
                // Validated shortcuts reuse the live ACP session: every sender
                // call forwards all ACP events to the frontend. Intermediate
                // Finished events use overwrite semantics downstream, so the
                // final Finished wins without filtering here.
                let initial = composed.initial_prompt().to_string();
                let mut sender = |retry_text: &str| {
                    acp.prompt(
                        retry_text,
                        cwd.clone(),
                        profile_id.clone(),
                        context.clone(),
                        images.clone(),
                        saved_session.clone(),
                        settings.clone(),
                        profiles.clone(),
                        |event| {
                            if let Err(error) = on_event.send(event) {
                                tracing::warn!(%error, "failed to send ACP event");
                            }
                        },
                    )
                };
                run_shortcut_with_validation(shortcut_task, composed, initial, &mut sender)
            } else {
                let prompt_text = composed.initial_prompt().to_string();
                acp.prompt(
                    prompt_text,
                    cwd,
                    profile_id,
                    context,
                    images,
                    saved_session,
                    settings,
                    profiles,
                    |event| {
                        if let Err(error) = on_event.send(event) {
                            tracing::warn!(%error, "failed to send ACP event");
                        }
                    },
                )
            }
        } else {
            acp.prompt(
                text,
                cwd,
                profile_id,
                context,
                images,
                saved_session,
                settings,
                profiles,
                |event| {
                    if let Err(error) = on_event.send(event) {
                        tracing::warn!(%error, "failed to send ACP event");
                    }
                },
            )
        }
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp prompt join: {error}"))))?
}

/// Compose a versioned shortcut task prompt without changing the ordinary chat
/// path. The task prompt is sent through the existing ACP session; no new
/// session, history entry or MCP evidence is created here.
/// String wrapper behind [`compose_task_composed`], kept for tests.
#[cfg(test)]
fn compose_task_prompt(
    task_id: &str,
    user_text: &str,
    context: Option<&VideoPromptContext>,
) -> Result<String, AcpError> {
    compose_task_composed(task_id, user_text, context)
        .map(|prompt| prompt.initial_prompt().to_string())
}

/// Reusable [`ComposedPrompt`] behind [`compose_task_prompt`].
///
/// Validation and error messages are identical to the original string helper;
/// the composed form additionally carries the validation-retry state used by
/// [`run_shortcut_with_validation`].
fn compose_task_composed(
    task_id: &str,
    user_text: &str,
    context: Option<&VideoPromptContext>,
) -> Result<ComposedPrompt, AcpError> {
    let task_id = TaskId::from_str(task_id.trim())
        .map_err(|_| AcpError::bad_request("快捷 AI 操作不受支持"))?;
    let context = context
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AcpError::bad_request("请先打开视频后再使用快捷 AI 操作"))?;
    let media_path = context
        .media_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AcpError::bad_request("当前没有可分析的视频"))?;
    let position_ms = context.position_ms.unwrap_or_default();
    let slots = PromptSlots::default()
        .with_media(MediaContext {
            media_id: media_path.to_string(),
            title: context.media_title.clone(),
            duration_ms: context.duration_ms,
        })
        .with_episode(EpisodeContext {
            series_id: None,
            series_title: None,
            season: context.season,
            episode: context.episode,
            title: context.episode_title.clone(),
        })
        .with_viewing(ViewingContext {
            position_ms,
            spoiler_boundary: SpoilerBoundary::CurrentPosition,
        });
    let slots = if task_id
        .definition()
        .dynamic_slots
        .contains(&"user_instruction")
        && !user_text.trim().is_empty()
    {
        slots.with_user_instruction(user_text.trim())
    } else {
        slots
    };
    compose_prompt(task_id, &slots).map_err(|error| {
        AcpError::internal(Some(&format!("task prompt composition failed: {error}")))
    })
}

/// Whether a shortcut task owns a JSON validator.
///
/// Mirrors the `None` arm of [`validate_task_output`]: tasks without a JSON
/// contract skip validation and pass the reply through on the original path.
fn task_needs_validation(task_id: TaskId) -> bool {
    matches!(
        task_id,
        TaskId::ChapterRecap
            | TaskId::ChapterOutlook
            | TaskId::PlotSummary
            | TaskId::QuestionCandidates
    )
}

/// Shortcut validation-retry loop for tasks with a JSON validator.
///
/// Sends `initial_prompt` first, then validates each reply with
/// [`validate_task_output`]. An empty report returns the reply; otherwise the
/// report is appended as an incremental correction message and only
/// `delta.message` is sent next. When the retry budget is exhausted the last
/// reply is returned for graceful frontend degradation. Sender errors
/// (cancel/transport failures) propagate immediately without retry.
fn run_shortcut_with_validation(
    task_id: TaskId,
    mut composed: ComposedPrompt,
    initial_prompt: String,
    sender: &mut dyn FnMut(&str) -> Result<String, AcpError>,
) -> Result<String, AcpError> {
    let mut current = initial_prompt;
    loop {
        let reply = sender(current.as_str())?;
        let report = match validate_task_output(task_id, reply.as_str()) {
            Some(report) => report,
            None => return Ok(reply),
        };
        if report.is_empty() {
            return Ok(reply);
        }
        match composed.append_validation_report(report) {
            Ok(delta) => {
                current = delta.message.clone();
            }
            Err(_) => return Ok(reply),
        }
    }
}

/// Per-turn: progress always. Episode plot only when media/episode switched.
fn enrich_prompt_context(
    context: Option<VideoPromptContext>,
    snapshot: &lumina_mcp::LuminaMcpSnapshot,
    media_changed: bool,
) -> Option<VideoPromptContext> {
    let mut ctx = context.unwrap_or_default();
    ctx.episode_title = None;
    ctx.episode_overview = None;

    if let Some(anchor) = snapshot.anchor.as_ref() {
        if ctx.media_path.as_ref().is_none_or(|s| s.trim().is_empty()) {
            ctx.media_path = Some(anchor.media_path.clone());
        }
        if ctx.media_title.as_ref().is_none_or(|s| s.trim().is_empty()) {
            ctx.media_title = anchor.media_title.clone();
        }
        if ctx.position_ms.is_none() {
            ctx.position_ms = Some(anchor.position_ms);
        }
        if ctx.duration_ms.is_none() {
            ctx.duration_ms = anchor.duration_ms;
        }
        if ctx
            .subtitle_choice_id
            .as_ref()
            .is_none_or(|s| s.trim().is_empty())
        {
            ctx.subtitle_choice_id = anchor.subtitle_choice_id.clone();
        }
        if ctx.season.is_none() {
            ctx.season = anchor.season;
        }
        if ctx.episode.is_none() {
            ctx.episode = anchor.episode;
        }
    }
    if let Some(episode) = snapshot.current_episode.as_ref() {
        if ctx.season.is_none() {
            ctx.season = episode.season;
        }
        if ctx.episode.is_none() {
            ctx.episode = episode.episode;
        }
        // Conditional: pack episode plot into the prompt only on media switch.
        if media_changed {
            ctx.episode_title = episode.title.clone();
            ctx.episode_overview = episode.overview.clone();
        }
    }
    if ctx.is_empty() {
        None
    } else {
        Some(ctx)
    }
}

#[tauri::command]
pub fn acp_task_contracts() -> Vec<AcpTaskContract> {
    let repository = PromptRepository::new();
    TaskId::ALL
        .iter()
        .map(|task_id| {
            let definition = repository.definition(*task_id);
            AcpTaskContract {
                task_id: definition.task_id.as_str().to_string(),
                output_contract_version: definition.output_contract_version.to_string(),
                prompt_version: definition.version.to_string(),
            }
        })
        .collect()
}

/// Versioned task contracts owned by the Rust prompt repository.
///
/// Only identifiers and versions cross the IPC boundary here. Prompt role,
/// objectives and rules stay in Rust and are never exposed to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpTaskContract {
    pub task_id: String,
    pub output_contract_version: String,
    pub prompt_version: String,
}

#[tauri::command]
pub async fn acp_cancel(app: AppHandle) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.request_cancel();
        Ok(())
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp cancel join: {error}"))))?
}

#[tauri::command]
pub async fn acp_set_session_model(
    state: State<'_, AppState>,
    model_id: Option<String>,
    reasoning_effort: Option<String>,
    on_event: Channel<AcpEvent>,
) -> Result<crate::acp::AcpSessionModelOptions, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || {
        acp.set_session_model(model_id, reasoning_effort, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP set session model event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp set session model join: {error}"))))?
}

#[tauri::command]
pub async fn acp_new_chat(
    state: State<'_, AppState>,
    cwd: Option<String>,
    profile_id: Option<String>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let snapshots = state.prompt_snapshots.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        adapter::reset_prompt_snapshot_state(&snapshots);
        acp.new_chat(cwd, profile_id, settings, profiles, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP new chat event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp new chat join: {error}"))))?
}

/// Switch the visible conversation on the live agent process when possible
/// (close + resume/new without respawning). Falls back to a full connect
/// honoring the hint when no live process exists.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn acp_switch_session(
    state: State<'_, AppState>,
    cwd: Option<String>,
    profile_id: Option<String>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let snapshots = state.prompt_snapshots.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        adapter::reset_prompt_snapshot_state(&snapshots);
        acp.switch_session(
            cwd,
            profile_id,
            saved_session,
            settings,
            profiles,
            |event| {
                if let Err(error) = on_event.send(event) {
                    tracing::warn!(%error, "failed to send ACP switch session event");
                }
            },
        )
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp switch session join: {error}"))))?
}

#[tauri::command]
pub async fn acp_close(app: AppHandle) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        let result = state.acp.close_session();
        adapter::reset_prompt_snapshot_state(&state.prompt_snapshots);
        result
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp close join: {error}"))))?
}

#[tauri::command]
pub async fn acp_login_antigravity(proxy_port: Option<u16>) -> Result<String, AcpError> {
    tauri::async_runtime::spawn_blocking(move || lumina_acp::login_antigravity(proxy_port))
        .await
        .map_err(|error| {
            AcpError::internal(Some(&format!("acp login antigravity join: {error}")))
        })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_context() -> VideoPromptContext {
        VideoPromptContext {
            media_path: Some("D:/media/episode-01.mp4".into()),
            media_title: Some("第一集".into()),
            position_ms: Some(42_000),
            duration_ms: Some(600_000),
            season: Some(1),
            episode: Some(1),
            episode_title: Some("开端".into()),
            ..VideoPromptContext::default()
        }
    }

    #[test]
    fn task_contracts_come_from_the_prompt_repository() {
        let contracts = super::acp_task_contracts();
        assert_eq!(contracts.len(), TaskId::ALL.len());
        for contract in &contracts {
            assert!(!contract.task_id.is_empty());
            assert!(contract.output_contract_version.contains('.'));
            assert!(!contract.prompt_version.is_empty());
        }
        let recap = contracts
            .iter()
            .find(|contract| contract.task_id == "chapter_recap")
            .expect("chapter_recap contract");
        assert_eq!(recap.output_contract_version, "chapter_recap.v1");
    }

    #[test]
    fn task_route_uses_stable_id_and_current_position_boundary() {
        let result =
            compose_task_prompt("chapter_outlook", "请关注人物关系", Some(&video_context()));
        let prompt = match result {
            Ok(prompt) => prompt,
            Err(error) => panic!("expected task prompt, got {error}"),
        };

        assert!(prompt.contains("Lumina task: chapter_outlook"));
        assert!(prompt.contains("current_position"));
        assert!(prompt.contains("请关注人物关系"));
        assert!(prompt.contains("episode-01.mp4"));
    }

    #[test]
    fn task_route_does_not_add_instruction_to_tasks_without_that_slot() {
        let result = compose_task_prompt("plot_summary", "不要加入这段话", Some(&video_context()));
        let prompt = match result {
            Ok(prompt) => prompt,
            Err(error) => panic!("expected task prompt, got {error}"),
        };

        assert!(!prompt.contains("不要加入这段话"));
        assert!(prompt.contains("Lumina task: plot_summary"));
    }

    #[test]
    fn task_route_rejects_unknown_or_contextless_tasks() {
        let unknown = compose_task_prompt("not_a_task", "", Some(&video_context()));
        assert_eq!(
            unknown.as_ref().err().map(|error| error.message.as_str()),
            Some("快捷 AI 操作不受支持")
        );

        let missing_context = compose_task_prompt("chapter_recap", "", None);
        assert_eq!(
            missing_context
                .as_ref()
                .err()
                .map(|error| error.message.as_str()),
            Some("请先打开视频后再使用快捷 AI 操作")
        );
    }

    fn test_composed(task_id: TaskId) -> ComposedPrompt {
        let slots = PromptSlots::default();
        match compose_prompt(task_id, &slots) {
            Ok(prompt) => prompt,
            Err(error) => panic!("expected test prompt, got {error}"),
        }
    }

    fn valid_recap_reply() -> String {
        let version = TaskId::ChapterRecap.definition().output_contract_version;
        format!(r#"{{"version": "{version}", "summary": "Grounded recap.", "evidence": []}}"#)
    }

    #[test]
    fn shortcut_loop_returns_first_reply_when_valid() {
        let composed = test_composed(TaskId::ChapterRecap);
        let initial = composed.initial_prompt().to_string();
        let expected = valid_recap_reply();
        let mut calls = Vec::new();
        let mut sender = |text: &str| -> Result<String, AcpError> {
            calls.push(text.to_string());
            Ok(expected.clone())
        };
        let result = match run_shortcut_with_validation(
            TaskId::ChapterRecap,
            composed,
            initial.clone(),
            &mut sender,
        ) {
            Ok(reply) => reply,
            Err(error) => panic!("expected reply, got {error}"),
        };
        assert_eq!(result, expected);
        assert_eq!(calls, vec![initial]);
    }

    #[test]
    fn shortcut_loop_retries_with_incremental_delta() {
        let composed = test_composed(TaskId::ChapterRecap);
        let initial = composed.initial_prompt().to_string();
        let fixed = valid_recap_reply();
        let mut calls = Vec::new();
        let mut first = true;
        let mut sender = |text: &str| -> Result<String, AcpError> {
            calls.push(text.to_string());
            if first {
                first = false;
                Ok("not json at all".to_string())
            } else {
                Ok(fixed.clone())
            }
        };
        let result = match run_shortcut_with_validation(
            TaskId::ChapterRecap,
            composed,
            initial.clone(),
            &mut sender,
        ) {
            Ok(reply) => reply,
            Err(error) => panic!("expected retried reply, got {error}"),
        };
        assert_eq!(result, fixed);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], initial);
        assert!(calls[1].contains("Validation retry"));
        assert!(calls[1].contains("invalid_json"));
        assert!(!calls[1].contains("Lumina task:"));
    }

    #[test]
    fn shortcut_loop_returns_last_reply_when_budget_exhausted() {
        let composed = test_composed(TaskId::ChapterRecap);
        let initial = composed.initial_prompt().to_string();
        let mut calls = Vec::new();
        let mut sender = |text: &str| -> Result<String, AcpError> {
            calls.push(text.to_string());
            Ok("still not json".to_string())
        };
        let result = match run_shortcut_with_validation(
            TaskId::ChapterRecap,
            composed,
            initial.clone(),
            &mut sender,
        ) {
            Ok(reply) => reply,
            Err(error) => panic!("budget exhaustion should return last reply, got {error}"),
        };
        assert_eq!(result, "still not json");
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0], initial);
        for retry in calls.iter().skip(1) {
            assert!(retry.contains("Validation retry"));
            assert!(!retry.contains("Lumina task:"));
        }
    }

    #[test]
    fn shortcut_loop_propagates_sender_errors() {
        let composed = test_composed(TaskId::ChapterRecap);
        let initial = composed.initial_prompt().to_string();
        let mut calls = Vec::new();
        let mut sender = |text: &str| -> Result<String, AcpError> {
            calls.push(text.to_string());
            Err(AcpError::cancelled())
        };
        let error = match run_shortcut_with_validation(
            TaskId::ChapterRecap,
            composed,
            initial,
            &mut sender,
        ) {
            Ok(reply) => panic!("expected sender error, got reply {reply}"),
            Err(error) => error,
        };
        assert_eq!(error.code, lumina_acp::AcpErrorCode::Cancelled);
        assert_eq!(calls.len(), 1);
    }

    fn stored_metadata(
        kind: StoredMetadataKind,
        tmdb_id: u64,
        series_tmdb_id: Option<u64>,
        season: Option<u32>,
        episode: Option<u32>,
    ) -> lumina_library::StoredMetadata {
        lumina_library::StoredMetadata {
            schema_version: 2,
            kind,
            tmdb_id,
            series_tmdb_id,
            title: "测试媒体".into(),
            original_title: None,
            original_language: None,
            title_zh: None,
            overview: None,
            year: None,
            season,
            episode,
            genres: Vec::new(),
            cast: Vec::new(),
            creators: Vec::new(),
            network: None,
            status: None,
            updated_at_ms: 1,
        }
    }

    fn feed_item(id: i64, dedupe_key: &str, content: &str) -> WatchFeedItemRecord {
        feed_item_in_scope(id, dedupe_key, content, None, Some(1))
    }

    fn feed_item_in_scope(
        id: i64,
        dedupe_key: &str,
        content: &str,
        chapter_id: Option<i64>,
        task_id: Option<i64>,
    ) -> WatchFeedItemRecord {
        WatchFeedItemRecord {
            id,
            episode_id: Some(1),
            chapter_id,
            revision_id: None,
            task_id,
            item_type: "summary".into(),
            source: "ai".into(),
            content: content.into(),
            spoiler_level: "current_chapter".into(),
            content_version: "v1".into(),
            dedupe_key: dedupe_key.into(),
            published_at_ms: Some(id),
            created_at_ms: id,
        }
    }

    #[test]
    fn watch_feed_identity_plan_prefers_valid_tmdb_episode_identity() {
        let context = MediaMetadataContext {
            media_path: "D:/media/episode-03.mp4".into(),
            group: stored_metadata(StoredMetadataKind::Series, 1234, None, None, None),
            item: Some(stored_metadata(
                StoredMetadataKind::Episode,
                5678,
                Some(1234),
                Some(2),
                Some(3),
            )),
            wiki: None,
            merged: None,
        };

        let plan = watch_feed_identity_plan(&context.media_path, Some(&context));
        assert_eq!(
            plan.authoritative,
            Some(WatchFeedIdentity {
                series_stable_id: "tmdb:tv:1234".into(),
                episode_stable_id: "s02e03".into(),
            })
        );
        assert_eq!(plan.legacy_episode_stable_id, "D:/media/episode-03.mp4");
    }

    #[test]
    fn watch_feed_identity_plan_falls_back_when_library_context_is_incomplete() {
        let context = MediaMetadataContext {
            media_path: "D:/media/episode-03.mp4".into(),
            group: stored_metadata(StoredMetadataKind::Series, 1234, None, None, None),
            item: Some(stored_metadata(
                StoredMetadataKind::Episode,
                5678,
                Some(9999),
                Some(2),
                Some(3),
            )),
            wiki: None,
            merged: None,
        };

        let plan = watch_feed_identity_plan(&context.media_path, Some(&context));
        assert!(plan.authoritative.is_none());
        assert_eq!(
            plan.legacy_series_stable_id,
            "media-series:D:/media/episode-03.mp4"
        );
    }

    #[test]
    fn watch_feed_merge_is_deterministic_and_authoritative_first() {
        let authoritative = vec![feed_item(10, "summary:1", "权威摘要")];
        let legacy = vec![
            feed_item(20, "summary:1", "旧摘要"),
            feed_item(21, "question:1", "旧问题"),
        ];

        let first = merge_watch_feed_item_records(&[authoritative.clone(), legacy.clone()]);
        let second = merge_watch_feed_item_records(&[authoritative, legacy]);

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|(source, item)| (*source, item.id, item.content.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, 10, "权威摘要"), (1, 21, "旧问题")]
        );
    }

    #[test]
    fn watch_feed_fallback_key_keeps_same_text_in_different_chapters_or_tasks() {
        let same_text_different_chapter = feed_item_in_scope(30, "", "相同内容", Some(1), Some(7));
        let same_text_different_task = feed_item_in_scope(31, "", "相同内容", Some(1), Some(8));
        let same_text_different_chapter_again =
            feed_item_in_scope(32, "", "相同内容", Some(2), Some(7));

        let merged = merge_watch_feed_item_records(&[
            vec![same_text_different_chapter],
            vec![same_text_different_task, same_text_different_chapter_again],
        ]);

        assert_eq!(
            merged.iter().map(|(_, item)| item.id).collect::<Vec<_>>(),
            vec![30, 31, 32]
        );
    }

    #[test]
    fn watch_feed_dto_preserves_projection_links_and_spoiler_fields() {
        let item = WatchFeedItemRecord {
            id: 7,
            episode_id: Some(2),
            chapter_id: Some(3),
            revision_id: Some(4),
            task_id: Some(5),
            item_type: "chapter".to_string(),
            source: "ai".to_string(),
            content: "章节主线".to_string(),
            spoiler_level: "current_chapter".to_string(),
            content_version: "v1".to_string(),
            dedupe_key: "task:5:chapter-feed:1".to_string(),
            published_at_ms: Some(100),
            created_at_ms: 100,
        };
        let chapter = ChapterRecord {
            id: 3,
            episode_id: 2,
            stable_id: "chapter-1".to_string(),
            source: "ai".to_string(),
            start_ms: 1_000,
            end_ms: 2_000,
            spoiler_level: "current_chapter".to_string(),
            title: Some("初见".to_string()),
            mainline: Some("主线".to_string()),
            status: "ready".to_string(),
            created_at_ms: 100,
            updated_at_ms: 100,
        };

        let dto = map_watch_feed_item(
            &item,
            Some(&chapter),
            None,
            None,
            vec!["opaque-frame-1".to_string()],
            Some("opaque-cover-1".to_string()),
        );

        assert_eq!(dto.chapter_id, Some(3));
        assert_eq!(dto.revision_id, Some(4));
        assert_eq!(dto.spoiler_level, "current_chapter");
        assert_eq!(
            dto.chapter
                .as_ref()
                .and_then(|value| value.title.as_deref()),
            Some("初见")
        );
        assert_eq!(dto.screenshot_refs, vec!["opaque-frame-1"]);
        assert_eq!(dto.cover_ref.as_deref(), Some("opaque-cover-1"));
    }

    #[test]
    fn watch_feed_asset_paths_become_opaque_resource_ids() {
        let assets = vec![
            ChapterAssetRecord {
                id: 11,
                chapter_id: 3,
                asset_type: "screenshot".to_string(),
                path: r"C:\private\frame.png".to_string(),
                content_hash: "frame-hash".to_string(),
                captured_at_ms: 1_000,
                width: None,
                height: None,
                source: "ai".to_string(),
                created_at_ms: 1_000,
            },
            ChapterAssetRecord {
                id: 12,
                chapter_id: 3,
                asset_type: "cover".to_string(),
                path: r"C:\private\cover.png".to_string(),
                content_hash: "cover-hash".to_string(),
                captured_at_ms: 1_100,
                width: None,
                height: None,
                source: "ai".to_string(),
                created_at_ms: 1_100,
            },
        ];

        let (screenshots, cover) = opaque_asset_refs(&assets);
        assert_eq!(screenshots, vec!["chapter-asset:11"]);
        assert_eq!(cover.as_deref(), Some("chapter-asset:12"));
        assert!(!screenshots
            .iter()
            .any(|reference| reference.contains("private")));
        assert!(!cover.unwrap_or_default().contains("private"));
    }

    #[test]
    fn empty_watch_feed_response_is_safe_and_explicit() {
        let response = empty_watch_feed_response();
        assert_eq!(response.source, AcpWatchFeedSource::Empty);
        assert!(response.items.is_empty());
    }
}
