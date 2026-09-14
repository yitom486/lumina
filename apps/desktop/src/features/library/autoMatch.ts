import type { TmdbCandidate } from "./types";

/**
 * One-click match helpers (frontend orchestration over the existing
 * search/apply commands — no new IPC). Mirrors the backend trailing-year
 * split so the unique-winner rule sees the same title/year the search used.
 */
export function splitManualTitle(input: string): {
  text: string;
  year: number | null;
} {
  const text = input.trim();
  const match = text.match(/^(.*)\s+(\d{4})$/);
  if (match && match[1].trim()) {
    const year = Number(match[2]);
    if (year >= 1900 && year <= 2030) return { text: match[1].trim(), year };
  }
  return { text, year: null };
}

function normalizeTitle(value: string): string {
  return value
    .toLowerCase()
    .replace(/[\s_.()[\]{}\-–—:：,，!！?？'’"“”]/g, "");
}

/**
 * Decide whether the search result has an unambiguous winner that one-click
 * match may confirm without asking. Conservative on purpose: a single
 * candidate wins outright; otherwise exactly one candidate must match the
 * typed title (script-sensitive — CJK input never equals a romanized title)
 * with a compatible year. Anything else returns null and the caller falls
 * back to the manual candidate list.
 */
export function pickSingleCandidate(
  candidates: TmdbCandidate[],
  title: string,
): TmdbCandidate | null {
  if (candidates.length === 1) return candidates[0];
  if (candidates.length === 0) return null;
  const { text, year } = splitManualTitle(title);
  if (!text) return null;
  const wanted = normalizeTitle(text);
  const exact = candidates.filter(
    (candidate) => normalizeTitle(candidate.title) === wanted,
  );
  if (exact.length === 0) return null;
  if (year == null) return exact.length === 1 ? exact[0] : null;
  const dated = exact.filter(
    (candidate) => candidate.year == null || candidate.year === year,
  );
  return dated.length === 1 ? dated[0] : null;
}
