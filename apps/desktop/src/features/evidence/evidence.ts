/**
 * P6-M2 structured citation contract.
 *
 * A citation is only ever *rendered* clickable after it resolves against
 * known evidence (current media + duration, or a library-resolved episode
 * file). Model-emitted timestamps that fail validation stay plain text:
 * clickable is navigation, never proof.
 */

export type EvidenceSource = "subtitle" | "video" | "audio";

export type EvidenceTarget =
  | { kind: "current" }
  | {
      kind: "episode";
      /** Null = same season as the current media (backend inherits). */
      season: number | null;
      episode: number;
    };

export type EvidenceRef = {
  target: EvidenceTarget;
  startMs: number;
  /** Null means a point reference (single timestamp). */
  endMs: number | null;
  /** Timestamp citations default to subtitle; extended by P6-M3/P7. */
  source: EvidenceSource;
  /** Subtitle track / source version for future cue-coverage checks (P6-M3). */
  trackId: string | null;
  /** Original bracket text, e.g. "[03:12]" / "[第2集 · 03:12]". */
  label: string;
};

export type UnverifiedReason =
  | "unknown-media"
  | "no-duration"
  | "out-of-range"
  | "unresolvable-episode";

export type ParsedSegment =
  | { kind: "text"; text: string }
  | { kind: "citation"; ref: EvidenceRef };

export type ResolvedCitation =
  | { verified: true; ref: EvidenceRef; mediaPath: string }
  | { verified: false; ref: EvidenceRef; reason: UnverifiedReason };

export type EvidenceContext = {
  currentFile: string | null;
  durationMs: number;
  /** Null season is passed as 0 (backend inherits the current season). */
  resolveEpisodeFile: (
    season: number | null,
    episode: number,
  ) => Promise<string | null>;
  fetchTargetDurationMs?: (mediaPath: string) => Promise<number | null>;
};

const HOUR = 3_600_000;
const MINUTE = 60_000;
const SECOND = 1_000;

/**
 * Conservative by construction: only bracketed forms linkify.
 * `[03:12]` / `[1:02:03]` / `[03:12-04:00]` / `[第2集 · 03:12]` /
 * `[第2季第3集 · 03:12]`. Bare timestamps in prose never become links.
 */
const CITATION_RE =
  /\[((?:第\s*(?:(\d{1,3})\s*季\s*第\s*)?(\d{1,3})\s*集\s*[·•]\s*)?(?:(\d{1,3}):)?(\d{1,3}):(\d{2})(?:\s*[-–—]\s*(?:(\d{1,3}):)?(\d{1,3}):(\d{2}))?)\]/g;

function toMs(hour: string | undefined, min: string, sec: string): number {
  return (
    (hour ? Number(hour) : 0) * HOUR + Number(min) * MINUTE + Number(sec) * SECOND
  );
}

export function parseEvidenceSegments(text: string): ParsedSegment[] {
  const raw: ParsedSegment[] = [];
  let last = 0;
  CITATION_RE.lastIndex = 0;
  for (;;) {
    const match = CITATION_RE.exec(text);
    if (!match) break;
    if (match.index > last) {
      raw.push({ kind: "text", text: text.slice(last, match.index) });
    }
    const [, , seasonRaw, ep, h1, m1, s1, h2, m2, s2] = match;
    const full = match[0];
    if (m1 == null || s1 == null) {
      raw.push({ kind: "text", text: full });
    } else {
      const startMs = toMs(h1, m1, s1);
      const endMs = m2 != null && s2 != null ? toMs(h2, m2, s2) : null;
      raw.push({
        kind: "citation",
        ref: {
          target:
            ep != null
              ? {
                  kind: "episode",
                  season: seasonRaw != null ? Number(seasonRaw) : null,
                  episode: Number(ep),
                }
              : { kind: "current" },
          startMs,
          endMs,
          source: "subtitle",
          trackId: null,
          label: full,
        },
      });
    }
    last = match.index + full.length;
  }
  if (last < text.length) {
    raw.push({ kind: "text", text: text.slice(last) });
  }
  return mergeBracketRanges(raw);
}

