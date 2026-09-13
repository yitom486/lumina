import { describe, expect, it } from "vitest";

import { subtitleChoicesKey, transcriptKey } from "./queries";

describe("transcript query keys", () => {
  it("builds the shared transcript key", () => {
    expect(transcriptKey("C:\\v\\a.mp4", "s1")).toEqual([
      "transcript",
      "C:\\v\\a.mp4",
      "s1",
    ]);
  });

  it("builds the shared subtitle-choices key", () => {
    expect(subtitleChoicesKey("C:\\v\\a.mp4")).toEqual([
      "subtitleChoices",
      "C:\\v\\a.mp4",
    ]);
  });
});
