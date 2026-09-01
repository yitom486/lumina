//! Durable TMDb metadata documents and current-media context reader.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::library::error::LibraryError;
use crate::library::wikitext::episode_plot_for;
use crate::library::model::{
    IndexedMediaFile, LibraryIndex, MediaGroup, MediaMetadataContext, MergedMediaContext,
    MetadataCastMember, MetadataMediaType, MetadataWriteResult, StoredMetadata, StoredMetadataKind,
    TmdbConfig, WikiCharacter, WikiEnrichmentCandidate, WikiEnrichmentPreview, WikiEpisodeSummary,
    WikiGroupStatus, WikiMatchInfo, WikiMatchMethod, WikiMetadata, WikiWriteResult,
    METADATA_SCHEMA_VERSION,
};
use crate::library::{resolver, store, wikipedia};

const MAX_OVERVIEW_CAST: usize = 15;
const MAX_EPISODE_CAST: usize = 10;
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
                let credits = resolver::fetch_tmdb_credits(
                    config,
                    tmdb_id,
                    media_type,
                    Some(season),
                    Some(episode),
                )?;
                let document = document_from_tmdb(
                    &detail,
                    Some(&credits),
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
    let relative = media_path
        .strip_prefix(root)
        .map_err(|_error| LibraryError::invalid_input("当前媒体不在已启用目录中"))?;
    let relative = relative.to_string_lossy().replace('\\', "/");
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
                .map(|value| cast_from_credits(value, MAX_OVERVIEW_CAST, false))
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
                .map(|value| cast_from_credits(value, MAX_OVERVIEW_CAST, false))
                .unwrap_or_default();
            let creators = credits
                .map(directors_from_credits)
                .unwrap_or_default();
            (cast, creators, None, None)
        }
        StoredMetadataKind::Episode => {
            let cast = credits
                .map(|value| cast_from_credits(value, MAX_EPISODE_CAST, true))
                .unwrap_or_default();
            (cast, Vec::new(), None, None)
        }
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

fn cast_from_credits(credits: &Value, limit: usize, episode: bool) -> Vec<MetadataCastMember> {
    let array = if episode {
        credits
            .get("guest_stars")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .or_else(|| credits.get("cast").and_then(Value::as_array))
    } else {
        credits.get("cast").and_then(Value::as_array)
    };
    let Some(array) = array else {
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
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
    fn episode_prefers_guest_stars_from_credits() {
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
        assert_eq!(document.cast.len(), 1);
        assert_eq!(document.cast[0].name, "客串演员");
        assert!(document.creators.is_empty());
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
}
