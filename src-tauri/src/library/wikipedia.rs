//! Optional Wikipedia enrichment via Wikidata bridge + title search.
//!
//! No API key required; requests must identify the app (User-Agent).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use url::form_urlencoded;

use crate::library::error::LibraryError;
use crate::library::model::{
    GroupResolution, MetadataMediaType, StoredMetadata, WikiCandidateSource,
    WikiEnrichmentCandidate, WikiEnrichmentPreview, WikiMatchInfo, WikiMatchMethod, WikiMetadata,
    MediaGroup, WikiWriteResult, WikiGroupStatus, WikiZhReference, WIKI_METADATA_SCHEMA_VERSION,
    WIKI_STALE_AFTER_MS,
};
use crate::library::{resolver, store, wikitext};

const USER_AGENT: &str = "Lumina/0.1 (local media reader; Tauri app)";
const PREFERRED_WIKI_LANG: &str = "en";
const ZHWIKI_LANG: &str = "zh";
const SEARCH_LIMIT: usize = 3;

pub fn load_existing_wiki(root: &Path, group_key: &str) -> Result<Option<WikiMetadata>, LibraryError> {
    store::load_group_json(root, group_key, "wiki.json")
}

pub fn is_wiki_stale(updated_at_ms: u128, now_ms: u128) -> bool {
    now_ms.saturating_sub(updated_at_ms) > WIKI_STALE_AFTER_MS
}

pub fn preview_enrichment(
    stored: &StoredMetadata,
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::library::model::TmdbConfig,
    existing: Option<&WikiMetadata>,
) -> Result<WikiEnrichmentPreview, LibraryError> {
    let wikidata_candidate = resolve_wikidata_candidate(tmdb_id, media_type, tmdb)?;
    let search_candidates = search_title_candidates(stored)?;
    let aligned = align_candidates(wikidata_candidate.as_ref(), &search_candidates);
    let now = now_ms();
    let is_stale = existing
        .map(|wiki| is_wiki_stale(wiki.updated_at_ms, now))
        .unwrap_or(false);
    let wikidata_id = wikidata_candidate
        .as_ref()
        .and_then(|item| item.wikidata_id.as_deref())
        .or_else(|| existing.and_then(|wiki| wiki.wikidata_id.as_deref()));
    let zhwiki_reference = wikidata_id.and_then(fetch_zhwiki_reference);
    Ok(WikiEnrichmentPreview {
        wikidata_candidate,
        search_candidates,
        recommended: aligned.recommended,
        needs_user_pick: aligned.needs_user_pick,
        conflict: aligned.conflict,
        existing: existing.cloned(),
        is_stale,
        stale_after_ms: WIKI_STALE_AFTER_MS,
        zhwiki_reference,
    })
}

pub fn refresh_existing_page(root: &Path, group_key: &str) -> Result<WikiWriteResult, LibraryError> {
    let Some(existing) = load_existing_wiki(root, group_key)? else {
        return Err(LibraryError::invalid_input("尚未补充维基百科，请先选择英文页面"));
    };
    let candidate = WikiEnrichmentCandidate {
        page_lang: existing.page_lang.clone(),
        page_title: existing.page_title.clone(),
        page_url: existing.page_url.clone(),
        wikidata_id: existing.wikidata_id.clone(),
        extract: None,
        source: WikiCandidateSource::Wikidata,
    };
    write_selected_page(
        root,
        group_key,
        &candidate,
        existing.match_info.method,
        existing.match_info.candidates_considered,
    )
}

pub fn statuses_for_matched_groups(
    root: &Path,
    groups: &[MediaGroup],
) -> Result<Vec<WikiGroupStatus>, LibraryError> {
    let now = now_ms();
    Ok(groups
        .iter()
        .filter(|group| matches!(group.resolution, GroupResolution::Matched { .. }))
        .map(|group| {
            let existing = load_existing_wiki(root, &group.key).ok().flatten();
            let is_stale = existing
                .as_ref()
                .map(|wiki| is_wiki_stale(wiki.updated_at_ms, now))
                .unwrap_or(false);
            WikiGroupStatus {
                group_key: group.key.clone(),
                existing,
                is_stale,
                stale_after_ms: WIKI_STALE_AFTER_MS,
            }
        })
        .collect())
}

