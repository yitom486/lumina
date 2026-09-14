import { describe, expect, it } from "vitest";

import { pickSingleCandidate, splitManualTitle } from "./autoMatch";
import type { TmdbCandidate } from "./types";

function candidate(
  tmdbId: number,
  title: string,
  year?: number | null,
): TmdbCandidate {
  return { tmdbId, mediaType: "tv", title, year: year ?? null };
}

describe("splitManualTitle", () => {
  it("splits a trailing release year", () => {
    expect(splitManualTitle("Our Beloved Summer 2021")).toEqual({
      text: "Our Beloved Summer",
      year: 2021,
    });
  });

  it("keeps futuristic years and bare years whole", () => {
    expect(splitManualTitle("Blade Runner 2049").year).toBeNull();
    expect(splitManualTitle("2012")).toEqual({ text: "2012", year: null });
  });
});

describe("pickSingleCandidate", () => {
  it("takes a lone candidate without asking", () => {
    const only = candidate(135897, "那年，我们的夏天", 2021);
    expect(pickSingleCandidate([only], "Our Beloved Summer 2021")).toBe(only);
  });

  it("returns null for empty results", () => {
    expect(pickSingleCandidate([], "Whatever")).toBeNull();
  });

  it("picks the unique exact title plus year match", () => {
    const winner = candidate(1, "Show", 2021);
    const others = [candidate(2, "Show", 2019), candidate(3, "Other Show", 2021)];
    expect(pickSingleCandidate([winner, ...others], "Show 2021")).toBe(winner);
  });

  it("refuses ambiguity instead of guessing", () => {
    const list = [candidate(1, "Show", 2021), candidate(2, "Show", 2021)];
    expect(pickSingleCandidate(list, "Show 2021")).toBeNull();
    expect(pickSingleCandidate(list, "Something Else")).toBeNull();
  });
});
