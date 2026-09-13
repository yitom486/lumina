//! Durable TMDb metadata documents and current-media context reader.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::library::error::LibraryError;
use crate::library::model::{
    EpisodeFile, EpisodeIndexEntry, GroupResolution, IndexedMediaFile, LibraryIndex, MediaGroup,
    MediaMetadataContext, MergedMediaContext, MetadataCastMember, MetadataMediaType,
    MetadataWriteResult, SeriesLibraryCache, SeriesReading, StoredMetadata, StoredMetadataKind,
    TmdbConfig, TmdbGroupStatus, WikiEnrichmentCandidate, WikiEnrichmentPreview, WikiGroupStatus,
    WikiMatchMethod, WikiMetadata, WikiWriteResult, METADATA_SCHEMA_VERSION,
};
use crate::library::paths::{display_relative_path, relativize_under_root};
use crate::library::wikitext::episode_plot_for;
use crate::library::{resolver, store, wikipedia};

const MAX_OVERVIEW_CAST: usize = 15;
const MAX_CREATORS: usize = 5;

pub fn write_confirmed_metadata(
    root: &Path,
    index: &LibraryIndex,
    group: &MediaGroup,
    tmdb_id: u64,
    media_type: MetadataMediaType,
    config: &TmdbConfig,
) -> Result<MetadataWriteResult, LibraryError> {
    let mut written_files = Vec::new();
    match media_type {
        MetadataMediaType::Movie => {
            let detail = resolver::fetch_tmdb_details(config, tmdb_id, media_type, None, None)?;
            let credits = resolver::fetch_tmdb_credits(config, tmdb_id, media_type, None, None)?;
            let document = document_from_tmdb(
                &detail,
                Some(&credits),
                StoredMetadataKind::Movie,
                tmdb_id,
                None,
                None,
                None,
            )?;
            written_files.push(relative_group_path(
                root,
                &store::save_group_json(root, &group.key, "movie.json", &document)?,
            ));
        }
        MetadataMediaType::Tv => {
            let detail = resolver::fetch_tmdb_details(config, tmdb_id, media_type, None, None)?;
            let credits = resolver::fetch_tmdb_credits(config, tmdb_id, media_type, None, None)?;
            let document = document_from_tmdb(
                &detail,
                Some(&credits),
                StoredMetadataKind::Series,
                tmdb_id,
                None,
                None,
                None,
            )?;
            written_files.push(relative_group_path(
                root,
                &store::save_group_json(root, &group.key, "series.json", &document)?,
            ));
            for file in group_files(index, group) {
                let (Some(season), Some(episode)) = (file.season, file.episode) else {
                    continue;
                };
                let detail = resolver::fetch_tmdb_details(
                    config,
                    tmdb_id,
                    media_type,
                    Some(season),
                    Some(episode),
                )?;
                let document = document_from_tmdb(
                    &detail,
                    None,
                    StoredMetadataKind::Episode,
                    tmdb_id,
                    Some(tmdb_id),
                    Some(season),
                    Some(episode),
                )?;
                let file_name = format!("S{season:02}E{episode:02}.json");
                written_files.push(relative_group_path(
                    root,
                    &store::save_group_json(root, &group.key, &file_name, &document)?,
                ));
            }
        }
    }
    Ok(MetadataWriteResult {
        root: root.to_string_lossy().to_string(),
        group_key: group.key.clone(),
        tmdb_id,
        media_type,
        written_files,
    })
}

