//! Optional Wikipedia enrichment via Wikidata bridge + title search.
//!
//! No API key required; requests must identify the app (User-Agent).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use url::form_urlencoded;

use crate::error::LibraryError;
use crate::model::{
    GroupResolution, MediaGroup, MetadataMediaType, StoredMetadata, WikiCandidateSource,
    WikiEnrichmentCandidate, WikiEnrichmentPreview, WikiGroupStatus, WikiMatchInfo,
    WikiMatchMethod, WikiMetadata, WikiWriteResult, WikiZhReference, WIKI_METADATA_SCHEMA_VERSION,
    WIKI_STALE_AFTER_MS,
};
use crate::{resolver, store, wikitext};

const USER_AGENT: &str = "Lumina/0.1 (local media reader; Tauri app)";
const PREFERRED_WIKI_LANG: &str = "en";
const ZHWIKI_LANG: &str = "zh";
const SEARCH_LIMIT: usize = 3;
const MAX_ZH_CAST_MEMBERS: usize = 40;

pub fn load_existing_wiki(
    root: &Path,
    group_key: &str,
) -> Result<Option<WikiMetadata>, LibraryError> {
    store::load_group_json(root, group_key, "wiki.json")
}

pub fn is_wiki_stale(updated_at_ms: u128, now_ms: u128) -> bool {
    now_ms.saturating_sub(updated_at_ms) > WIKI_STALE_AFTER_MS
}