function sameTarget(a: EvidenceTarget, b: EvidenceTarget): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "current" || b.kind === "current") return true;
  return a.season === b.season && a.episode === b.episode;
}

/**
 * Models also emit ranges as two brackets (`[11:27] – [12:04]`).
 * Merge citation + dash-only text + citation into one range citation.
 */
function mergeBracketRanges(segments: ParsedSegment[]): ParsedSegment[] {
  const out: ParsedSegment[] = [];
  let index = 0;
  while (index < segments.length) {
    const first = segments[index];
    const middle = segments[index + 1];
    const second = segments[index + 2];
    if (
      first?.kind === "citation" &&
      middle?.kind === "text" &&
      /^\s*[-–—]\s*$/.test(middle.text) &&
      second?.kind === "citation" &&
      sameTarget(first.ref.target, second.ref.target)
    ) {
      const endMs = second.ref.endMs ?? second.ref.startMs;
      out.push({
        kind: "citation",
        ref: {
          ...first.ref,
          endMs,
          label: first.ref.label + middle.text + second.ref.label,
        },
      });
      index += 3;
    } else {
      out.push(first as ParsedSegment);
      index += 1;
    }
  }
  return out;
}

function inRange(ref: EvidenceRef, durationMs: number): boolean {
  if (ref.startMs < 0 || ref.startMs > durationMs) return false;
  if (ref.endMs == null) return true;
  return ref.endMs >= ref.startMs && ref.endMs <= durationMs;
}

function unverified(ref: EvidenceRef, reason: UnverifiedReason): ResolvedCitation {
  return { verified: false, ref, reason };
}

export async function resolveCitation(
  ref: EvidenceRef,
  ctx: EvidenceContext,
): Promise<ResolvedCitation> {
  if (!ctx.currentFile) return unverified(ref, "unknown-media");
  if (ref.target.kind === "current") {
    if (!(ctx.durationMs > 0)) return unverified(ref, "no-duration");
    if (!inRange(ref, ctx.durationMs)) return unverified(ref, "out-of-range");
    return { verified: true, ref, mediaPath: ctx.currentFile };
  }
  const { season, episode } = ref.target;
  // Season null = same season as current media (backend inherits via season 0).
  // Episode must always be explicit and positive.
  if (
    (season != null &&
      (!Number.isInteger(season) || season <= 0)) ||
    !Number.isInteger(episode) ||
    episode <= 0
  ) {
    return unverified(ref, "unresolvable-episode");
  }
  let target: string | null;
  try {
    target = await ctx.resolveEpisodeFile(season, episode);
  } catch {
    return unverified(ref, "unresolvable-episode");
  }
  if (!target) return unverified(ref, "unresolvable-episode");
  // Cross-episode bounds need the target duration; unknown means unverifiable.
  let duration: number | null = null;
  if (ctx.fetchTargetDurationMs) {
    try {
      duration = await ctx.fetchTargetDurationMs(target);
    } catch {
      duration = null;
    }
  }
  if (!(duration != null && duration > 0)) {
    return unverified(ref, "no-duration");
  }
  if (!inRange(ref, duration)) return unverified(ref, "out-of-range");
  return { verified: true, ref, mediaPath: target };
}

/** User-facing reason copy for unverified citations (never technical details). */
export function unverifiedReasonText(reason: UnverifiedReason): string {
  switch (reason) {
    case "unknown-media":
      return "无当前媒体，引用不可验证";
    case "no-duration":
      return "未知时长，引用不可验证";
    case "out-of-range":
      return "超出时长范围，引用不可验证";
    case "unresolvable-episode":
      return "找不到该分集，引用不可验证";
  }
}
