//! Resolve subtitle cue excerpts attached to a note.

use crate::notes::model::NoteQuote;
use crate::subtitle::model::Cue;

pub const QUOTE_CONTEXT: usize = 3;
pub const HINT_WINDOW_MS: u64 = 60_000;

#[derive(Debug, Clone)]
pub struct QuoteResolveInput<'a> {
    pub cues: &'a [Cue],
    pub position_ms: u64,
    pub anchor_cue_index: Option<u32>,
    pub quote_cue_indices: Option<&'a [u32]>,
    pub quote_hint: Option<&'a str>,
}

pub fn resolve_quotes(input: QuoteResolveInput<'_>) -> Vec<NoteQuote> {
    let cues = input.cues;
    if cues.is_empty() {
        return Vec::new();
    }

    if let Some(indices) = input
        .quote_cue_indices
        .filter(|items| !items.is_empty())
    {
        return collect_manual(cues, indices, input.anchor_cue_index);
    }

    let anchor = input
        .anchor_cue_index
        .and_then(|index| cues.iter().position(|cue| cue.index == index))
        .or_else(|| {
            let hint = input.quote_hint.filter(|text| !text.trim().is_empty());
            if hint.is_some() {
                find_by_hint(cues, input.position_ms, hint.unwrap())
            } else {
                None
            }
        });

    if let Some(anchor_index) = anchor {
        return collect_around_anchor(cues, anchor_index);
    }

    collect_recent_before(cues, input.position_ms)
}

fn cue_to_quote(cue: &Cue, anchor: bool) -> NoteQuote {
    NoteQuote {
        index: cue.index,
        start_ms: cue.start_ms,
        end_ms: cue.end_ms,
        text: cue.text.clone(),
        anchor,
    }
}

fn collect_manual(cues: &[Cue], indices: &[u32], anchor_cue_index: Option<u32>) -> Vec<NoteQuote> {
    let mut quotes: Vec<NoteQuote> = indices
        .iter()
        .filter_map(|id| cues.iter().find(|cue| cue.index == *id))
        .map(|cue| cue_to_quote(cue, anchor_cue_index == Some(cue.index)))
        .collect();
    quotes.sort_by_key(|quote| quote.start_ms);
    quotes
}

fn collect_around_anchor(cues: &[Cue], anchor_index: usize) -> Vec<NoteQuote> {
    let start = anchor_index.saturating_sub(QUOTE_CONTEXT);
    let end = (anchor_index + QUOTE_CONTEXT).min(cues.len().saturating_sub(1));
    (start..=end)
        .map(|index| cue_to_quote(&cues[index], index == anchor_index))
        .collect()
}

fn collect_recent_before(cues: &[Cue], position_ms: u64) -> Vec<NoteQuote> {
    let indices: Vec<usize> = cues
        .iter()
        .enumerate()
        .filter(|(_, cue)| cue.start_ms < position_ms)
        .map(|(index, _)| index)
        .collect();
    let start = indices.len().saturating_sub(QUOTE_CONTEXT);
    indices[start..]
        .iter()
        .map(|&index| cue_to_quote(&cues[index], false))
        .collect()
}

fn find_by_hint(cues: &[Cue], position_ms: u64, hint: &str) -> Option<usize> {
    let hint_lower = normalize_match_text(hint);
    if hint_lower.is_empty() {
        return None;
    }
    let window_start = position_ms.saturating_sub(HINT_WINDOW_MS);
    let window_end = position_ms.saturating_add(HINT_WINDOW_MS);

    let mut best: Option<(usize, u64)> = None;
    for (index, cue) in cues.iter().enumerate() {
        if cue.end_ms <= window_start || cue.start_ms >= window_end {
            continue;
        }
        let text_lower = normalize_match_text(&cue.text);
        if !text_lower.contains(&hint_lower) {
            continue;
        }
        let distance = cue.start_ms.abs_diff(position_ms);
        match best {
            None => best = Some((index, distance)),
            Some((_, best_distance)) if distance < best_distance => {
                best = Some((index, distance));
            }
            _ => {}
        }
    }
    best.map(|(index, _)| index)
}

fn normalize_match_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(index: u32, start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue {
            index,
            start_ms,
            end_ms,
            text: text.into(),
        }
    }

    #[test]
    fn recent_three_before_position() {
        let cues = vec![
            cue(1, 0, 1_000, "a"),
            cue(2, 1_000, 2_000, "b"),
            cue(3, 2_000, 3_000, "c"),
            cue(4, 3_000, 4_000, "d"),
            cue(5, 4_000, 5_000, "e"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 4_500,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: None,
        });
        assert_eq!(quotes.len(), 3);
        assert_eq!(quotes[0].text, "c");
        assert_eq!(quotes[2].text, "e");
        assert!(!quotes.iter().any(|q| q.anchor));
    }

    #[test]
    fn around_anchor_by_index() {
        let cues = vec![
            cue(1, 0, 1_000, "a"),
            cue(2, 1_000, 2_000, "b"),
            cue(3, 2_000, 3_000, "c"),
            cue(4, 3_000, 4_000, "d"),
            cue(5, 4_000, 5_000, "e"),
            cue(6, 5_000, 6_000, "f"),
            cue(7, 6_000, 7_000, "g"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 3_500,
            anchor_cue_index: Some(4),
            quote_cue_indices: None,
            quote_hint: None,
        });
        assert_eq!(quotes.len(), 7);
        assert_eq!(quotes[3].text, "d");
        assert!(quotes[3].anchor);
    }

    #[test]
    fn hint_within_one_minute() {
        let cues = vec![
            cue(1, 58_000, 59_000, "开场"),
            cue(2, 60_000, 61_000, "她捂住了鼻子"),
            cue(3, 62_000, 63_000, "后续"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 60_500,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: Some("鼻子"),
        });
        assert!(quotes.iter().any(|q| q.anchor && q.text.contains("鼻子")));
        assert!(quotes.len() >= 1);
    }

    #[test]
    fn manual_indices_preserve_user_selection() {
        let cues = vec![
            cue(1, 0, 1_000, "a"),
            cue(2, 1_000, 2_000, "b"),
            cue(5, 4_000, 5_000, "e"),
            cue(7, 6_000, 7_000, "g"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 5_000,
            anchor_cue_index: Some(5),
            quote_cue_indices: Some(&[5, 1, 7]),
            quote_hint: None,
        });
        assert_eq!(quotes.len(), 3);
        assert_eq!(quotes[0].text, "a");
        assert_eq!(quotes[1].text, "e");
        assert!(quotes[1].anchor);
        assert_eq!(quotes[2].text, "g");
    }
}