pub fn preview_enrichment(
    stored: &StoredMetadata,
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::model::TmdbConfig,
    existing: Option<&WikiMetadata>,
) -> Result<WikiEnrichmentPreview, LibraryError> {
    let wikidata_candidate = try_resolve_wikidata_candidate(tmdb_id, media_type, tmdb);
    let mut search_candidates = search_title_candidates(stored)?;
    if wikidata_candidate.is_none() || search_candidates.is_empty() {
        match search_title_candidates_from_tmdb(tmdb_id, media_type, tmdb) {
            Ok(fallback) => merge_search_candidates(&mut search_candidates, fallback),
            Err(error) => {
                tracing::warn!(
                    details = ?error.details,
                    tmdb_id,
                    ?media_type,
                    "tmdb-id wikipedia fallback skipped"
                );
            }
        }
    }
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

pub fn refresh_existing_page(
    root: &Path,
    group_key: &str,
) -> Result<WikiWriteResult, LibraryError> {
    let Some(existing) = load_existing_wiki(root, group_key)? else {
        return Err(LibraryError::invalid_input(
            "尚未补充维基百科，请先选择英文页面",
        ));
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
    // Round 1 (parallel): the main summary — fatal, the whole write needs
    // it — plus the zh sitelink title, which only feeds the best-effort
    // sidecar below.
    let (summary, zh_title) = fetch_summary_and_zh_title(candidate)?;
    // Round 2 (parallel): every remaining fetch is best-effort; a missing
    // piece degrades to a summary-only (or sidecar-less) document.
    // Note the zh summary is deliberately NOT fetched here: the write path
    // only needs the zh title, wikitext, and qid (the preview path is what
    // shows the zh extract).
    let details = fetch_detail_texts(candidate, summary.title.as_str(), zh_title.as_deref());
    let mut characters = Vec::new();
    let mut episodes = Vec::new();
    if candidate.page_lang == PREFERRED_WIKI_LANG {
        match details.main_wikitext {
            Some(wikitext) => {
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
            None => {
                tracing::warn!(
                    page = %summary.title,
                    "wikipedia wikitext fetch skipped; keeping summary only"
                );
            }
        }
    } else if candidate.page_lang == ZHWIKI_LANG {
        // A zh main page carries its own cast triples; no sidecar needed.
        match details.main_wikitext {
            Some(wikitext) => {
                characters = wikitext::zh_cast_from_wikitext(&wikitext);
                tracing::info!(
                    characters = characters.len(),
                    page = %summary.title,
                    "parsed zh wikipedia cast content"
                );
            }
            None => {
                tracing::warn!(
                    page = %summary.title,
                    "zh wikipedia wikitext fetch skipped; keeping summary only"
                );
            }
        }
    }
    // The en detail document travels with the aligned zh article's cast
    // triples (Chinese names + bios).
    let (zh_cast, zh_page_url) = match (
        zh_title.as_deref(),
        details.zh_qid.as_deref(),
        details.zh_wikitext.as_deref(),
    ) {
        (Some(title), Some(qid), Some(wikitext))
            if candidate.page_lang == PREFERRED_WIKI_LANG
                && qid == normalize_wikidata_id(candidate.wikidata_id.as_deref().unwrap_or("")) =>
        {
            let mut cast = wikitext::zh_cast_from_wikitext(wikitext);
            cast.truncate(MAX_ZH_CAST_MEMBERS);
            if cast.is_empty() {
                (Vec::new(), None)
            } else {
                tracing::info!(
                    members = cast.len(),
                    page = %title,
                    "zh cast sidecar attached"
                );
                (cast, Some(wiki_page_url(ZHWIKI_LANG, title)))
            }
        }
        _ => (Vec::new(), None),
    };
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
        zh_cast,
        zh_page_url,
        updated_at_ms: now_ms(),
    };
    let path = store::save_group_json(root, group_key, "wiki.json", &document)?;
    Ok(WikiWriteResult {
        root: root.to_string_lossy().to_string(),
        group_key: group_key.to_string(),
        written_file: crate::paths::display_relative_path(root, &path),
    })
}

/// Main summary (fatal) plus zh sitelink title (best-effort), fetched in
/// parallel: neither depends on the other.
fn fetch_summary_and_zh_title(
    candidate: &WikiEnrichmentCandidate,
) -> Result<(PageSummary, Option<String>), LibraryError> {
    std::thread::scope(
        |scope| -> Result<(PageSummary, Option<String>), LibraryError> {
            let summary_handle =
                scope.spawn(|| fetch_page_summary(&candidate.page_lang, &candidate.page_title));
            let zh_title_handle = scope.spawn(|| match candidate.wikidata_id.as_deref() {
                Some(qid) if candidate.page_lang == PREFERRED_WIKI_LANG => {
                    Ok(fetch_wikidata_sitelink(qid, ZHWIKI_LANG).ok().flatten())
                }
                _ => Ok(None),
            });
            let summary = summary_handle
                .join()
                .map_err(|_| LibraryError::internal(Some("wiki fetch thread panicked")))??;
            let zh_title = zh_title_handle
                .join()
                .map_err(|_| LibraryError::internal(Some("wiki fetch thread panicked")))??;
            Ok((summary, zh_title))
        },
    )
}

/// Best-effort detail texts, fetched in parallel. Join failures and request
/// failures both degrade to `None` with a warning, never failing the write.
struct WikiDetailTexts {
    main_wikitext: Option<String>,
    zh_wikitext: Option<String>,
    zh_qid: Option<String>,
}

fn fetch_detail_texts(
    candidate: &WikiEnrichmentCandidate,
    summary_title: &str,
    zh_title: Option<&str>,
) -> WikiDetailTexts {
    std::thread::scope(|scope| {
        let main_handle = scope.spawn(|| {
            if candidate.page_lang == PREFERRED_WIKI_LANG || candidate.page_lang == ZHWIKI_LANG {
                fetch_page_wikitext(&candidate.page_lang, summary_title).map(Some)
            } else {
                Ok(None)
            }
        });
        let zh_wikitext_handle = scope.spawn(|| match zh_title {
            Some(title) if candidate.page_lang == PREFERRED_WIKI_LANG => {
                fetch_page_wikitext(ZHWIKI_LANG, title).map(Some)
            }
            _ => Ok(None),
        });
        let zh_qid_handle = scope.spawn(|| match zh_title {
            Some(title) if candidate.page_lang == PREFERRED_WIKI_LANG => {
                Ok(lookup_page_wikidata_id(ZHWIKI_LANG, title).ok().flatten())
            }
            _ => Ok(None),
        });
        WikiDetailTexts {
            main_wikitext: quiet_fetch(summary_title, main_handle.join()),
            zh_wikitext: quiet_fetch("zh-cast", zh_wikitext_handle.join()),
            zh_qid: quiet_fetch("zh-qid", zh_qid_handle.join()),
        }
    })
}

/// Unwrap a best-effort fetch: join panics and request failures both degrade
/// to `None` with a warning, never failing the write.
fn quiet_fetch<T>(
    page: &str,
    result: std::thread::Result<Result<Option<T>, LibraryError>>,
) -> Option<T> {
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            tracing::warn!(details = ?error.details, page, "wikipedia detail fetch skipped");
            None
        }
        Err(_) => {
            tracing::warn!(page, "wikipedia detail fetch thread panicked");
            None
        }
    }
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

fn try_resolve_wikidata_candidate(
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::model::TmdbConfig,
) -> Option<WikiEnrichmentCandidate> {
    if let Some(candidate) = try_bridge_via_tmdb_external_ids(tmdb_id, media_type, tmdb) {
        return Some(candidate);
    }
    try_bridge_via_wikidata_tmdb_id(tmdb_id, media_type)
}

fn try_bridge_via_tmdb_external_ids(
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::model::TmdbConfig,
) -> Option<WikiEnrichmentCandidate> {
    let wikidata_id = resolver::fetch_tmdb_external_ids(tmdb, tmdb_id, media_type)
        .ok()
        .flatten()?;
    candidate_from_wikidata_id(&wikidata_id, WikiCandidateSource::Wikidata)
}

fn try_bridge_via_wikidata_tmdb_id(
    tmdb_id: u64,
    media_type: MetadataMediaType,
) -> Option<WikiEnrichmentCandidate> {
    let wikidata_id = lookup_wikidata_id_by_tmdb_id(tmdb_id, media_type)
        .ok()
        .flatten()?;
    tracing::info!(
        tmdb_id,
        ?media_type,
        wikidata_id = %wikidata_id,
        "resolved wikipedia candidate via wikidata tmdb-id lookup"
    );
    candidate_from_wikidata_id(&wikidata_id, WikiCandidateSource::Wikidata)
}

fn candidate_from_wikidata_id(
    wikidata_id: &str,
    source: WikiCandidateSource,
) -> Option<WikiEnrichmentCandidate> {
    let normalized = normalize_wikidata_id(wikidata_id);
    if normalized.is_empty() {
        return None;
    }
    let page_title = fetch_wikidata_sitelink(&normalized, PREFERRED_WIKI_LANG)
        .ok()
        .flatten()?;
    let summary = fetch_page_summary(PREFERRED_WIKI_LANG, &page_title).ok()?;
    Some(WikiEnrichmentCandidate {
        page_lang: PREFERRED_WIKI_LANG.into(),
        page_title: summary.title,
        page_url: summary.page_url,
        wikidata_id: Some(normalized),
        extract: Some(summary.extract),
        source,
    })
}

fn search_title_candidates_from_tmdb(
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::model::TmdbConfig,
) -> Result<Vec<WikiEnrichmentCandidate>, LibraryError> {
    let queries = tmdb_title_queries(tmdb_id, media_type, tmdb)?;
    if queries.is_empty() {
        return Ok(Vec::new());
    }
    let mut seen_titles = std::collections::BTreeSet::new();
    let mut candidates = Vec::new();
    for query in queries {
        let hits = search_wikipedia(PREFERRED_WIKI_LANG, &query).unwrap_or_default();
        for hit in hits {
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

fn tmdb_title_queries(
    tmdb_id: u64,
    media_type: MetadataMediaType,
    tmdb: &crate::model::TmdbConfig,
) -> Result<Vec<String>, LibraryError> {
    let detail = resolver::fetch_tmdb_details(tmdb, tmdb_id, media_type, None, None)?;
    let mut queries = titles_from_tmdb_detail(&detail, media_type);
    if !tmdb.language.starts_with("en") {
        if let Ok(en_detail) = resolver::fetch_tmdb_details_with_language(
            tmdb, tmdb_id, media_type, None, None, "en-US",
        ) {
            for title in titles_from_tmdb_detail(&en_detail, media_type) {
                push_unique_query(&mut queries, title);
            }
        }
    }
    Ok(queries)
}

fn titles_from_tmdb_detail(detail: &Value, media_type: MetadataMediaType) -> Vec<String> {
    let mut queries = Vec::new();
    match media_type {
        MetadataMediaType::Tv => {
            push_optional_query(&mut queries, detail.get("name").and_then(Value::as_str));
            push_optional_query(
                &mut queries,
                detail.get("original_name").and_then(Value::as_str),
            );
        }
        MetadataMediaType::Movie => {
            push_optional_query(&mut queries, detail.get("title").and_then(Value::as_str));
            push_optional_query(
                &mut queries,
                detail.get("original_title").and_then(Value::as_str),
            );
        }
    }
    if let Some(alternatives) = detail
        .get("alternative_titles")
        .and_then(|value| value.get("titles"))
        .and_then(Value::as_array)
    {
        for item in alternatives {
            let iso = item
                .get("iso_3166_1")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if iso.eq_ignore_ascii_case("US") || iso.eq_ignore_ascii_case("GB") {
                push_optional_query(&mut queries, Some(title));
            }
        }
    }
    queries
}

fn push_optional_query(queries: &mut Vec<String>, value: Option<&str>) {
    if let Some(text) = value {
        push_unique_query(queries, text.to_string());
    }
}

fn push_unique_query(queries: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if queries.iter().any(|query| query == trimmed) {
        return;
    }
    queries.push(trimmed.to_string());
}

fn merge_search_candidates(
    target: &mut Vec<WikiEnrichmentCandidate>,
    incoming: Vec<WikiEnrichmentCandidate>,
) {
    let mut seen: std::collections::BTreeSet<String> =
        target.iter().map(|item| item.page_title.clone()).collect();
    for candidate in incoming {
        if !seen.insert(candidate.page_title.clone()) {
            continue;
        }
        target.push(candidate);
        if target.len() >= SEARCH_LIMIT {
            break;
        }
    }
}

fn lookup_wikidata_id_by_tmdb_id(
    tmdb_id: u64,
    media_type: MetadataMediaType,
) -> Result<Option<String>, LibraryError> {
    let query = wikidata_sparql_for_tmdb_id(tmdb_id, media_type);
    let encoded = form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
    let endpoint = format!("https://query.wikidata.org/sparql?format=json&query={encoded}");
    let payload: Value = get_json(&endpoint, "wikidata sparql")?;
    let item = payload
        .get("results")
        .and_then(|results| results.get("bindings"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("item"))
        .and_then(|item| item.get("value"))
        .and_then(Value::as_str)
        .and_then(wikidata_entity_id_from_iri);
    Ok(item.as_deref().map(normalize_wikidata_id))
}

fn wikidata_sparql_for_tmdb_id(tmdb_id: u64, media_type: MetadataMediaType) -> String {
    match media_type {
        MetadataMediaType::Movie => format!(
            "SELECT ?item WHERE {{ ?item wdt:P4947 \"{tmdb_id}\" . }} LIMIT 1"
        ),
        MetadataMediaType::Tv => format!(
            "SELECT ?item WHERE {{ {{ ?item wdt:P4983 \"{tmdb_id}\" . }} UNION {{ ?item wdt:P11408 \"{tmdb_id}\" . }} }} LIMIT 1"
        ),
    }
}

fn wikidata_entity_id_from_iri(iri: &str) -> Option<String> {
    iri.rsplit('/')
        .next()
        .filter(|segment| segment.starts_with('Q'))
        .map(str::to_string)
}

fn search_title_candidates(
    stored: &StoredMetadata,
) -> Result<Vec<WikiEnrichmentCandidate>, LibraryError> {
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
    let page_title = fetch_wikidata_sitelink(&normalized, ZHWIKI_LANG)
        .ok()
        .flatten()?;
    // Summary and qid only need the title: fetch them in parallel.
    let (summary, zh_qid) = std::thread::scope(|scope| {
        let summary_handle = scope.spawn(|| fetch_page_summary(ZHWIKI_LANG, &page_title).ok());
        let qid_handle = scope.spawn(|| {
            lookup_page_wikidata_id(ZHWIKI_LANG, &page_title)
                .ok()
                .flatten()
        });
        (
            summary_handle.join().ok().flatten(),
            qid_handle.join().ok().flatten(),
        )
    });
    let summary = summary?;
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
    // `props=sitelinks` answers in ~2KB instead of the ~29KB full entity and
    // carries the same `entities.Q.sitelinks.{lang}wiki.title` shape.
    let normalized = normalize_wikidata_id(wikidata_id);
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("action", "wbgetentities")
        .append_pair("ids", &normalized)
        .append_pair("props", "sitelinks")
        .append_pair("format", "json")
        .append_pair("origin", "*")
        .finish();
    let endpoint = format!("https://www.wikidata.org/w/api.php?{query_string}");
    let payload: Value = get_json(&endpoint, "wikidata entity")?;
    Ok(sitelink_from_wikidata_payload(&payload, &normalized, lang))
}

/// Pure sitelink extraction, covered offline with a real slim response.
fn sitelink_from_wikidata_payload(payload: &Value, qid: &str, lang: &str) -> Option<String> {
    payload
        .get("entities")
        .and_then(|entities| entities.get(qid))
        .and_then(|entity| entity.get("sitelinks"))
        .and_then(|links| links.get(format!("{lang}wiki")))
        .and_then(|link| link.get("title"))
        .and_then(Value::as_str)
        .map(str::to_string)
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
    let payload: Value = get_json(&endpoint, "wikipedia search")?;
    let Some(results) = payload
        .get("query")
        .and_then(|query| query.get("search"))
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };

    let titles: Vec<String> = results
        .iter()
        .filter_map(|hit| hit.get("title").and_then(Value::as_str).map(str::to_string))
        .collect();
    // Hits are independent: enrich them in parallel (bounded), joining in
    // order so recommendation and first-pick order never change. Per-hit
    // failures stay tolerant, exactly as before. Kept at 3, not higher:
    // measured 6-wide bursts self-trigger 429 storms on the anon API
    // (backoff then eats the gain), and a throttled UA punishes later clicks.
    const HIT_CONCURRENCY: usize = 3;
    let mut enriched = Vec::with_capacity(titles.len());
    for chunk in titles.chunks(HIT_CONCURRENCY) {
        std::thread::scope(|scope| -> Result<(), LibraryError> {
            let handles: Vec<_> = chunk
                .iter()
                .map(|title| {
                    scope.spawn(move || {
                        let wikidata_id = lookup_page_wikidata_id(lang, title).ok().flatten();
                        let summary = fetch_page_summary(lang, title).ok();
                        (wikidata_id, summary)
                    })
                })
                .collect();
            for handle in handles {
                let pair = handle
                    .join()
                    .map_err(|_| LibraryError::internal(Some("wiki search thread panicked")))?;
                enriched.push(pair);
            }
            Ok(())
        })?;
    }
    Ok(titles
        .into_iter()
        .zip(enriched)
        .map(|(title, (wikidata_id, summary))| {
            let page_url = wiki_page_url(lang, &title);
            WikiEnrichmentCandidate {
                page_lang: lang.to_string(),
                page_title: title,
                page_url,
                wikidata_id,
                extract: summary.map(|item| item.extract),
                source: WikiCandidateSource::Search,
            }
        })
        .collect())
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
    let payload: Value = get_json(&endpoint, "wikipedia pageprops")?;
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
    let payload: Value = get_json(&endpoint, "wikipedia wikitext")?;
    payload
        .get("parse")
        .and_then(|parse| parse.get("wikitext"))
        .and_then(|wikitext| wikitext.get("*"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            LibraryError::wikipedia_summary_unavailable(Some("wikipedia wikitext missing"))
        })
}

fn fetch_page_summary(lang: &str, title: &str) -> Result<PageSummary, LibraryError> {
    match fetch_page_summary_rest(lang, title) {
        Ok(summary) => Ok(summary),
        Err(error) if is_wikipedia_not_found(&error) => fetch_page_summary_query(lang, title),
        Err(error) => Err(error),
    }
}

fn fetch_page_summary_rest(lang: &str, title: &str) -> Result<PageSummary, LibraryError> {
    let encoded = encode_rest_title(title);
    let endpoint = format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{encoded}");
    let payload: Value = get_json(&endpoint, "wikipedia summary")?;
    page_summary_from_rest_payload(lang, title, &payload)
}

fn fetch_page_summary_query(lang: &str, title: &str) -> Result<PageSummary, LibraryError> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("action", "query")
        .append_pair("prop", "extracts")
        .append_pair("exintro", "1")
        .append_pair("explaintext", "1")
        .append_pair("redirects", "1")
        .append_pair("titles", title)
        .append_pair("format", "json")
        .append_pair("origin", "*")
        .finish();
    let endpoint = format!("https://{lang}.wikipedia.org/w/api.php?{query_string}");
    let payload: Value = get_json(&endpoint, "wikipedia extract")?;
    let pages = payload
        .get("query")
        .and_then(|query| query.get("pages"))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LibraryError::wikipedia_page_not_found(Some("wikipedia extract pages missing"))
        })?;
    for page in pages.values() {
        if page.get("missing").is_some() {
            continue;
        }
        let resolved_title = page
            .get("title")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(title)
            .to_string();
        let extract = page
            .get("extract")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if extract.is_empty() {
            continue;
        }
        return Ok(PageSummary {
            title: resolved_title.clone(),
            extract,
            page_url: wiki_page_url(lang, &resolved_title),
        });
    }
    Err(LibraryError::wikipedia_page_not_found(Some(
        "wikipedia extract page missing",
    )))
}

fn page_summary_from_rest_payload(
    lang: &str,
    fallback_title: &str,
    payload: &Value,
) -> Result<PageSummary, LibraryError> {
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            LibraryError::wikipedia_summary_unavailable(Some("wikipedia summary title missing"))
        })?
        .to_string();
    let extract = payload
        .get("extract")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if extract.is_empty() {
        return Err(LibraryError::wikipedia_summary_unavailable(Some(
            "wikipedia summary extract missing",
        )));
    }
    let page_url = payload
        .get("content_urls")
        .and_then(|urls| urls.get("desktop"))
        .and_then(|desktop| desktop.get("page"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| wiki_page_url(lang, fallback_title));
    Ok(PageSummary {
        title,
        extract,
        page_url,
    })
}