pub fn load_context(
    root: &Path,
    index: &LibraryIndex,
    media_path: &Path,
) -> Result<Option<MediaMetadataContext>, LibraryError> {
    let relative = relativize_under_root(root, media_path)?;
    let Some(file) = index
        .files
        .iter()
        .find(|file| file.relative_path == relative)
    else {
        return Ok(None);
    };
    let Some(group) = index
        .groups
        .iter()
        .find(|group| group.key == file.group_key)
    else {
        return Ok(None);
    };
    let media_type = match group.resolution {
        crate::library::model::GroupResolution::Matched { media_type, .. } => media_type,
        _ => return Ok(None),
    };
    let overview_name = match media_type {
        MetadataMediaType::Movie => "movie.json",
        MetadataMediaType::Tv => "series.json",
    };
    let Some(group_document) = store::load_group_json(root, &group.key, overview_name)? else {
        return Ok(None);
    };
    let item = match (media_type, file.season, file.episode) {
        (MetadataMediaType::Tv, Some(season), Some(episode)) => {
            store::load_group_json(root, &group.key, &format!("S{season:02}E{episode:02}.json"))?
        }
        _ => None,
    };
    Ok(Some(build_media_context(
        media_path.to_string_lossy().to_string(),
        group_document,
        item,
        store::load_group_json(root, &group.key, "wiki.json")?,
    )))
}

pub fn build_media_context(
    media_path: String,
    group: StoredMetadata,
    item: Option<StoredMetadata>,
    wiki: Option<WikiMetadata>,
) -> MediaMetadataContext {
    let merged = Some(merge_media_context(&group, item.as_ref(), wiki.as_ref()));
    MediaMetadataContext {
        media_path,
        group,
        item,
        wiki,
        merged,
    }
}

pub fn merge_media_context(
    group: &StoredMetadata,
    item: Option<&StoredMetadata>,
    wiki: Option<&WikiMetadata>,
) -> MergedMediaContext {
    let overview = item
        .and_then(|entry| entry.overview.clone())
        .or_else(|| group.overview.clone());
    let synopsis = wiki
        .map(|entry| entry.extract.clone())
        .or_else(|| group.overview.clone());
    let tmdb_episode_overview = item.and_then(|entry| entry.overview.clone());
    let wiki_episode_plot = item.and_then(|entry| {
        let season = entry.season.unwrap_or(1);
        let episode = entry.episode?;
        wiki.and_then(|entry| episode_plot_for(&entry.episodes, season, episode))
    });
    let episode_overview = tmdb_episode_overview.or_else(|| wiki_episode_plot.clone());
    let characters = wiki
        .filter(|entry| !entry.characters.is_empty())
        .map(|entry| entry.characters.clone());
    MergedMediaContext {
        overview,
        synopsis,
        episode_overview,
        characters,
        wiki_episode_plot,
        wiki_attribution: wiki.map(|entry| entry.attribution.clone()),
        wiki_page_url: wiki.map(|entry| entry.page_url.clone()),
    }
}

pub fn load_group_overview(
    root: &Path,
    group_key: &str,
    media_type: MetadataMediaType,
) -> Result<Option<StoredMetadata>, LibraryError> {
    let file_name = match media_type {
        MetadataMediaType::Movie => "movie.json",
        MetadataMediaType::Tv => "series.json",
    };
    store::load_group_json(root, group_key, file_name)
}

pub fn load_context_at_root(
    root: &Path,
    media_path: &Path,
) -> Result<Option<MediaMetadataContext>, LibraryError> {
    let Some(index) = store::load(root)? else {
        return Ok(None);
    };
    if let Some(context) = load_context(root, &index, media_path)? {
        return Ok(Some(context));
    }
    if let Ok(Some((file, _group))) = resolve_media_in_index(&index, media_path, root) {
        return load_context_for_group(
            root,
            &file.group_key,
            media_path,
            file.season,
            file.episode,
        );
    }
    Ok(None)
}

pub fn series_cache_from_context(context: &MediaMetadataContext) -> SeriesLibraryCache {
    let merged = context.merged.as_ref();
    SeriesLibraryCache {
        title: context.group.title.clone(),
        synopsis: merged
            .and_then(|entry| entry.synopsis.clone())
            .or_else(|| context.group.overview.clone()),
        characters: merged.and_then(|entry| entry.characters.clone()),
        creators: context.group.creators.clone(),
        network: context.group.network.clone(),
        status: context.group.status.clone(),
        wiki_attribution: merged.and_then(|entry| entry.wiki_attribution.clone()),
        wiki_page_url: merged.and_then(|entry| entry.wiki_page_url.clone()),
    }
}

