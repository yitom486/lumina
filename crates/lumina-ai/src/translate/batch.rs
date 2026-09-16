use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use lumina_subtitle::error::SubtitleError;
use lumina_subtitle::model::Cue;

use super::context::{contains_case_insensitive, person_name_variants, TranslationGlossaryEntry};
use super::ReportedName;

pub(super) const AGENT_CONCURRENCY: usize = 4;

#[derive(Debug)]
pub(super) struct AttemptTracker {
    job_id: String,
    batch: usize,
    content_attempt: std::sync::atomic::AtomicU32,
}

impl AttemptTracker {
    pub(super) fn new(job_id: &str, batch_idx: usize) -> Self {
        Self {
            job_id: job_id.to_string(),
            batch: batch_idx + 1,
            content_attempt: std::sync::atomic::AtomicU32::new(1),
        }
    }

    pub(super) fn bump_content(&self) -> u32 {
        self.content_attempt.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(super) fn label(&self, transport_attempt: u32) -> String {
        format!(
            "job={} batch={} content_attempt={} transport_attempt={}",
            self.job_id,
            self.batch,
            self.content_attempt.load(Ordering::SeqCst),
            transport_attempt
        )
    }
}

pub(super) struct TranslatedBatchOutput {
    pub(super) indexed: Vec<(u32, String)>,
    pub(super) reported_names: Vec<ReportedName>,
}

pub(super) struct NameMismatch {
    pub(super) index: u32,
    pub(super) source: String,
    pub(super) expected: String,
}

pub(super) fn run_batches_in_order<T, E, F>(count: usize, run_one: F) -> Result<Vec<T>, E>
where
    F: Fn(usize) -> Result<T, E> + Sync + Send,
    T: Send,
    E: Send,
{
    if count == 0 {
        return Ok(Vec::new());
    }
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Result<T, E>>>> = (0..count).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..AGENT_CONCURRENCY.min(count) {
            scope.spawn(|| loop {
                let idx = next.fetch_add(1, Ordering::SeqCst);
                if idx >= count {
                    break;
                }
                let out = run_one(idx);
                if let Ok(mut slot) = slots[idx].lock() {
                    *slot = Some(out);
                }
            });
        }
    });
    let mut ordered = Vec::with_capacity(count);
    let mut first_error = None;
    for slot in &slots {
        let Ok(mut guard) = slot.lock() else {
            continue;
        };
        let Some(out) = guard.take() else {
            continue;
        };
        match out {
            Ok(value) => {
                if first_error.is_none() {
                    ordered.push(value);
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(ordered),
    }
}

const ALIGN_RETRY_NOTE: &str = "Return exactly one text per input cue, using each input index exactly once: no omissions, no duplicates, no extra entries.";

fn alignment_issues(chunk: &[Cue], indexed: &[(u32, String)]) -> (Vec<u32>, Vec<u32>) {
    let mut counts: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::with_capacity(indexed.len());
    for (index, _) in indexed {
        counts
            .entry(*index)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }
    let mut duplicated: Vec<u32> = counts
        .iter()
        .filter_map(
            |(index, count)| {
                if *count > 1 {
                    Some(*index)
                } else {
                    None
                }
            },
        )
        .collect();
    duplicated.sort_unstable();
    let mut missing = Vec::new();
    for cue in chunk {
        if !counts.contains_key(&cue.index) {
            missing.push(cue.index);
        }
    }
    (missing, duplicated)
}

fn format_index_list(indices: &[u32]) -> String {
    indices
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn align_retry_note(chunk_len: usize, missing: &[u32], duplicated: &[u32]) -> String {
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!(
            "缺 cue {}（共缺 {} 条）",
            format_index_list(missing),
            missing.len()
        ));
    }
    if !duplicated.is_empty() {
        parts.push(format!(
            "重复 cue {}（共重复 {} 条）",
            format_index_list(duplicated),
            duplicated.len()
        ));
    }
    let detail = if parts.is_empty() {
        "校验未通过".to_string()
    } else {
        parts.join("；")
    };
    format!(
        "上一批回复未通过校验：{detail}。请补上缺失条目并返回全部 {chunk_len} 条 cue，只返回 JSON。{ALIGN_RETRY_NOTE}"
    )
}

pub(super) fn align_with_one_retry(
    chunk: &[Cue],
    first: Vec<(u32, String)>,
    attempt: &AttemptTracker,
    retry_once: impl FnOnce(String) -> Result<Vec<(u32, String)>, SubtitleError>,
) -> Result<Vec<String>, SubtitleError> {
    let (missing, duplicated) = alignment_issues(chunk, &first);
    match align_batch_texts(chunk, first) {
        Ok(texts) => Ok(texts),
        Err(_) => {
            let content_attempt = attempt.bump_content();
            tracing::warn!(
                task_label = %attempt.label(1),
                content_attempt = content_attempt,
                missing = ?missing,
                duplicated = ?duplicated,
                "subtitle batch align failed, retrying once with missing cues"
            );
            let note = align_retry_note(chunk.len(), &missing, &duplicated);
            align_batch_texts(chunk, retry_once(note)?)
        }
    }
}

pub(super) fn align_batch_texts(
    chunk: &[Cue],
    indexed: Vec<(u32, String)>,
) -> Result<Vec<String>, SubtitleError> {
    let mut by_index = std::collections::HashMap::with_capacity(indexed.len());
    for (index, text) in indexed {
        if by_index.insert(index, text).is_some() {
            return Err(SubtitleError::export_failed(Some(&format!(
                "translated cue {index} duplicated"
            ))));
        }
    }
    chunk
        .iter()
        .map(|cue| {
            by_index.remove(&cue.index).ok_or_else(|| {
                SubtitleError::export_failed(Some(&format!("translated cue {} missing", cue.index)))
            })
        })
        .collect()
}

pub(super) fn glossary_mismatches(
    chunk: &[Cue],
    texts: &[String],
    glossary: &[TranslationGlossaryEntry],
) -> Vec<NameMismatch> {
    const MAX_MISMATCHES: usize = 10;

    chunk
        .iter()
        .zip(texts)
        .filter_map(|(cue, text)| {
            let entry = glossary.iter().find(|entry| {
                !entry.target.trim().is_empty()
                    && person_name_variants(&entry.source)
                        .iter()
                        .any(|variant| contains_case_insensitive(&cue.text, variant))
                    && !text.contains(entry.target.trim())
            })?;
            let source = person_name_variants(&entry.source)
                .into_iter()
                .find(|variant| contains_case_insensitive(&cue.text, variant))?;
            Some(NameMismatch {
                index: cue.index,
                source,
                expected: entry.target.clone(),
            })
        })
        .take(MAX_MISMATCHES)
        .collect()
}
