//! Build per-prompt MCP snapshots with library warm-cache policy.

use std::path::PathBuf;

use crate::snapshot::{
    should_warm_series_library, AgentCapabilities, CurrentEpisodeLite, LuminaMcpSnapshot,
    PromptAnchor, SessionPolicy, LIBRARY_WARM_EVERY, SNAPSHOT_SCHEMA_VERSION,
};
use lumina_library::LibraryError;
use lumina_library::{
    discover_library_root_for_media, load_library_index, resolve_media_in_index,
    series_cache_from_context, MediaLibraryService,
};

/// Generic prompt context for snapshot building (M7).
/// Same playback fields the app previously passed via the ACP context type;
/// the app adapter converts. No ACP dependency.
#[derive(Debug, Clone, Default)]
pub struct McpPromptContext {
    pub media_path: Option<String>,
    pub media_title: Option<String>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub subtitle_choice_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptSnapshotState {
    pub turn: u32,
    pub library_warmed_turn: u32,
    pub last_media_path: Option<String>,
}

/// Snapshot plus whether the media path changed this turn (episode switch).
#[derive(Debug, Clone)]
pub struct SnapshotBuildResult {
    pub snapshot: LuminaMcpSnapshot,
    pub media_changed: bool,
}

impl PromptSnapshotState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn next_snapshot(
        &mut self,
        context: Option<&McpPromptContext>,
        library: &MediaLibraryService,
        vision_capable: bool,
    ) -> Result<SnapshotBuildResult, LibraryError> {
        self.turn += 1;
        let turn = self.turn;
        let media_path = context
            .and_then(|ctx| ctx.media_path.as_deref())
            .filter(|path| !path.trim().is_empty());
        let media_changed = media_path
            .map(|path| self.last_media_path.as_deref() != Some(path))
            .unwrap_or(false);
        if let Some(path) = media_path {
            if media_changed || self.last_media_path.is_none() {
                self.last_media_path = Some(path.to_string());
            }
        }

        let warm = media_path.is_some() && should_warm_series_library(turn, media_changed);
        let library_root = media_path.and_then(|path| {
            library
                .library_root_for_media(path)
                .or_else(|| discover_library_root_for_media(PathBuf::from(path).as_path()))
        });
        let indexed = media_path.and_then(|path| {
            library_root.as_ref().and_then(|root| {
                let root_path = PathBuf::from(root);
                let media = PathBuf::from(path);
                let index = load_library_index(&root_path).ok()??;
                let (file, _group) = resolve_media_in_index(&index, &media, &root_path).ok()??;
                Some((file.group_key.clone(), file.season, file.episode))
            })
        });
        let media_context = media_path.and_then(|path| {
            library
                .context_for_media(path.to_string())
                .ok()
                .flatten()
        });
        let series_cache = if warm {
            media_context.as_ref().map(series_cache_from_context)
        } else {
            None
        };
        // Always refresh current-episode plot into the snapshot file for MCP.
        let current_episode = media_context.as_ref().and_then(|ctx| {
            let overview = ctx
                .merged
                .as_ref()
                .and_then(|merged| merged.episode_overview.clone())
                .or_else(|| ctx.item.as_ref().and_then(|item| item.overview.clone()))
                .filter(|text| !text.trim().is_empty());
            let title = ctx
                .item
                .as_ref()
                .map(|item| item.title.clone())
                .filter(|text| !text.trim().is_empty());
            let season = indexed
                .as_ref()
                .and_then(|(_, season, _)| *season)
                .or_else(|| ctx.item.as_ref().and_then(|item| item.season));
            let episode = indexed
                .as_ref()
                .and_then(|(_, _, episode)| *episode)
                .or_else(|| ctx.item.as_ref().and_then(|item| item.episode));
            if title.is_none() && overview.is_none() && season.is_none() && episode.is_none() {
                return None;
            }
            Some(CurrentEpisodeLite {
                season,
                episode,
                title,
                overview,
            })
        });
        if warm {
            self.library_warmed_turn = turn;
        }

        let anchor = media_path.map(|path| PromptAnchor {
            media_path: path.to_string(),
            media_title: context
                .and_then(|ctx| ctx.media_title.clone())
                .filter(|value| !value.trim().is_empty()),
            library_root: library_root
                .as_ref()
                .map(|root| root.to_string_lossy().to_string()),
            group_key: indexed.as_ref().map(|(key, _, _)| key.clone()),
            season: indexed.as_ref().and_then(|(_, season, _)| *season),
            episode: indexed.as_ref().and_then(|(_, _, episode)| *episode),
            position_ms: context.and_then(|ctx| ctx.position_ms).unwrap_or(0),
            duration_ms: context.and_then(|ctx| ctx.duration_ms),
            sent_at_ms: snapshot_now_ms(),
            subtitle_choice_id: context
                .and_then(|ctx| ctx.subtitle_choice_id.clone())
                .filter(|value| !value.trim().is_empty()),
        });

        Ok(SnapshotBuildResult {
            snapshot: LuminaMcpSnapshot {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                anchor,
                current_episode,
                library: series_cache,
                session: Some(SessionPolicy {
                    turn,
                    media_path: media_path.map(str::to_string),
                    library_warmed_turn: self.library_warmed_turn,
                    library_warm_every: LIBRARY_WARM_EVERY,
                }),
                capabilities: Some(AgentCapabilities {
                    vision_capable,
                    subtitle_workshop_enabled: false,
                    video_annotations_enabled: true,
                }),
                online: None,
                updated_at_ms: snapshot_now_ms(),
            },
            media_changed,
        })
    }
}

fn snapshot_now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{McpPromptContext, PromptSnapshotState};
    use crate::snapshot::should_warm_series_library;
    use lumina_library::MediaLibraryService;

    #[test]
    fn warm_turns_follow_first_and_every_fifth() {
        assert!(should_warm_series_library(1, false));
        assert!(!should_warm_series_library(2, false));
        assert!(should_warm_series_library(5, false));
        assert!(should_warm_series_library(3, true));
    }

    #[test]
    fn media_changed_true_on_first_path_and_switch() {
        let library = MediaLibraryService::new();
        let mut state = PromptSnapshotState::default();
        let a = McpPromptContext {
            media_path: Some(r"D:\videos\a.mp4".into()),
            position_ms: Some(1_000),
            ..Default::default()
        };
        let first = state
            .next_snapshot(Some(&a), &library, false)
            .expect("first snapshot");
        assert!(first.media_changed);

        let same = state
            .next_snapshot(Some(&a), &library, false)
            .expect("same media");
        assert!(!same.media_changed);

        let b = McpPromptContext {
            media_path: Some(r"D:\videos\b.mp4".into()),
            position_ms: Some(2_000),
            ..Default::default()
        };
        let switched = state
            .next_snapshot(Some(&b), &library, false)
            .expect("switched media");
        assert!(switched.media_changed);
    }
}