pub fn episode_index_for_group(
    root: &Path,
    group_key: &str,
) -> Result<Vec<EpisodeIndexEntry>, LibraryError> {
    let dir = store::group_dir(root, group_key);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)
        .map_err(|error| LibraryError::storage_failed(Some(&format!("read group dir: {error}"))))?
    {
        let entry = entry.map_err(|error| {
            LibraryError::storage_failed(Some(&format!("read group entry: {error}")))
        })?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some((season, episode)) = parse_episode_file_name(&file_name) else {
            continue;
        };
        let document = match store::load_group_json::<StoredMetadata>(root, group_key, &file_name) {
            Ok(document) => document,
            Err(error) => {
                tracing::warn!(
                    group_key,
                    file = %file_name,
                    reason = %error.message,
                    "episode index skipped unreadable metadata file"
                );
                None
            }
        };
        let (title, overview) = if let Some(document) = document {
            (document.title, document.overview)
        } else {
            (format!("S{season:02}E{episode:02}"), None)
        };
        entries.push(EpisodeIndexEntry {
            season,
            episode,
            title,
            overview,
        });
    }
    entries.sort_by(|left, right| {
        left.season
            .cmp(&right.season)
            .then_with(|| left.episode.cmp(&right.episode))
    });
    Ok(entries)
}

pub fn resolve_episode_media_file<'a>(
    index: &'a LibraryIndex,
    group_key: &str,
    season: u32,
    episode: u32,
) -> Result<&'a IndexedMediaFile, LibraryError> {
    if season == 0 || episode == 0 {
        return Err(LibraryError::invalid_input("season 与 episode 须大于 0"));
    }
    index
        .files
        .iter()
        .find(|file| {
            file.group_key == group_key
                && file.season == Some(season)
                && file.episode == Some(episode)
        })
        .ok_or_else(|| LibraryError::group_not_found(Some(&format!("S{season:02}E{episode:02}"))))
}

/// Assemble the P6 reading shelf for one series group: indexed episode files
/// with titles (metadata or `SxxExx` fallback), sorted. IO-free apart from
/// the episode title lookup, which tolerates a missing group dir.
pub fn assemble_series(
    root: &Path,
    index: &LibraryIndex,
    group_key: &str,
    label: String,
) -> SeriesReading {
    use std::collections::HashMap;

    let titles: HashMap<(u32, u32), String> = episode_index_for_group(root, group_key)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| ((entry.season, entry.episode), entry.title))
        .collect();
    let mut episodes: Vec<EpisodeFile> = index
        .files
        .iter()
        .filter(|file| file.group_key == group_key)
        .filter_map(|file| {
            let (season, episode) = match (file.season, file.episode) {
                (Some(season), Some(episode)) if season > 0 && episode > 0 => (season, episode),
                _ => return None,
            };
            let title = titles
                .get(&(season, episode))
                .cloned()
                .unwrap_or_else(|| format!("S{season:02}E{episode:02}"));
            Some(EpisodeFile {
                season,
                episode,
                title,
                path: root
                    .join(&file.relative_path)
                    .to_string_lossy()
                    .into_owned(),
            })
        })
        .collect();
    episodes.sort_by_key(|episode| (episode.season, episode.episode));
    SeriesReading {
        root: root.to_string_lossy().into_owned(),
        group_key: group_key.to_owned(),
        label,
        episodes,
    }
}

pub fn resolve_media_in_index<'a>(
    index: &'a LibraryIndex,
    media_path: &Path,
    root: &Path,
) -> Result<Option<(&'a IndexedMediaFile, &'a MediaGroup)>, LibraryError> {
    let relative = relativize_under_root(root, media_path)?;
    let Some(file) = index
        .files
        .iter()
        .find(|file| file.relative_path == relative)
    else {
        return Ok(None);
    };
    let Some(group) = index
        .groups
        .iter()
        .find(|group| group.key == file.group_key)
    else {
        return Ok(None);
    };
    Ok(Some((file, group)))
}