pub fn write_selected_page(
    root: &Path,
    group_key: &str,
    candidate: &WikiEnrichmentCandidate,
    match_method: WikiMatchMethod,
    candidates_considered: u32,
) -> Result<WikiWriteResult, LibraryError> {
    let summary = fetch_page_summary(&candidate.page_lang, &candidate.page_title)?;
    let mut characters = Vec::new();
    let mut episodes = Vec::new();
    if candidate.page_lang == PREFERRED_WIKI_LANG {
        match fetch_page_wikitext(&candidate.page_lang, &summary.title) {
            Ok(wikitext) => {
                let structured = wikitext::parse_structured_content(&wikitext);
                characters = structured.characters;
                episodes = structured.episodes;
                tracing::info!(
                    characters = characters.len(),
                    episodes = episodes.len(),
                    page = %summary.title,
                    "parsed wikipedia structured content"
                );
            }
            Err(error) => {
                tracing::warn!(
                    details = ?error.details,
                    page = %summary.title,
                    "wikipedia wikitext fetch skipped; keeping summary only"
                );
            }
        }
    }
    let document = WikiMetadata {
        schema_version: WIKI_METADATA_SCHEMA_VERSION,
        wikidata_id: candidate.wikidata_id.clone(),
        page_lang: candidate.page_lang.clone(),
        page_title: summary.title,
        page_url: summary.page_url,
        extract: summary.extract,
        attribution: "内容来自维基百科".into(),
        license: "CC BY-SA 4.0".into(),
        match_info: WikiMatchInfo {
            method: match_method,
            candidates_considered,
        },
        characters,
        episodes,
        relationships: None,
        updated_at_ms: now_ms(),
    };
    let path = store::save_group_json(root, group_key, "wiki.json", &document)?;
    Ok(WikiWriteResult {
        root: root.to_string_lossy().to_string(),
        group_key: group_key.to_string(),
        written_file: path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/"),
    })
}

pub(crate) struct AlignmentResult {
    recommended: Option<WikiEnrichmentCandidate>,
    needs_user_pick: bool,
    conflict: bool,
}

pub fn align_candidates(
    wikidata_candidate: Option<&WikiEnrichmentCandidate>,
    search_candidates: &[WikiEnrichmentCandidate],
) -> AlignmentResult {
    if wikidata_candidate.is_none() && search_candidates.is_empty() {
        return AlignmentResult {
            recommended: None,
            needs_user_pick: false,
            conflict: false,
        };
    }

    let wikidata_id = wikidata_candidate.and_then(|item| item.wikidata_id.as_deref());
    if let Some(path) = wikidata_candidate {
        if let Some(id) = wikidata_id {
            if search_candidates
                .iter()
                .any(|item| item.wikidata_id.as_deref() == Some(id))
            {
                return AlignmentResult {
                    recommended: Some(path.clone()),
                    needs_user_pick: false,
                    conflict: false,
                };
            }
            if let Some(search) = search_candidates.first() {
                if search.wikidata_id.as_deref() != Some(id) {
                    return AlignmentResult {
                        recommended: Some(path.clone()),
                        needs_user_pick: true,
                        conflict: true,
                    };
                }
            }
        }
        return AlignmentResult {
            recommended: Some(path.clone()),
            needs_user_pick: false,
            conflict: false,
        };
    }

    if search_candidates.len() == 1 {
        return AlignmentResult {
            recommended: Some(search_candidates[0].clone()),
            needs_user_pick: false,
            conflict: false,
        };
    }

    AlignmentResult {
        recommended: search_candidates.first().cloned(),
        needs_user_pick: true,
        conflict: false,
    }
}

fn resolve_wikidata_candidate(
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::library::model::TmdbConfig,
) -> Result<Option<WikiEnrichmentCandidate>, LibraryError> {
    let Some(wikidata_id) = resolver::fetch_tmdb_external_ids(tmdb, tmdb_id, media_type)? else {
        return Ok(None);
    };
    let Some(page_title) = fetch_wikidata_sitelink(&wikidata_id, PREFERRED_WIKI_LANG)? else {
        return Ok(None);
    };
    let summary = fetch_page_summary(PREFERRED_WIKI_LANG, &page_title)?;
    Ok(Some(WikiEnrichmentCandidate {
        page_lang: PREFERRED_WIKI_LANG.into(),
        page_title: summary.title,
        page_url: summary.page_url,
        wikidata_id: Some(normalize_wikidata_id(&wikidata_id)),
        extract: Some(summary.extract),
        source: WikiCandidateSource::Wikidata,
    }))
}

