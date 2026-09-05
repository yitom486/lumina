import { describe, expect, it } from "vitest";

import {
  parseEvidenceSegments,
  resolveCitation,
  type EvidenceContext,
  type EvidenceRef,
} from "./evidence";

function current(
  startMs: number,
  endMs: number | null = null,
): EvidenceRef {
  return {
    target: { kind: "current" },
    startMs,
    endMs,
    source: "subtitle",
    trackId: null,
    label: "[x]",
  };
}

const CTX: EvidenceContext = {
  currentFile: "C:\\v\\a.mp4",
  durationMs: 3_600_000,
  resolveEpisodeFile: async () => null,
};

describe("parseEvidenceSegments", () => {
  it("parses mm:ss and h:mm:ss", () => {
    const [seg] = parseEvidenceSegments("看[03:12]这里");
    expect(seg.kind).toBe("text");
    const segs = parseEvidenceSegments("看[03:12]和[1:02:03]这里");
    expect(segs).toHaveLength(5);
    const first = segs[1];
    const second = segs[3];
    expect(first.kind === "citation" && first.ref.startMs).toBe(192_000);
    expect(second.kind === "citation" && second.ref.startMs).toBe(3_723_000);
  });

  it("parses ranges and cross-episode forms", () => {
    const [seg] = parseEvidenceSegments("[03:12-04:00]");
    expect(seg.kind === "citation" && seg.ref.endMs).toBe(240_000);
    const [ep] = parseEvidenceSegments("[第2集 · 03:12]");
    expect(ep.kind === "citation" && ep.ref.target).toEqual({
      kind: "episode",
      season: null,
      episode: 2,
    });
    const [withSeason] = parseEvidenceSegments("[第2季第3集 · 00:10]");
    expect(
      withSeason.kind === "citation" && withSeason.ref.target,
    ).toEqual({ kind: "episode", season: 2, episode: 3 });
  });

  it("leaves bare timestamps and non-times alone", () => {
    expect(parseEvidenceSegments("3:12 开会")).toEqual([
      { kind: "text", text: "3:12 开会" },
    ]);
    expect(parseEvidenceSegments("[不是时间]")).toEqual([
      { kind: "text", text: "[不是时间]" },
    ]);
    expect(parseEvidenceSegments("")).toEqual([]);
  });
});

describe("resolveCitation", () => {
  it("verifies in-range current-media citations", async () => {
    const ok = await resolveCitation(current(192_000), CTX);
    expect(ok).toEqual({
      verified: true,
      ref: expect.anything(),
      mediaPath: "C:\\v\\a.mp4",
    });
  });

  it("rejects out-of-range, inverted ranges, unknown media and duration", async () => {
    expect(
      await resolveCitation(current(3_600_001), CTX),
    ).toMatchObject({ verified: false, reason: "out-of-range" });
    expect(
      await resolveCitation(current(200_000, 100_000), CTX),
    ).toMatchObject({ verified: false, reason: "out-of-range" });
    expect(
      await resolveCitation(current(100), { ...CTX, currentFile: null }),
    ).toMatchObject({ verified: false, reason: "unknown-media" });
    expect(
      await resolveCitation(current(100), { ...CTX, durationMs: 0 }),
    ).toMatchObject({ verified: false, reason: "no-duration" });
  });

  it("resolves cross-episode files and bounds", async () => {
    const ref: EvidenceRef = {
      ...current(60_000),
      target: { kind: "episode", season: null, episode: 2 },
    };
    const ok = await resolveCitation(ref, {
      ...CTX,
      resolveEpisodeFile: async () => "C:\\v\\b.mp4",
      fetchTargetDurationMs: async () => 120_000,
    });
    expect(ok).toEqual({
      verified: true,
      ref: expect.anything(),
      mediaPath: "C:\\v\\b.mp4",
    });

    const missing = await resolveCitation(ref, CTX);
    expect(missing).toMatchObject({
      verified: false,
      reason: "unresolvable-episode",
    });

    const badSeason: EvidenceRef = {
      ...current(60_000),
      target: { kind: "episode", season: 0, episode: 2 },
    };
    expect(await resolveCitation(badSeason, CTX)).toMatchObject({
      verified: false,
      reason: "unresolvable-episode",
    });
  });
});
