//! Resolve subtitle cue excerpts attached to a note.

use crate::notes::model::NoteQuote;
use crate::subtitle::model::Cue;

pub const QUOTE_CONTEXT: usize = 3;
pub const HINT_WINDOW_MS: u64 = 60_000;

/// Minimum similarity (0..=1) for fuzzy hint matches after exact contains fails.
const FUZZY_MIN_SCORE: f64 = 0.58;

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

    if let Some(indices) = input.quote_cue_indices.filter(|items| !items.is_empty()) {
        return collect_manual(cues, indices, input.anchor_cue_index);
    }

    let anchor = input
        .anchor_cue_index
        .and_then(|index| cues.iter().position(|cue| cue.index == index))
        .or_else(|| {
            let hint = input.quote_hint.filter(|text| !text.trim().is_empty());
            if let Some(hint) = hint {
                find_by_hint(cues, input.position_ms, hint)
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
    let fragments = hint_fragments(hint);
    if fragments.is_empty() {
        return None;
    }

    let window_start = position_ms.saturating_sub(HINT_WINDOW_MS);
    let window_end = position_ms.saturating_add(HINT_WINDOW_MS);

    let mut best: Option<HintCandidate> = None;
    for (index, cue) in cues.iter().enumerate() {
        if cue.end_ms <= window_start || cue.start_ms >= window_end {
            continue;
        }
        let cue_norm = normalize_match_text(&cue.text);
        if cue_norm.is_empty() {
            continue;
        }
        let distance = cue.start_ms.abs_diff(position_ms);
        for fragment in &fragments {
            let score = match_score(fragment, &cue_norm);
            if score < FUZZY_MIN_SCORE {
                continue;
            }
            let candidate = HintCandidate {
                index,
                score,
                distance,
            };
            if is_better_candidate(&candidate, &best) {
                best = Some(candidate);
            }
        }
    }
    best.map(|candidate| candidate.index)
}

#[derive(Debug, Clone, Copy)]
struct HintCandidate {
    index: usize,
    score: f64,
    distance: u64,
}

fn is_better_candidate(next: &HintCandidate, best: &Option<HintCandidate>) -> bool {
    let Some(current) = best else {
        return true;
    };
    // Prefer higher similarity; break ties by closer playback time.
    if (next.score - current.score).abs() > 0.02 {
        return next.score > current.score;
    }
    next.distance < current.distance
}

/// Split Agent / user hints into matchable fragments (full text + lines / clauses).
fn hint_fragments(hint: &str) -> Vec<String> {
    let mut fragments = Vec::new();
    let push_unique = |out: &mut Vec<String>, raw: &str| {
        let normalized = normalize_match_text(raw);
        if normalized.is_empty() {
            return;
        }
        if !out.iter().any(|existing| existing == &normalized) {
            out.push(normalized);
        }
    };

    push_unique(&mut fragments, hint);
    for line in hint.split(['\n', '|', '；', ';']) {
        push_unique(&mut fragments, line);
    }
    // Sentence-ish splits for long prose hints.
    for part in hint.split(['。', '！', '？', '!', '?', '…']) {
        push_unique(&mut fragments, part);
    }
    fragments
}

fn match_score(hint_norm: &str, cue_norm: &str) -> f64 {
    if hint_norm.is_empty() || cue_norm.is_empty() {
        return 0.0;
    }
    if cue_norm.contains(hint_norm) || hint_norm.contains(cue_norm) {
        return 1.0;
    }

    // Compare against sliding windows when cue is longer than the hint.
    let hint_chars: Vec<char> = hint_norm.chars().collect();
    let cue_chars: Vec<char> = cue_norm.chars().collect();
    let hint_len = hint_chars.len();
    let cue_len = cue_chars.len();
    if hint_len == 0 || cue_len == 0 {
        return 0.0;
    }

    if cue_len <= hint_len {
        return similarity_ratio(&hint_chars, &cue_chars);
    }

    let window = hint_len.max(1);
    let mut best = 0.0_f64;
    let last_start = cue_len.saturating_sub(window);
    for start in 0..=last_start {
        let slice = &cue_chars[start..start + window];
        let score = similarity_ratio(&hint_chars, slice);
        if score > best {
            best = score;
        }
        if best >= 0.99 {
            break;
        }
    }
    // Also allow slightly longer windows for short OCR / punctuation remnants.
    if window + 2 <= cue_len {
        for start in 0..=(cue_len - (window + 2)) {
            let slice = &cue_chars[start..start + window + 2];
            let score = similarity_ratio(&hint_chars, slice);
            if score > best {
                best = score;
            }
        }
    }
    best
}

fn similarity_ratio(a: &[char], b: &[char]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 0.0;
    }
    let distance = levenshtein(a, b);
    1.0 - (distance as f64 / max_len as f64)
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

fn normalize_match_text(text: &str) -> String {
    text.chars()
        .filter_map(|ch| {
            if ch.is_whitespace() {
                return None;
            }
            if is_ignorable_punct(ch) {
                return None;
            }
            Some(ch.to_lowercase().next().unwrap_or(ch))
        })
        .collect()
}

fn is_ignorable_punct(ch: char) -> bool {
    matches!(
        ch,
        ',' | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | '"'
            | '\''
            | '`'
            | '~'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '/'
            | '\\'
            | '|'
            | '-'
            | '_'
            | '*'
            | '#'
            | '@'
            | '&'
            | '%'
            | '+'
            | '='
            | '，'
            | '。'
            | '！'
            | '？'
            | '；'
            | '：'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '（'
            | '）'
            | '【'
            | '】'
            | '『'
            | '』'
            | '「'
            | '」'
            | '《'
            | '》'
            | '、'
            | '…'
            | '—'
            | '–'
            | '·'
            | '〜'
            | '～'
    ) || ch.is_ascii_punctuation()
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
        assert!(!quotes.is_empty());
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

    #[test]
    fn hint_ignores_punctuation_and_case() {
        let cues = vec![
            cue(1, 10_000, 11_000, "Hello, world!"),
            cue(2, 12_000, 13_000, "别的台词"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 10_500,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: Some("hello world"),
        });
        assert!(quotes.iter().any(|q| q.anchor && q.text.contains("Hello")));
    }

    #[test]
    fn hint_fuzzy_tolerates_typo() {
        let cues = vec![
            cue(1, 20_000, 21_000, "前一句"),
            cue(2, 22_000, 23_000, "她捂住了鼻子"),
            cue(3, 24_000, 25_000, "后一句"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 22_500,
            anchor_cue_index: None,
            quote_cue_indices: None,
            // one character typo vs 鼻子
            quote_hint: Some("她捂住了鼻了"),
        });
        assert!(
            quotes
                .iter()
                .any(|q| q.anchor && q.text.contains("捂住了鼻子")),
            "fuzzy typo should still anchor the intended cue"
        );
    }

    #[test]
    fn hint_multiline_matches_best_clause() {
        let cues = vec![
            cue(1, 30_000, 31_000, "化学反应"),
            cue(2, 32_000, 33_000, "转变开始"),
            cue(3, 34_000, 35_000, "别的"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 32_200,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: Some("东晚哥在开玩笑。\n转变开始。\n然后离开。"),
        });
        assert!(quotes
            .iter()
            .any(|q| q.anchor && q.text.contains("转变开始")));
    }

    #[test]
    fn hint_prefers_closer_cue_when_scores_tie() {
        let cues = vec![
            cue(1, 40_000, 41_000, "相同关键词"),
            cue(2, 50_000, 51_000, "相同关键词"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 49_500,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: Some("相同关键词"),
        });
        assert!(quotes.iter().any(|q| q.anchor && q.index == 2));
    }

    #[test]
    fn unmatched_hint_falls_back_to_recent_three() {
        let cues = vec![
            cue(1, 0, 1_000, "a"),
            cue(2, 1_000, 2_000, "b"),
            cue(3, 2_000, 3_000, "c"),
            cue(4, 3_000, 4_000, "d"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 3_500,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: Some("完全不存在的台词片段xyz"),
        });
        assert_eq!(quotes.len(), 3);
        assert!(!quotes.iter().any(|q| q.anchor));
        assert_eq!(quotes[0].text, "b");
        assert_eq!(quotes[2].text, "d");
    }

    #[test]
    fn korean_substring_hint() {
        let cues = vec![
            cue(1, 10_000, 11_000, "안녕하세요"),
            cue(2, 12_000, 13_000, "오늘 날씨가 좋아요"),
        ];
        let quotes = resolve_quotes(QuoteResolveInput {
            cues: &cues,
            position_ms: 12_200,
            anchor_cue_index: None,
            quote_cue_indices: None,
            quote_hint: Some("날씨가"),
        });
        assert!(quotes.iter().any(|q| q.anchor && q.text.contains("날씨가")));
    }
}