fn search_title_candidates(stored: &StoredMetadata) -> Result<Vec<WikiEnrichmentCandidate>, LibraryError> {
    let mut queries = Vec::new();
    if !stored.title.trim().is_empty() {
        queries.push(stored.title.trim().to_string());
    }
    if let Some(original) = stored.original_title.as_deref() {
        let trimmed = original.trim();
        if !trimmed.is_empty() && !queries.iter().any(|query| query == trimmed) {
            queries.push(trimmed.to_string());
        }
    }

    let mut seen_titles = std::collections::BTreeSet::new();
    let mut candidates = Vec::new();
    for query in queries {
        for hit in search_wikipedia(PREFERRED_WIKI_LANG, &query)? {
            if !seen_titles.insert(hit.page_title.clone()) {
                continue;
            }
            candidates.push(hit);
            if candidates.len() >= SEARCH_LIMIT {
                return Ok(candidates);
            }
        }
    }
    Ok(candidates)
}

fn fetch_zhwiki_reference(wikidata_id: &str) -> Option<WikiZhReference> {
    let normalized = normalize_wikidata_id(wikidata_id);
    if normalized.is_empty() {
        return None;
    }
    let page_title = fetch_wikidata_sitelink(&normalized, ZHWIKI_LANG).ok().flatten()?;
    let summary = fetch_page_summary(ZHWIKI_LANG, &page_title).ok()?;
    let zh_qid = lookup_page_wikidata_id(ZHWIKI_LANG, &page_title)
        .ok()
        .flatten();
    let aligned_with_en = zh_qid
        .as_deref()
        .map(|id| id == normalized)
        .unwrap_or(false);
    Some(WikiZhReference {
        page_lang: ZHWIKI_LANG.into(),
        page_title: summary.title,
        page_url: summary.page_url,
        extract: Some(summary.extract),
        wikidata_id: zh_qid,
        aligned_with_en,
    })
}

fn fetch_wikidata_sitelink(wikidata_id: &str, lang: &str) -> Result<Option<String>, LibraryError> {
    let normalized = normalize_wikidata_id(wikidata_id);
    let endpoint = format!(
        "https://www.wikidata.org/wiki/Special:EntityData/{normalized}.json"
    );
    let payload: Value = get_json(&endpoint)?;
    let title = payload
        .get("entities")
        .and_then(|entities| entities.get(&normalized))
        .and_then(|entity| entity.get("sitelinks"))
        .and_then(|links| links.get(format!("{lang}wiki")))
        .and_then(|link| link.get("title"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(title)
}

fn search_wikipedia(lang: &str, query: &str) -> Result<Vec<WikiEnrichmentCandidate>, LibraryError> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("action", "query")
        .append_pair("list", "search")
        .append_pair("srsearch", query)
        .append_pair("srlimit", "5")
        .append_pair("format", "json")
        .append_pair("origin", "*")
        .finish();
    let endpoint = format!("https://{lang}.wikipedia.org/w/api.php?{query_string}");
    let payload: Value = get_json(&endpoint)?;
    let Some(results) = payload
        .get("query")
        .and_then(|query| query.get("search"))
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };

    let mut candidates = Vec::new();
    for hit in results {
        let Some(title) = hit.get("title").and_then(Value::as_str) else {
            continue;
        };
        let wikidata_id = lookup_page_wikidata_id(lang, title)?;
        let summary = fetch_page_summary(lang, title).ok();
        candidates.push(WikiEnrichmentCandidate {
            page_lang: lang.to_string(),
            page_title: title.to_string(),
            page_url: wiki_page_url(lang, title),
            wikidata_id,
            extract: summary.map(|item| item.extract),
            source: WikiCandidateSource::Search,
        });
    }
    Ok(candidates)
}

fn lookup_page_wikidata_id(lang: &str, title: &str) -> Result<Option<String>, LibraryError> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("action", "query")
        .append_pair("prop", "pageprops")
        .append_pair("ppprop", "wikibase_item")
        .append_pair("titles", title)
        .append_pair("format", "json")
        .append_pair("origin", "*")
        .finish();
    let endpoint = format!("https://{lang}.wikipedia.org/w/api.php?{query_string}");
    let payload: Value = get_json(&endpoint)?;
    let id = payload
        .get("query")
        .and_then(|query| query.get("pages"))
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .find_map(|(_key, page)| {
            page.get("pageprops")
                .and_then(|props| props.get("wikibase_item"))
                .and_then(Value::as_str)
        })
        .map(normalize_wikidata_id);
    Ok(id)
}

