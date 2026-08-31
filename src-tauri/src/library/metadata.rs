//! Durable TMDb metadata documents and current-media context reader.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::library::error::LibraryError;
use crate::library::model::{
    IndexedMediaFile, LibraryIndex, MediaGroup, MetadataMediaType, MetadataWriteResult,
    StoredMetadata, StoredMetadataKind, TmdbConfig,
};
use crate::library::{resolver, store};

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
            let document = document_from_detail(
                &detail,
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
            let document = document_from_detail(
                &detail,
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
                let document = document_from_detail(
                    &detail,
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

fn group_files<'a>(index: &'a LibraryIndex, group: &MediaGroup) -> Vec<&'a IndexedMediaFile> {
    index
        .files
        .iter()
        .filter(|file| file.group_key == group.key)
        .collect()
}

fn document_from_detail(
    value: &Value,
    kind: StoredMetadataKind,
    expected_id: u64,
    series_tmdb_id: Option<u64>,
    season: Option<u32>,
    episode: Option<u32>,
) -> Result<StoredMetadata, LibraryError> {
    let tmdb_id = value
        .get("id")
        .and_then(Value::as_u64)
        .unwrap_or(expected_id);
    let title = value
        .get("title")
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| LibraryError::remote_request_failed(Some("TMDb detail title missing")))?
        .to_string();
    let date = value
        .get("release_date")
        .or_else(|| value.get("first_air_date"))
        .or_else(|| value.get("air_date"))
        .and_then(Value::as_str);
    Ok(StoredMetadata {
        schema_version: 1,
        kind,
        tmdb_id,
        series_tmdb_id,
        title,
        original_title: value
            .get("original_title")
            .or_else(|| value.get("original_name"))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string),
        overview: value
            .get("overview")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string),
        year: date
            .and_then(|text| text.get(0..4))
            .and_then(|year| year.parse().ok()),
        season,
        episode,
        genres: value
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
        updated_at_ms: now_ms(),
    })
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
    fn maps_tmdb_tv_detail_to_series_document() {
        let document = document_from_detail(
            &json!({
                "id": 1,
                "name": "示例剧", "original_name": "Example Show",
                "first_air_date": "2024-01-02", "overview": "简介",
                "genres": [{"name": "剧情"}]
            }),
            StoredMetadataKind::Series,
            1,
            None,
            None,
            None,
        )
        .expect("document");
        assert_eq!(document.title, "示例剧");
        assert_eq!(document.year, Some(2024));
        assert_eq!(document.genres, vec!["剧情"]);
    }
}