fn is_wikipedia_not_found(error: &LibraryError) -> bool {
    matches!(
        error.message.as_str(),
        "未找到对应的英文维基页面，请尝试重新选择"
    ) || error
        .details
        .as_deref()
        .is_some_and(|details| details.contains("http status: 404"))
}

fn get_json(endpoint: &str, context: &str) -> Result<Value, LibraryError> {
    // Wikipedia answers bursts with 429; back off and retry instead of
    // failing the enrichment (the exhausted error already says "try later").
    // Fixed delays (ureq surfaces only the status, not Retry-After).
    const MAX_ATTEMPTS: u32 = 3;
    const RETRY_BASE_SECS: u64 = 2;

    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = fetch_json_once(endpoint, context);
        match result {
            Ok(payload) => return Ok(payload),
            Err(error) if attempt < MAX_ATTEMPTS && is_http_429(&error) => {
                let wait_secs = RETRY_BASE_SECS * u64::from(attempt);
                tracing::info!(
                    context,
                    attempt,
                    wait_secs,
                    "wikipedia throttled; backing off"
                );
                std::thread::sleep(std::time::Duration::from_secs(wait_secs));
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_http_429(error: &LibraryError) -> bool {
    error
        .details
        .as_deref()
        .is_some_and(|details| details.contains("http status: 429"))
}

fn fetch_json_once(endpoint: &str, context: &str) -> Result<Value, LibraryError> {
    let mut response = ureq::get(endpoint)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()
        .map_err(|error| map_http_error(context, endpoint, &error))?;
    response
        .body_mut()
        .read_json()
        .map_err(|error| map_http_error(context, endpoint, &error))
}

fn map_http_error(context: &str, endpoint: &str, error: &impl std::fmt::Display) -> LibraryError {
    let details = format!("{context}: {error} url={endpoint}");
    let detail_text = details.as_str();
    if detail_text.contains("http status: 404") {
        if context.starts_with("wikidata") {
            return LibraryError::wikipedia_bridge_failed(Some(detail_text));
        }
        return LibraryError::wikipedia_page_not_found(Some(detail_text));
    }
    if detail_text.contains("http status: 0")
        || detail_text.contains("Connection refused")
        || detail_text.contains("timed out")
        || detail_text.contains("dns")
    {
        return LibraryError::wikipedia_unavailable(Some(detail_text));
    }
    LibraryError::remote_request_failed(Some(detail_text))
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
        let wikidata = candidate(
            "Our Beloved Summer",
            "Q110123456",
            WikiCandidateSource::Wikidata,
        );
        let search = vec![candidate(
            "Our Beloved Summer",
            "Q110123456",
            WikiCandidateSource::Search,
        )];
        let aligned = align_candidates(Some(&wikidata), &search);
        assert!(!aligned.needs_user_pick);
        assert!(!aligned.conflict);
        assert_eq!(
            aligned
                .recommended
                .as_ref()
                .map(|item| item.page_title.as_str()),
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
    fn throttled_responses_are_detected_for_backoff() {
        let throttled =
            LibraryError::remote_request_failed(Some("wikipedia search: http status: 429"));
        assert!(is_http_429(&throttled));
        let other = LibraryError::remote_request_failed(Some("wikipedia search: http status: 500"));
        assert!(!is_http_429(&other));
        assert!(!is_http_429(&LibraryError::remote_request_failed(None)));
    }

    #[test]
    fn slim_sitelink_payload_resolves_titles() {
        // Shape of `wbgetentities?props=sitelinks` (live Q107474096, trimmed).
        let payload = serde_json::json!({
            "entities": {
                "Q107474096": {
                    "type": "item",
                    "id": "Q107474096",
                    "sitelinks": {
                        "enwiki": {"site": "enwiki", "title": "Our Beloved Summer"},
                        "zhwiki": {"site": "zhwiki", "title": "那年，我们的夏天"}
                    }
                }
            },
            "success": 1
        });
        assert_eq!(
            sitelink_from_wikidata_payload(&payload, "Q107474096", "zh").as_deref(),
            Some("那年，我们的夏天")
        );
        assert_eq!(
            sitelink_from_wikidata_payload(&payload, "Q107474096", "en").as_deref(),
            Some("Our Beloved Summer")
        );
        assert_eq!(
            sitelink_from_wikidata_payload(&payload, "Q107474096", "ko"),
            None
        );
    }

    #[test]
    fn stale_after_threshold() {
        let now = WIKI_STALE_AFTER_MS + 1_000;
        assert!(!is_wiki_stale(now - WIKI_STALE_AFTER_MS, now));
        assert!(is_wiki_stale(now - WIKI_STALE_AFTER_MS - 1, now));
    }

    #[test]
    fn maps_wikipedia_404_to_page_not_found_message() {
        let error = map_http_error(
            "wikipedia summary",
            "https://en.wikipedia.org/api/rest_v1/page/summary/Foo",
            &"http status: 404",
        );
        assert_eq!(error.message, "未找到对应的英文维基页面，请尝试重新选择");
    }

    #[test]
    fn extracts_tmdb_tv_titles_for_fallback_search() {
        let detail = serde_json::json!({
            "name": "모두가 자신의 무가치함과 싸우고",
            "original_name": "We Are All Trying Here"
        });
        let titles = titles_from_tmdb_detail(&detail, MetadataMediaType::Tv);
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"We Are All Trying Here".to_string()));
    }

    #[test]
    fn parses_wikidata_entity_id_from_sparql_iri() {
        assert_eq!(
            wikidata_entity_id_from_iri("http://www.wikidata.org/entity/Q137843064"),
            Some("Q137843064".to_string())
        );
    }
}
