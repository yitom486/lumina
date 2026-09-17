use std::collections::BTreeMap;

use lumina_subtitle::error::SubtitleError;
use lumina_subtitle::model::{Cue, Transcript};
use lumina_subtitle::service::SubtitleService;
use lumina_subtitle::write::{export_sidecar_srt, normalize_lang_token};

use lumina_core::{AgentInvoker, BatchCheckpoint, CheckpointBatch};

use super::agent::{map_agent_error, proofread_batch, source_language_name, translate_batch};
use super::batch::{
    align_batch_texts, align_with_one_retry, glossary_mismatches, run_batches_in_order,
    AttemptTracker,
};
use super::context::{append_glossary_delta, context_with_glossary_delta};
use super::{
    normalize_translation_language, CheckpointFactory, ProgressUpdate, ReportedName,
    TranslatedTrack, TranslationContext, TranslationGlossaryEntry, TranslationResult,
    TRANSLATE_BATCH_SIZE,
};

#[allow(clippy::too_many_arguments)]
pub fn translate_and_export_track(
    media_path: &str,
    choice_id: &str,
    target_lang: &str,
    context: Option<&TranslationContext>,
    profile_id: &str,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    invoker: &dyn AgentInvoker,
    mut on_progress: impl FnMut(ProgressUpdate) + Send,
    checkpoint_factory: Option<CheckpointFactory<'_>>,
    job_id: &str,
) -> Result<TranslatedTrack, SubtitleError> {
    let target_lang = normalize_translation_language(target_lang)?;
    let token = normalize_lang_token(&target_lang)?;
    let model_key = format!(
        "{}|{}",
        model_id.unwrap_or("default"),
        reasoning_effort.unwrap_or("default")
    );

    on_progress(ProgressUpdate {
        message: "正在加载源字幕…".into(),
        done: None,
        total: None,
    });
    let source = SubtitleService::load_choice(media_path, choice_id)?;
    let checkpoint = checkpoint_factory.map(|build| build(&source, &token, &model_key));
    let checkpoint_ref = checkpoint.as_deref();
    let translated = translate_cues(
        &source,
        &target_lang,
        context,
        profile_id,
        model_id,
        reasoning_effort,
        invoker,
        &mut on_progress,
        checkpoint_ref,
        job_id,
    )?;
    if let Some(store) = checkpoint_ref {
        if let Err(error) = store.clear() {
            tracing::warn!(%error, "checkpoint clear failed");
        }
    }

    on_progress(ProgressUpdate {
        message: format!("正在保存外挂字幕（{token}）…"),
        done: None,
        total: None,
    });
    let transcript =
        export_sidecar_srt(std::path::Path::new(media_path), &token, &translated.cues)?;
    Ok(TranslatedTrack {
        transcript,
        reported_names: translated.reported_names,
    })
}