struct PageSummary {
    title: String,
    extract: String,
    page_url: String,
}

fn fetch_page_wikitext(lang: &str, title: &str) -> Result<String, LibraryError> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("action", "parse")
        .append_pair("page", title)
        .append_pair("prop", "wikitext")
        .append_pair("format", "json")
        .append_pair("origin", "*")
        .finish();
    let endpoint = format!("https://{lang}.wikipedia.org/w/api.php?{query_string}");
    let payload: Value = get_json(&endpoint)?;
    payload
        .get("parse")
        .and_then(|parse| parse.get("wikitext"))
        .and_then(|wikitext| wikitext.get("*"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| LibraryError::remote_request_failed(Some("Wikipedia wikitext missing")))
}

fn fetch_page_summary(lang: &str, title: &str) -> Result<PageSummary, LibraryError> {
    let encoded = encode_rest_title(title);
    let endpoint = format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{encoded}");
    let payload: Value = get_json(&endpoint)?;
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| LibraryError::remote_request_failed(Some("Wikipedia summary title missing")))?
        .to_string();
    let extract = payload
        .get("extract")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if extract.is_empty() {
        return Err(LibraryError::remote_request_failed(Some(
            "Wikipedia summary extract missing",
        )));
    }
    let page_url = payload
        .get("content_urls")
        .and_then(|urls| urls.get("desktop"))
        .and_then(|desktop| desktop.get("page"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| wiki_page_url(lang, &title));
    Ok(PageSummary {
        title,
        extract,
        page_url,
    })
}

fn get_json(endpoint: &str) -> Result<Value, LibraryError> {
    let mut response = ureq::get(endpoint)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()
        .map_err(|error| {
            LibraryError::remote_request_failed(Some(&format!("Wikipedia request: {error}")))
        })?;
    response.body_mut().read_json().map_err(|error| {
        LibraryError::remote_request_failed(Some(&format!("Wikipedia response: {error}")))
    })
}

fn wiki_page_url(lang: &str, title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|ch| if ch == ' ' { '_' } else { ch })
        .collect();
    format!("https://{lang}.wikipedia.org/wiki/{slug}")
}

fn encode_rest_title(title: &str) -> String {
    form_urlencoded::byte_serialize(title.as_bytes()).collect()
}

pub fn normalize_wikidata_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('Q') || trimmed.starts_with('q') {
        return trimmed.to_uppercase();
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return format!("Q{trimmed}");
    }
    trimmed.to_string()
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

    fn candidate(title: &str, qid: &str, source: WikiCandidateSource) -> WikiEnrichmentCandidate {
        WikiEnrichmentCandidate {
            page_lang: "en".into(),
            page_title: title.into(),
            page_url: format!("https://en.wikipedia.org/wiki/{title}"),
            wikidata_id: Some(qid.into()),
            extract: Some("summary".into()),
            source,
        }
    }

    #[test]
    fn auto_when_wikidata_and_search_share_qid() {
        let wikidata = candidate("Our Beloved Summer", "Q110123456", WikiCandidateSource::Wikidata);
        let search = vec![candidate(
            "Our Beloved Summer",
            "Q110123456",
            WikiCandidateSource::Search,
        )];
        let aligned = align_candidates(Some(&wikidata), &search);
        assert!(!aligned.needs_user_pick);
        assert!(!aligned.conflict);
        assert_eq!(
            aligned.recommended.as_ref().map(|item| item.page_title.as_str()),
            Some("Our Beloved Summer")
        );
    }

    #[test]
    fn conflict_when_qids_differ() {
        let wikidata = candidate("Show A", "Q1", WikiCandidateSource::Wikidata);
        let search = vec![candidate("Show B", "Q2", WikiCandidateSource::Search)];
        let aligned = align_candidates(Some(&wikidata), &search);
        assert!(aligned.needs_user_pick);
        assert!(aligned.conflict);
    }

    #[test]
    fn normalize_wikidata_id_adds_prefix() {
        assert_eq!(normalize_wikidata_id("12345"), "Q12345");
        assert_eq!(normalize_wikidata_id("q99"), "Q99");
    }

    #[test]
    fn stale_after_threshold() {
        let now = WIKI_STALE_AFTER_MS + 1_000;
        assert!(!is_wiki_stale(now - WIKI_STALE_AFTER_MS, now));
        assert!(is_wiki_stale(now - WIKI_STALE_AFTER_MS - 1, now));
    }
}
