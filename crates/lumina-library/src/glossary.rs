//! Per-group person-name glossary backing subtitle translation quality.
//!
//! Tiers: `verified` (wiki-curated or human-approved, always enforced),
//! `auto` (model-suggested, enforced once admitted), `pending` (conflicts or
//! review-mode intake, never enforced), plus invisible `candidates` counts.
//! A pair enters `auto` only after repeated sightings; a conflicting pair
//! always lands in `pending`, even with review mode off. Best-effort by
//! design: translation never fails for glossary IO.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::LibraryError;
use crate::store;

pub const GLOSSARY_FILE_NAME: &str = "glossary.json";
pub const GLOSSARY_SCHEMA_VERSION: u32 = 1;
/// Repeated sightings required before a candidate pair is admitted to `auto`.
pub const GLOSSARY_AUTO_SIGHTINGS: u32 = 2;
const MAX_BUCKET_ENTRIES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryName {
    pub source: String,
    pub target: String,
    pub origin: String,
    #[serde(default)]
    pub sightings: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingGlossaryName {
    pub source: String,
    pub target: String,
    pub origin: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NameGlossary {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub verified: Vec<GlossaryName>,
    #[serde(default)]
    pub auto: Vec<GlossaryName>,
    #[serde(default)]
    pub pending: Vec<PendingGlossaryName>,
    #[serde(default)]
    pub candidates: BTreeMap<String, BTreeMap<String, u32>>,
}

impl NameGlossary {
    fn find_target(&self, source: &str) -> Option<&str> {
        self.verified
            .iter()
            .find(|entry| entry.source == source)
            .map(|entry| entry.target.as_str())
            .or_else(|| {
                self.auto
                    .iter()
                    .find(|entry| entry.source == source)
                    .map(|entry| entry.target.as_str())
            })
    }
}

/// Merge model-reported `(source, target)` pairs. Returns the merged file
/// plus the count of pairs that newly entered `auto` or `pending` (for
/// progress copy). Pure and fully unit-tested; IO lives with the caller.
pub fn record_reported(
    mut current: NameGlossary,
    reported: &[(String, String)],
    origin: &str,
    review_mode: bool,
) -> (NameGlossary, usize) {
    let mut changed = 0;
    current.schema_version = GLOSSARY_SCHEMA_VERSION;
    for (source, target) in reported {
        let source = source.trim();
        let target = target.trim();
        if source.is_empty() || target.is_empty() || source == target {
            continue;
        }
        match current.find_target(source) {
            Some(known) if known == target => {
                if let Some(entry) = current.auto.iter_mut().find(|entry| entry.source == source) {
                    entry.sightings += 1;
                }
            }
            Some(_) => {
                // A second Chinese face for one source name: never overwrite,
                // not even with review mode off.
                let exists = current
                    .pending
                    .iter()
                    .any(|entry| entry.source == source && entry.target == target);
                if !exists && current.pending.len() < MAX_BUCKET_ENTRIES {
                    current.pending.push(PendingGlossaryName {
                        source: source.to_string(),
                        target: target.to_string(),
                        origin: origin.to_string(),
                        reason: "conflict".into(),
                    });
                    changed += 1;
                }
            }
            None => {
                if review_mode {
                    let exists = current
                        .pending
                        .iter()
                        .any(|entry| entry.source == source && entry.target == target);
                    if !exists && current.pending.len() < MAX_BUCKET_ENTRIES {
                        current.pending.push(PendingGlossaryName {
                            source: source.to_string(),
                            target: target.to_string(),
                            origin: origin.to_string(),
                            reason: "review".into(),
                        });
                        changed += 1;
                    }
                    continue;
                }
                if current.candidates.len() >= MAX_BUCKET_ENTRIES {
                    continue;
                }
                let count = current
                    .candidates
                    .entry(source.to_string())
                    .or_default()
                    .entry(target.to_string())
                    .or_insert(0);
                *count += 1;
                if *count >= GLOSSARY_AUTO_SIGHTINGS && current.auto.len() < MAX_BUCKET_ENTRIES {
                    current.auto.push(GlossaryName {
                        source: source.to_string(),
                        target: target.to_string(),
                        origin: origin.to_string(),
                        sightings: *count,
                    });
                    if let Some(targets) = current.candidates.get_mut(source) {
                        targets.remove(target);
                        if targets.is_empty() {
                            current.candidates.remove(source);
                        }
                    }
                    changed += 1;
                }
            }
        }
    }
    (current, changed)
}

pub fn load_glossary(root: &Path, group_key: &str) -> Result<NameGlossary, LibraryError> {
    Ok(store::load_group_json(root, group_key, GLOSSARY_FILE_NAME)?.unwrap_or_default())
}

pub fn save_glossary(
    root: &Path,
    group_key: &str,
    glossary: &NameGlossary,
) -> Result<(), LibraryError> {
    store::save_group_json(root, group_key, GLOSSARY_FILE_NAME, glossary)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(source: &str, target: &str) -> (String, String) {
        (source.into(), target.into())
    }

    #[test]
    fn repeated_sightings_promote_candidates_to_auto() {
        let (file, changed) = record_reported(
            NameGlossary::default(),
            &[pair("Choi Woong", "崔雄")],
            "t1",
            false,
        );
        assert_eq!(changed, 0);
        assert!(file.auto.is_empty());
        let (file, changed) = record_reported(file, &[pair("Choi Woong", "崔雄")], "t2", false);
        assert_eq!(changed, 1);
        assert_eq!(file.auto.len(), 1);
        assert_eq!(file.auto[0].sightings, 2);
        // Third sighting only bumps the counter.
        let (file, changed) = record_reported(file, &[pair("Choi Woong", "崔雄")], "t3", false);
        assert_eq!(changed, 0);
        assert_eq!(file.auto[0].sightings, 3);
    }

    #[test]
    fn conflicting_targets_land_in_pending_even_without_review() {
        let verified = NameGlossary {
            verified: vec![GlossaryName {
                source: "Choi Woong".into(),
                target: "崔雄".into(),
                origin: "wiki".into(),
                sightings: 0,
            }],
            ..Default::default()
        };
        let (file, changed) = record_reported(verified, &[pair("Choi Woong", "崔熊")], "t1", false);
        assert_eq!(changed, 1);
        assert_eq!(file.verified[0].target, "崔雄");
        assert_eq!(file.pending.len(), 1);
        assert_eq!(file.pending[0].reason, "conflict");
    }

    #[test]
    fn review_mode_queues_everything_as_pending() {
        let (file, changed) =
            record_reported(NameGlossary::default(), &[pair("NJ", "恩宰")], "t1", true);
        assert_eq!(changed, 1);
        assert!(file.auto.is_empty());
        assert_eq!(file.pending[0].reason, "review");
    }

    #[test]
    fn junk_pairs_are_ignored() {
        let (file, changed) = record_reported(
            NameGlossary::default(),
            &[pair("", "空"), pair("NJ", "NJ"), pair("  ", "  ")],
            "t1",
            false,
        );
        assert_eq!(changed, 0);
        assert!(file.auto.is_empty());
        assert!(file.pending.is_empty());
    }

    #[test]
    fn glossary_roundtrips_through_json() {
        let (file, _) = record_reported(NameGlossary::default(), &[pair("A", "甲")], "t1", false);
        let text = serde_json::to_string(&file).expect("json");
        let parsed: NameGlossary = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed, file);
    }
}