/// Translate an already-loaded transcript (timeline preserved). Batches fan
/// out over at most [`AGENT_CONCURRENCY`] isolated sessions and rejoin in
/// input order; the first error wins. The caller owns loading (local
/// sidecar, process cache, …) and exporting, so cached downloads can reuse
/// this without touching beside-media files.
#[allow(clippy::too_many_arguments)]
pub fn translate_cues(
    source: &Transcript,
    target_lang: &str,
    context: Option<&TranslationContext>,
    profile_id: &str,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    invoker: &dyn AgentInvoker,
    on_progress: &mut (impl FnMut(ProgressUpdate) + Send),
    checkpoint: Option<&dyn BatchCheckpoint>,
    job_id: &str,
) -> Result<TranslationResult, SubtitleError> {
    if source.cues.is_empty() {
        return Err(SubtitleError::export_failed(Some(
            "source transcript empty",
        )));
    }
    let target_lang = normalize_translation_language(target_lang)?;
    let source_lang = source_language_name(source);

    let total = source.cues.len().div_ceil(TRANSLATE_BATCH_SIZE);
    let chunks: Vec<&[Cue]> = source.cues.chunks(TRANSLATE_BATCH_SIZE).collect();
    // Resume: previously finished batches replay without LLM calls. The owner
    // keyed them by (media, source, target, model, batch size); a length
    // mismatch re-runs the batch instead of corrupting output.
    let resumed: BTreeMap<usize, CheckpointBatch> = checkpoint
        .map(|store| store.load_completed())
        .unwrap_or_default();
    // Counter starts at zero: replayed batches emit their own completion
    // events, so seeding it with resumed.len() would overshoot the total.
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let progress = std::sync::Mutex::new(on_progress);
    if !resumed.is_empty() {
        if let Ok(mut guard) = progress.lock() {
            guard(ProgressUpdate {
                message: format!("已恢复 {}/{} 批，继续翻译...", resumed.len(), total),
                done: Some(resumed.len()),
                total: Some(total),
            });
        }
    }
    let glossary: &[TranslationGlossaryEntry] = context
        .map(|context| context.glossary.as_slice())
        .unwrap_or(&[]);
    let glossary_delta =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<TranslationGlossaryEntry>::new()));

    let outputs = run_batches_in_order(chunks.len(), |batch_idx| -> Result<_, SubtitleError> {
        let chunk = chunks[batch_idx];
        let attempt = AttemptTracker::new(job_id, batch_idx);
        let batch_delta = glossary_delta
            .lock()
            .map(|delta| delta.clone())
            .unwrap_or_default();
        let batch_context = context_with_glossary_delta(context, &batch_delta);
        let batch_context_ref = batch_context.as_ref();
        let effective_glossary = batch_context_ref
            .map(|context| context.glossary.as_slice())
            .unwrap_or(glossary);
        if let Some(saved) = resumed
            .get(&batch_idx)
            .filter(|saved| saved.texts.len() == chunk.len())
        {
            let reported = saved
                .reported
                .iter()
                .map(|(source, target)| ReportedName {
                    source: source.clone(),
                    target: target.clone(),
                })
                .collect::<Vec<_>>();
            if let Ok(mut delta) = glossary_delta.lock() {
                append_glossary_delta(&mut delta, glossary, &reported);
            }
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if let Ok(mut guard) = progress.lock() {
                guard(ProgressUpdate {
                    message: format!("已完成 {done}/{total} 批"),
                    done: Some(done),
                    total: Some(total),
                });
            }
            return Ok((saved.texts.clone(), reported));
        }
        let mut conversation = invoker
            .open_conversation(Some(attempt.label(1)))
            .map_err(map_agent_error)?;
        let out = translate_batch(
            chunk,
            &source_lang,
            &target_lang,
            batch_context_ref,
            &batch_delta,
            None,
            profile_id,
            model_id,
            reasoning_effort,
            invoker,
            &mut *conversation,
            &attempt,
        )?;
        let mut reported = out.reported_names;
        let mut texts = align_with_one_retry(chunk, out.indexed, &attempt, |note| {
            let retry = translate_batch(
                chunk,
                &source_lang,
                &target_lang,
                batch_context_ref,
                &batch_delta,
                Some(note.as_str()),
                profile_id,
                model_id,
                reasoning_effort,
                invoker,
                &mut *conversation,
                &attempt,
            )?;
            reported.extend(retry.reported_names);
            Ok(retry.indexed)
        })?;
        // One bounded retry when glossary names slip through; the second
        // answer stands, good or bad — no retry loops.
        let missed = glossary_mismatches(chunk, &texts, effective_glossary);
        if !missed.is_empty() {
            let note = format!(
                "Fix these person names (use the exact given forms): {}.",
                missed
                    .iter()
                    .map(|mismatch| format!(
                        "cue {}: {} -> {}",
                        mismatch.index, mismatch.source, mismatch.expected
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            let content_attempt = attempt.bump_content();
            let mismatched_cues: Vec<u32> = missed.iter().map(|mismatch| mismatch.index).collect();
            tracing::warn!(
                task_label = %attempt.label(1),
                content_attempt = content_attempt,
                mismatched_cues = ?mismatched_cues,
                "translation glossary names missing, retrying once"
            );
            let retry = translate_batch(
                chunk,
                &source_lang,
                &target_lang,
                batch_context_ref,
                &batch_delta,
                Some(&note),
                profile_id,
                model_id,
                reasoning_effort,
                invoker,
                &mut *conversation,
                &attempt,
            )?;
            texts = align_batch_texts(chunk, retry.indexed)?;
            reported.extend(retry.reported_names);
        }
        if let Ok(mut delta) = glossary_delta.lock() {
            append_glossary_delta(&mut delta, glossary, &reported);
        }
        // Durable checkpoint: a later run replays this batch instead of
        // re-calling the model. Save failures stay in-memory-only (warned),
        // never fail the job — durability is best-effort, correctness isn't.
        if let Some(store) = checkpoint {
            let batch = CheckpointBatch {
                texts: texts.clone(),
                reported: reported
                    .iter()
                    .map(|name| (name.source.clone(), name.target.clone()))
                    .collect(),
            };
            if let Err(error) = store.save_batch(batch_idx, &batch) {
                tracing::warn!(%error, batch = batch_idx, "checkpoint save failed, continuing in memory");
            }
        }
        let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if let Ok(mut guard) = progress.lock() {
            guard(ProgressUpdate {
                message: format!("已完成 {done}/{total} 批"),
                done: Some(done),
                total: Some(total),
            });
        }
        Ok((texts, reported))
    })?;

    let mut translated = Vec::with_capacity(source.cues.len());
    let mut reported_names = Vec::new();
    for (chunk, (texts, reported)) in chunks.iter().zip(outputs) {
        for (cue, text) in chunk.iter().zip(texts) {
            translated.push(Cue {
                index: cue.index,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                text,
            });
        }
        reported_names.extend(reported);
    }
    // Invariant: threading and alignment lose no cue, ever.
    if translated.len() != source.cues.len() {
        return Err(SubtitleError::export_failed(Some(
            "translated cue count mismatch after join",
        )));
    }
    Ok(TranslationResult {
        cues: translated,
        reported_names,
    })
}

/// Proofread source-language cues without translating: fix typos, OCR
/// artifacts and misheard words, normalize punctuation. Timeline, count and
/// order are preserved exactly like translation; the glossary pins names.
/// Deterministic `[sound tag]` stripping is the caller's switch.
#[allow(clippy::too_many_arguments)]
pub fn proofread_cues(
    source: &Transcript,
    context: Option<&TranslationContext>,
    strip_sound_tags: bool,
    profile_id: &str,
    model_id: Option<&str>,
    reasoning_effort: Option<&str>,
    invoker: &dyn AgentInvoker,
    on_progress: &mut (impl FnMut(ProgressUpdate) + Send),
    checkpoint: Option<&dyn BatchCheckpoint>,
    job_id: &str,
) -> Result<Vec<Cue>, SubtitleError> {
    let working: Vec<Cue> = source
        .cues
        .iter()
        .map(|cue| Cue {
            index: cue.index,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: if strip_sound_tags {
                lumina_subtitle::parse::strip_bracketed_sound_tags(&cue.text)
            } else {
                cue.text.clone()
            },
        })
        .filter(|cue| !cue.text.trim().is_empty())
        .collect();
    if working.is_empty() {
        return Err(SubtitleError::export_failed(Some(
            "source transcript empty",
        )));
    }

    let total = working.len().div_ceil(TRANSLATE_BATCH_SIZE);
    let chunks: Vec<&[Cue]> = working.chunks(TRANSLATE_BATCH_SIZE).collect();
    let resumed: BTreeMap<usize, CheckpointBatch> = checkpoint
        .map(|store| store.load_completed())
        .unwrap_or_default();
    // Counter starts at zero: replayed batches emit their own completion
    // events, so seeding it with resumed.len() would overshoot the total.
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let progress = std::sync::Mutex::new(on_progress);
    if !resumed.is_empty() {
        if let Ok(mut guard) = progress.lock() {
            guard(ProgressUpdate {
                message: format!("已恢复 {}/{} 批，继续校对...", resumed.len(), total),
                done: Some(resumed.len()),
                total: Some(total),
            });
        }
    }
    let source_lang = source_language_name(source);

    let outputs = run_batches_in_order(chunks.len(), |batch_idx| -> Result<_, SubtitleError> {
        let chunk = chunks[batch_idx];
        let attempt = AttemptTracker::new(job_id, batch_idx);
        if let Some(saved) = resumed
            .get(&batch_idx)
            .filter(|saved| saved.texts.len() == chunk.len())
        {
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if let Ok(mut guard) = progress.lock() {
                guard(ProgressUpdate {
                    message: format!("已完成 {done}/{total} 批"),
                    done: Some(done),
                    total: Some(total),
                });
            }
            return Ok(saved.texts.clone());
        }
        let mut conversation = invoker
            .open_conversation(Some(attempt.label(1)))
            .map_err(map_agent_error)?;
        let out = proofread_batch(
            chunk,
            &source_lang,
            context,
            None,
            profile_id,
            model_id,
            reasoning_effort,
            invoker,
            &mut *conversation,
            &attempt,
        )?;
        let texts = align_with_one_retry(chunk, out, &attempt, |note| {
            proofread_batch(
                chunk,
                &source_lang,
                context,
                Some(note.as_str()),
                profile_id,
                model_id,
                reasoning_effort,
                invoker,
                &mut *conversation,
                &attempt,
            )
        })?;
        if let Some(store) = checkpoint {
            let batch = CheckpointBatch {
                texts: texts.clone(),
                reported: Vec::new(),
            };
            if let Err(error) = store.save_batch(batch_idx, &batch) {
                tracing::warn!(%error, batch = batch_idx, "checkpoint save failed, continuing in memory");
            }
        }
        let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if let Ok(mut guard) = progress.lock() {
            guard(ProgressUpdate {
                message: format!("已完成 {done}/{total} 批"),
                done: Some(done),
                total: Some(total),
            });
        }
        Ok(texts)
    })?;

    let mut proofread = Vec::with_capacity(working.len());
    for (chunk, texts) in chunks.iter().zip(outputs) {
        for (cue, text) in chunk.iter().zip(texts) {
            proofread.push(Cue {
                index: cue.index,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                text,
            });
        }
    }
    if proofread.len() != working.len() {
        return Err(SubtitleError::export_failed(Some(
            "proofread cue count mismatch after join",
        )));
    }
    Ok(proofread)
}
