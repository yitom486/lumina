use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use lumina_subtitle::model::Cue;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationGlossaryEntry {
    pub source: String,
    pub target: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synopsis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_episode_plot: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glossary: Vec<TranslationGlossaryEntry>,
}

/// A person name the model translated that was NOT in the glossary.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportedName {
    pub source: String,
    pub target: String,
}

/// Canonicalize the known romanization variant used by the show's metadata.
pub fn canonicalize_person_name(source: &str) -> String {
    source
        .split_whitespace()
        .map(canonicalize_person_name_token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Return the canonical spelling followed by known source-text aliases.
pub fn person_name_variants(source: &str) -> Vec<String> {
    let canonical = canonicalize_person_name(source);
    let mut variants = vec![canonical.clone()];
    let alias = canonical
        .split_whitespace()
        .map(|token| {
            if token.eq_ignore_ascii_case("Woong") {
                "Ung".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if alias != canonical {
        variants.push(alias);
    }
    let original = source.trim();
    if !original.is_empty() && !variants.iter().any(|value| value == original) {
        variants.push(original.to_string());
    }
    variants
}

fn canonicalize_person_name_token(token: &str) -> String {
    let core = token.trim_matches(|character: char| !character.is_ascii_alphanumeric());
    if core.is_empty() || (!core.eq_ignore_ascii_case("ung") && !core.eq_ignore_ascii_case("woong"))
    {
        return token.to_string();
    }
    let start = token.find(core).unwrap_or(0);
    let end = start + core.len();
    format!("{}Woong{}", &token[..start], &token[end..])
}

pub(super) fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn normalized_glossary(entries: &[TranslationGlossaryEntry]) -> Vec<TranslationGlossaryEntry> {
    let mut normalized = BTreeMap::<String, TranslationGlossaryEntry>::new();
    for entry in entries {
        let source = canonicalize_person_name(&entry.source);
        let target = entry.target.trim();
        if source.trim().is_empty() || target.is_empty() {
            continue;
        }
        let key = source.to_lowercase();
        let candidate = TranslationGlossaryEntry {
            source,
            target: target.to_string(),
            verified: entry.verified,
        };
        match normalized.get(&key) {
            Some(existing) if existing.verified || !candidate.verified => {}
            _ => {
                normalized.insert(key, candidate);
            }
        }
    }
    normalized.into_values().collect()
}

pub(super) fn context_with_glossary_delta(
    context: Option<&TranslationContext>,
    delta: &[TranslationGlossaryEntry],
) -> Option<TranslationContext> {
    if context.is_none() && delta.is_empty() {
        return None;
    }
    let mut merged = context.cloned().unwrap_or_default();
    merged.glossary.extend(delta.iter().cloned());
    Some(merged)
}

pub(super) fn append_glossary_delta(
    delta: &mut Vec<TranslationGlossaryEntry>,
    initial: &[TranslationGlossaryEntry],
    reported: &[ReportedName],
) {
    let mut known = normalized_glossary(initial);
    known.extend(delta.iter().cloned());
    known = normalized_glossary(&known);
    for name in reported {
        let candidate = TranslationGlossaryEntry {
            source: canonicalize_person_name(&name.source),
            target: name.target.trim().to_string(),
            verified: false,
        };
        if candidate.source.is_empty() || candidate.target.is_empty() {
            continue;
        }
        let candidate_key = candidate.source.to_lowercase();
        let already_known = known.iter().any(|entry| {
            entry.source.to_lowercase() == candidate_key && entry.target == candidate.target
        });
        if !already_known {
            known.push(candidate.clone());
            delta.push(candidate);
        }
    }
}

pub(super) fn extract_reported_names(entries: &[Value], cues: &[Cue]) -> Vec<ReportedName> {
    const MAX_REPORTED_PER_BATCH: usize = 20;

    entries
        .iter()
        .filter_map(|entry| {
            let source = entry.get("source")?.as_str()?.trim();
            let target = entry.get("target")?.as_str()?.trim();
            if source.is_empty() || target.is_empty() || source == target {
                return None;
            }
            let mentioned = person_name_variants(source).iter().any(|variant| {
                cues.iter()
                    .any(|cue| contains_case_insensitive(&cue.text, variant))
            });
            if !mentioned {
                return None;
            }
            Some(ReportedName {
                source: canonicalize_person_name(source),
                target: target.to_string(),
            })
        })
        .take(MAX_REPORTED_PER_BATCH)
        .collect()
}

pub(super) fn context_block(
    context: Option<&TranslationContext>,
    for_proofread: bool,
) -> Option<(String, Value)> {
    let context = context?;
    let mut lines = Vec::new();
    if let Some(synopsis) = context
        .synopsis
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        lines.push(format!(
            "Series synopsis for context (do not translate it, use it to disambiguate): {synopsis}"
        ));
    }
    for (label, plot) in [
        (
            "Episode plot from metadata",
            context.episode_overview.as_deref(),
        ),
        (
            "Episode plot from Wikipedia",
            context.wiki_episode_plot.as_deref(),
        ),
    ] {
        if let Some(plot) = plot.map(str::trim).filter(|text| !text.is_empty()) {
            lines.push(format!(
                "{label} (do not translate it, use it only to disambiguate the current episode): {plot}"
            ));
        }
    }
    let glossary = normalized_glossary(&context.glossary);
    let pairs: Vec<String> = glossary
        .iter()
        .filter(|entry| !entry.source.trim().is_empty() && !entry.target.trim().is_empty())
        .map(|entry| {
            let tier = if entry.verified { "verified" } else { "auto" };
            let variants = person_name_variants(&entry.source);
            let alias_note = if variants.len() > 1 {
                format!("; source variant: {}", variants[1..].join(", "))
            } else {
                String::new()
            };
            format!(
                "{} -> {} ({tier}{alias_note})",
                variants[0],
                entry.target.trim()
            )
        })
        .collect();
    if !pairs.is_empty() {
        if for_proofread {
            lines.push(format!(
                "Person-name glossary (reference only): keep the listed source forms exactly as written; do not translate or alter person names. {}",
                pairs.join("; ")
            ));
        } else {
            lines.push(format!(
                "Person-name glossary: when a listed source name appears, you MUST use the given translation (verified entries outrank auto ones); names absent from the glossary: keep the original form unchanged in the translated text (do not transliterate), and report your suggested translation under `glossary` as {{\"source\":original,\"target\":translation}} pairs. {}",
                pairs.join("; ")
            ));
        }
    }
    if lines.is_empty() {
        return None;
    }
    let json = json!({
        "synopsis": context.synopsis,
        "episodeOverview": context.episode_overview,
        "wikiEpisodePlot": context.wiki_episode_plot,
        "glossary": glossary,
    });
    Some((lines.join(" "), json))
}