/// Load metadata documents directly from the group folder (bypasses index lookup).
pub fn load_context_for_group(
    root: &Path,
    group_key: &str,
    media_path: &Path,
    season: Option<u32>,
    episode: Option<u32>,
) -> Result<Option<MediaMetadataContext>, LibraryError> {
    let Some(group_document) = store::load_group_json(root, group_key, "series.json")? else {
        return Ok(None);
    };
    let item = match (season, episode) {
        (Some(season), Some(episode)) => {
            store::load_group_json(root, group_key, &format!("S{season:02}E{episode:02}.json"))?
        }
        _ => None,
    };
    Ok(Some(build_media_context(
        media_path.to_string_lossy().to_string(),
        group_document,
        item,
        store::load_group_json(root, group_key, "wiki.json")?,
    )))
}

fn parse_episode_file_name(file_name: &str) -> Option<(u32, u32)> {
    if !file_name.starts_with('S') || !file_name.ends_with(".json") {
        return None;
    }
    let stem = file_name.strip_suffix(".json")?;
    let body = stem.strip_prefix('S')?;
    let (season_text, episode_text) = body.split_once('E')?;
    let season = season_text.parse().ok()?;
    let episode = episode_text.parse().ok()?;
    if season == 0 || episode == 0 {
        return None;
    }
    Some((season, episode))
}

pub fn preview_wikipedia_enrichment(
    root: &Path,
    group_key: &str,
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &TmdbConfig,
) -> Result<WikiEnrichmentPreview, LibraryError> {
    let Some(stored) = load_group_overview(root, group_key, media_type)? else {
        return Err(LibraryError::invalid_input("请先完成 TMDb 匹配"));
    };
    let existing = wikipedia::load_existing_wiki(root, group_key)?;
    wikipedia::preview_enrichment(&stored, tmdb_id, media_type, tmdb, existing.as_ref())
}

pub fn refresh_wikipedia_page(
    root: &Path,
    group_key: &str,
) -> Result<WikiWriteResult, LibraryError> {
    wikipedia::refresh_existing_page(root, group_key)
}

pub fn refresh_tmdb_metadata(
    root: &Path,
    index: &LibraryIndex,
    group: &MediaGroup,
    config: &TmdbConfig,
) -> Result<MetadataWriteResult, LibraryError> {
    let GroupResolution::Matched {
        tmdb_id,
        media_type,
    } = group.resolution
    else {
        return Err(LibraryError::invalid_input("请先完成 TMDb 匹配"));
    };
    write_confirmed_metadata(root, index, group, tmdb_id, media_type, config)
}

pub fn tmdb_statuses_for_root(
    root: &Path,
    index: &LibraryIndex,
    groups: &[MediaGroup],
) -> Result<Vec<TmdbGroupStatus>, LibraryError> {
    Ok(groups
        .iter()
        .filter_map(|group| {
            let GroupResolution::Matched { media_type, .. } = group.resolution else {
                return None;
            };
            let stored = load_group_overview(root, &group.key, media_type)
                .ok()
                .flatten();
            let episode_file_count = group_files(index, group)
                .iter()
                .filter(|file| file.season.is_some() && file.episode.is_some())
                .count() as u32;
            Some(TmdbGroupStatus {
                group_key: group.key.clone(),
                title: stored.as_ref().map(|item| item.title.clone()),
                cast_count: stored
                    .as_ref()
                    .map(|item| item.cast.len() as u32)
                    .unwrap_or(0),
                creators_count: stored
                    .as_ref()
                    .map(|item| item.creators.len() as u32)
                    .unwrap_or(0),
                episode_file_count,
                network: stored.as_ref().and_then(|item| item.network.clone()),
                status: stored.as_ref().and_then(|item| item.status.clone()),
                updated_at_ms: stored.map(|item| item.updated_at_ms).unwrap_or(0),
            })
        })
        .collect())
}

