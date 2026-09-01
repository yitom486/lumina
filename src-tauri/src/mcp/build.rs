//! Build per-prompt MCP snapshots with library warm-cache policy.

use std::path::PathBuf;

use crate::acp::VideoPromptContext;
use crate::library::LibraryError;
use crate::library::{
    discover_library_root_for_media, load_library_index, resolve_media_in_index,
    series_cache_from_context, MediaLibraryService,
};
use crate::mcp::snapshot::{
    should_warm_series_library, AgentCapabilities, LuminaMcpSnapshot, PlaybackLite, PromptAnchor,
    SessionPolicy, LIBRARY_WARM_EVERY, SNAPSHOT_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Default)]
pub struct PromptSnapshotState {
    pub turn: u32,
    pub library_warmed_turn: u32,
    pub last_media_path: Option<String>,
}

impl PromptSnapshotState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn next_snapshot(
        &mut self,
        context: Option<&VideoPromptContext>,
        library: &MediaLibraryService,
        vision_capable: bool,
    ) -> Result<LuminaMcpSnapshot, LibraryError> {
        self.turn += 1;
        let turn = self.turn;
        let media_path = context
            .and_then(|ctx| ctx.media_path.as_deref())
            .filter(|path| !path.trim().is_empty());
        let path_changed = media_path
            .map(|path| self.last_media_path.as_deref() != Some(path))
            .unwrap_or(false);
        if let Some(path) = media_path {
            if path_changed || self.last_media_path.is_none() {
                self.last_media_path = Some(path.to_string());
            }
        }

        let warm = media_path.is_some() && should_warm_series_library(turn, path_changed);
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
        let series_cache = if warm {
            media_path.and_then(|path| {
                library
                    .context_for_media(path.to_string())
                    .ok()
                    .flatten()
                    .map(|ctx| series_cache_from_context(&ctx))
            })
        } else {
            None
        };
        if warm {
            self.library_warmed_turn = turn;
        }

        let anchor = media_path.map(|path| PromptAnchor {
            media_path: path.to_string(),
            library_root: library_root
                .as_ref()
                .map(|root| root.to_string_lossy().to_string()),
            group_key: indexed.as_ref().map(|(key, _, _)| key.clone()),
            season: indexed.as_ref().and_then(|(_, season, _)| *season),
            episode: indexed.as_ref().and_then(|(_, _, episode)| *episode),
            position_ms: context.and_then(|ctx| ctx.position_ms).unwrap_or(0),
            sent_at_ms: snapshot_now_ms(),
            subtitle_choice_id: context
                .and_then(|ctx| ctx.subtitle_choice_id.clone())
                .filter(|value| !value.trim().is_empty()),
        });

        Ok(LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor,
            playback: context.map(playback_lite_from_video_context),
            library: series_cache,
            session: Some(SessionPolicy {
                turn,
                media_path: media_path.map(str::to_string),
                library_warmed_turn: self.library_warmed_turn,
                library_warm_every: LIBRARY_WARM_EVERY,
            }),
            capabilities: Some(AgentCapabilities { vision_capable }),
            updated_at_ms: snapshot_now_ms(),
        })
    }
}

fn playback_lite_from_video_context(context: &VideoPromptContext) -> PlaybackLite {
    PlaybackLite {
        media_path: context.media_path.clone(),
        media_title: context.media_title.clone(),
        position_ms: context.position_ms,
        duration_ms: context.duration_ms,
        chapter_title: context.chapter_title.clone(),
        notes_excerpt: context.notes_excerpt.clone(),
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
    use crate::mcp::snapshot::should_warm_series_library;

    #[test]
    fn warm_turns_follow_first_and_every_fifth() {
        assert!(should_warm_series_library(1, false));
        assert!(!should_warm_series_library(2, false));
        assert!(should_warm_series_library(5, false));
        assert!(should_warm_series_library(3, true));
    }
}