pub fn wikipedia_statuses_for_root(
    root: &Path,
    index: &LibraryIndex,
) -> Result<Vec<WikiGroupStatus>, LibraryError> {
    wikipedia::statuses_for_matched_groups(root, &index.groups)
}

pub fn apply_wikipedia_page(
    root: &Path,
    group_key: &str,
    candidate: WikiEnrichmentCandidate,
    match_method: WikiMatchMethod,
    candidates_considered: u32,
) -> Result<WikiWriteResult, LibraryError> {
    wikipedia::write_selected_page(
        root,
        group_key,
        &candidate,
        match_method,
        candidates_considered,
    )
}

fn group_files<'a>(index: &'a LibraryIndex, group: &MediaGroup) -> Vec<&'a IndexedMediaFile> {
    index
        .files
        .iter()
        .filter(|file| file.group_key == group.key)
        .collect()
}

fn document_from_tmdb(
    detail: &Value,
    credits: Option<&Value>,
    kind: StoredMetadataKind,
    expected_id: u64,
    series_tmdb_id: Option<u64>,
    season: Option<u32>,
    episode: Option<u32>,
) -> Result<StoredMetadata, LibraryError> {
    let tmdb_id = detail
        .get("id")
        .and_then(Value::as_u64)
        .unwrap_or(expected_id);
    let title = detail
        .get("title")
        .or_else(|| detail.get("name"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| LibraryError::remote_request_failed(Some("TMDb detail title missing")))?
        .to_string();
    let date = detail
        .get("release_date")
        .or_else(|| detail.get("first_air_date"))
        .or_else(|| detail.get("air_date"))
        .and_then(Value::as_str);

    let (cast, creators, network, status) = match kind {
        StoredMetadataKind::Series => {
            let cast = credits
                .map(|value| cast_from_credits(value, MAX_OVERVIEW_CAST))
                .unwrap_or_default();
            (
                cast,
                creators_from_tv_detail(detail),
                network_from_tv_detail(detail),
                status_from_tv_detail(detail),
            )
        }
        StoredMetadataKind::Movie => {
            let cast = credits
                .map(|value| cast_from_credits(value, MAX_OVERVIEW_CAST))
                .unwrap_or_default();
            let creators = credits.map(directors_from_credits).unwrap_or_default();
            (cast, creators, None, None)
        }
        StoredMetadataKind::Episode => (Vec::new(), Vec::new(), None, None),
    };

    Ok(StoredMetadata {
        schema_version: METADATA_SCHEMA_VERSION,
        kind,
        tmdb_id,
        series_tmdb_id,
        title,
        original_title: detail
            .get("original_title")
            .or_else(|| detail.get("original_name"))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string),
        overview: detail
            .get("overview")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string),
        year: date
            .and_then(|text| text.get(0..4))
            .and_then(|year| year.parse().ok()),
        season,
        episode,
        genres: detail
            .get("genres")
            .and_then(Value::as_array)
            .map(|genres| {
                genres
                    .iter()
                    .filter_map(|genre| genre.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        cast,
        creators,
        network,
        status,
        updated_at_ms: now_ms(),
    })
}

fn cast_from_credits(credits: &Value, limit: usize) -> Vec<MetadataCastMember> {
    let Some(array) = credits.get("cast").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut members = array
        .iter()
        .filter_map(cast_member_from_value)
        .collect::<Vec<_>>();
    members.sort_by_key(|member| member.order);
    members.truncate(limit);
    members
}

fn cast_member_from_value(value: &Value) -> Option<MetadataCastMember> {
    let name = value.get("name")?.as_str()?.trim();
    let character = value
        .get("character")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return None;
    }
    Some(MetadataCastMember {
        name: name.to_string(),
        character: if character.is_empty() {
            name.to_string()
        } else {
            character.to_string()
        },
        order: value.get("order").and_then(Value::as_u64).unwrap_or(999) as u32,
    })
}

fn creators_from_tv_detail(detail: &Value) -> Vec<String> {
    detail
        .get("created_by")
        .and_then(Value::as_array)
        .map(|creators| {
            creators
                .iter()
                .filter_map(|creator| creator.get("name").and_then(Value::as_str))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .take(MAX_CREATORS)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn directors_from_credits(credits: &Value) -> Vec<String> {
    credits
        .get("crew")
        .and_then(Value::as_array)
        .map(|crew| {
            crew.iter()
                .filter(|member| {
                    member
                        .get("job")
                        .and_then(Value::as_str)
                        .is_some_and(|job| job == "Director")
                })
                .filter_map(|member| member.get("name").and_then(Value::as_str))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .take(MAX_CREATORS)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn network_from_tv_detail(detail: &Value) -> Option<String> {
    detail
        .get("networks")
        .and_then(Value::as_array)
        .and_then(|networks| networks.first())
        .and_then(|network| network.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn status_from_tv_detail(detail: &Value) -> Option<String> {
    detail
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .map(str::to_string)
}

fn relative_group_path(root: &Path, path: &Path) -> String {
    display_relative_path(root, path)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::model::{WikiCharacter, WikiEpisodeSummary, WikiMatchInfo};
    use serde_json::json;

    #[test]
    fn maps_tmdb_tv_detail_and_credits_to_series_document() {
        let document = document_from_tmdb(
            &json!({
                "id": 289424,
                "name": "努力克服自卑的我们",
                "original_name": "모두가 자신의 무가치함과 싸우고 있다",
                "first_air_date": "2026-04-18",
                "overview": "简介",
                "status": "Ended",
                "genres": [{"name": "剧情"}],
                "created_by": [{"name": "Park Hae-young"}, {"name": "Cha Yeong-hun"}],
                "networks": [{"name": "JTBC"}]
            }),
            Some(&json!({
                "cast": [
                    {"name": "具教焕", "character": "黄东万", "order": 0},
                    {"name": "高允贞", "character": "边恩雅", "order": 1}
                ]
            })),
            StoredMetadataKind::Series,
            289424,
            None,
            None,
            None,
        )
        .expect("document");
        assert_eq!(document.schema_version, METADATA_SCHEMA_VERSION);
        assert_eq!(document.title, "努力克服自卑的我们");
        assert_eq!(document.year, Some(2026));
        assert_eq!(document.genres, vec!["剧情"]);
        assert_eq!(document.cast.len(), 2);
        assert_eq!(document.cast[0].name, "具教焕");
        assert_eq!(document.cast[0].character, "黄东万");
        assert_eq!(document.creators, vec!["Park Hae-young", "Cha Yeong-hun"]);
        assert_eq!(document.network.as_deref(), Some("JTBC"));
        assert_eq!(document.status.as_deref(), Some("Ended"));
    }

    #[test]
    fn episode_documents_omit_cast() {
        let document = document_from_tmdb(
            &json!({
                "id": 10,
                "name": "第一集",
                "air_date": "2026-04-18",
                "overview": "分集简介"
            }),
            Some(&json!({
                "cast": [{"name": "常规演员", "character": "角色A", "order": 0}],
                "guest_stars": [{"name": "客串演员", "character": "角色B", "order": 0}]
            })),
            StoredMetadataKind::Episode,
            289424,
            Some(289424),
            Some(1),
            Some(1),
        )
        .expect("document");
        assert!(document.cast.is_empty());
        assert!(document.creators.is_empty());
        assert_eq!(document.overview.as_deref(), Some("分集简介"));
    }

    #[test]
    fn movie_collects_directors_from_credits() {
        let document = document_from_tmdb(
            &json!({
                "id": 99,
                "title": "示例电影",
                "release_date": "2024-05-01",
                "overview": "电影简介",
                "genres": [{"name": "剧情"}]
            }),
            Some(&json!({
                "cast": [{"name": "主演", "character": "主角", "order": 0}],
                "crew": [
                    {"name": "导演甲", "job": "Director"},
                    {"name": "摄影", "job": "Director of Photography"}
                ]
            })),
            StoredMetadataKind::Movie,
            99,
            None,
            None,
            None,
        )
        .expect("document");
        assert_eq!(document.creators, vec!["导演甲"]);
        assert_eq!(document.cast[0].name, "主演");
    }

    #[test]
    fn parse_episode_file_name_accepts_standard_season_episode_json() {
        assert_eq!(parse_episode_file_name("S01E01.json"), Some((1, 1)));
        assert_eq!(parse_episode_file_name("S12E08.json"), Some((12, 8)));
        assert!(parse_episode_file_name("series.json").is_none());
    }

    #[test]
    fn episode_index_for_group_reads_season_episode_files() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!("lumina-episode-index-{suffix}"));
        let group_key = "We.Are.All.Trying.Here";
        let dir = store::group_dir(&root, group_key);
        fs::create_dir_all(&dir).expect("mkdir group dir");

        let valid_episode = StoredMetadata {
            schema_version: 1,
            kind: StoredMetadataKind::Episode,
            tmdb_id: 10,
            series_tmdb_id: Some(289424),
            title: "第一集".into(),
            original_title: None,
            overview: Some("分集简介".into()),
            year: None,
            season: Some(1),
            episode: Some(1),
            genres: Vec::new(),
            cast: Vec::new(),
            creators: Vec::new(),
            network: None,
            status: None,
            updated_at_ms: 1,
        };
        store::save_group_json(&root, group_key, "S01E01.json", &valid_episode).expect("write ep1");
        fs::write(dir.join("S01E02.json"), "{not-json").expect("write broken ep2");
        assert!(dir.is_dir(), "group dir missing: {}", dir.display());
        assert!(dir.join("S01E01.json").is_file(), "episode file missing");

        let entries = episode_index_for_group(&root, group_key).expect("episode index");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].season, 1);
        assert_eq!(entries[0].episode, 1);
        assert_eq!(entries[0].title, "第一集");
        assert_eq!(entries[1].title, "S01E02");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn assemble_series_joins_index_files_with_titles() {
        use std::time::{SystemTime, UNIX_EPOCH};

        use crate::library::model::IndexedMediaFile;

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!("lumina-assemble-{suffix}"));
        let group_key = "Show";
        let dir = store::group_dir(&root, group_key);
        std::fs::create_dir_all(&dir).expect("mkdir group dir");
        // Only S01E01 gets a metadata title; S01E02 falls back to `SxxExx`.
        let doc = StoredMetadata {
            schema_version: 1,
            kind: StoredMetadataKind::Episode,
            tmdb_id: 1,
            series_tmdb_id: None,
            title: "开篇".into(),
            original_title: None,
            overview: None,
            year: None,
            season: Some(1),
            episode: Some(1),
            genres: Vec::new(),
            cast: Vec::new(),
            creators: Vec::new(),
            network: None,
            status: None,
            updated_at_ms: 1,
        };
        store::save_group_json(&root, group_key, "S01E01.json", &doc).expect("write ep1");

        let file = |name: &str, season: Option<u32>, episode: Option<u32>| IndexedMediaFile {
            relative_path: format!("Show/{name}"),
            file_name: name.into(),
            size_bytes: 1,
            modified_at_ms: 0,
            group_key: group_key.into(),
            season,
            episode,
        };
        let index = LibraryIndex {
            schema_version: 1,
            root: root.to_string_lossy().into_owned(),
            updated_at_ms: 0,
            files: vec![
                file("S01E02.mkv", Some(1), Some(2)),
                file("S01E01.mkv", Some(1), Some(1)),
                file("extra.mkv", None, None),
                IndexedMediaFile {
                    relative_path: "Other/M01.mkv".into(),
                    group_key: "Other".into(),
                    ..file("M01.mkv", Some(1), Some(1))
                },
            ],
            groups: Vec::new(),
        };
        let shelf = assemble_series(&root, &index, group_key, "剧名".into());
        assert_eq!(shelf.label, "剧名");
        assert_eq!(shelf.group_key, "Show");
        assert_eq!(shelf.episodes.len(), 2);
        assert_eq!(shelf.episodes[0].title, "开篇");
        assert_eq!(shelf.episodes[1].title, "S01E02");
        assert!(shelf.episodes[0].path.ends_with("S01E01.mkv"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_prefers_tmdb_episode_overview_with_wiki_fallback() {
        let group = StoredMetadata {
            schema_version: 1,
            kind: StoredMetadataKind::Series,
            tmdb_id: 1,
            series_tmdb_id: None,
            title: "示例".into(),
            original_title: None,
            overview: Some("短简介".into()),
            year: None,
            season: None,
            episode: None,
            genres: Vec::new(),
            cast: Vec::new(),
            creators: Vec::new(),
            network: None,
            status: None,
            updated_at_ms: 0,
        };
        let item = StoredMetadata {
            schema_version: 1,
            kind: StoredMetadataKind::Episode,
            tmdb_id: 1,
            series_tmdb_id: Some(1),
            title: "第一集".into(),
            original_title: None,
            overview: Some("TMDb 分集简介".into()),
            year: None,
            season: Some(1),
            episode: Some(2),
            genres: Vec::new(),
            cast: Vec::new(),
            creators: Vec::new(),
            network: None,
            status: None,
            updated_at_ms: 0,
        };
        let wiki = WikiMetadata {
            schema_version: 1,
            wikidata_id: None,
            page_lang: "en".into(),
            page_title: "Example".into(),
            page_url: "https://example.org".into(),
            extract: "Synopsis".into(),
            attribution: "内容来自维基百科".into(),
            license: "CC BY-SA 4.0".into(),
            match_info: WikiMatchInfo {
                method: WikiMatchMethod::Wikidata,
                candidates_considered: 1,
            },
            characters: vec![WikiCharacter {
                name: "主角".into(),
                actor: "演员".into(),
                bio: Some("小传".into()),
            }],
            episodes: vec![WikiEpisodeSummary {
                season: 1,
                episode: 2,
                title: Some("Episode 2".into()),
                plot: "Wiki 分集 plot".into(),
            }],
            relationships: None,
            updated_at_ms: 0,
        };

        let merged = merge_media_context(&group, Some(&item), Some(&wiki));
        assert_eq!(merged.synopsis.as_deref(), Some("Synopsis"));
        assert_eq!(merged.episode_overview.as_deref(), Some("TMDb 分集简介"));
        assert_eq!(merged.wiki_episode_plot.as_deref(), Some("Wiki 分集 plot"));
        assert_eq!(merged.characters.as_ref().map(|items| items.len()), Some(1));
    }

    #[test]
    fn resolve_episode_media_file_finds_matching_file() {
        use crate::library::model::IndexedMediaFile;

        let index = LibraryIndex {
            schema_version: 1,
            root: "library".into(),
            updated_at_ms: 0,
            files: vec![
                IndexedMediaFile {
                    relative_path: "Show/S01E01.mkv".into(),
                    file_name: "S01E01.mkv".into(),
                    size_bytes: 1,
                    modified_at_ms: 0,
                    group_key: "Show".into(),
                    season: Some(1),
                    episode: Some(1),
                },
                IndexedMediaFile {
                    relative_path: "Show/S01E02.mkv".into(),
                    file_name: "S01E02.mkv".into(),
                    size_bytes: 1,
                    modified_at_ms: 0,
                    group_key: "Show".into(),
                    season: Some(1),
                    episode: Some(2),
                },
            ],
            groups: Vec::new(),
        };
        let file = resolve_episode_media_file(&index, "Show", 1, 2).expect("episode");
        assert_eq!(file.relative_path, "Show/S01E02.mkv");
        assert!(resolve_episode_media_file(&index, "Show", 9, 9).is_err());
        // Zero season/episode is invalid input (frontend citations guarantee positives).
        let zero = resolve_episode_media_file(&index, "Show", 0, 1).expect_err("zero season");
        assert_eq!(
            zero.code,
            crate::library::error::LibraryErrorCode::InvalidInput
        );
    }
}
