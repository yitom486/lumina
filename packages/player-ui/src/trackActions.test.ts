import { describe, expect, it, vi } from "vitest";

import type { SubtitleChoice } from "@lumina/contracts";

import { applySubtitleChoice, audioTrackLabel } from "./trackActions";

function embeddedChoice(): SubtitleChoice {
  return {
    id: "embedded:0",
    source: "Embedded",
    label: "内嵌",
    supported: true,
    streamIndex: 0,
    externalPath: null,
    codecName: "subrip",
    language: "zh",
  };
}

describe("audioTrackLabel", () => {
  it("joins language, codec and channels", () => {
    expect(
      audioTrackLabel({ index: 1, language: "kor", codecName: "aac", channels: 2 }),
    ).toBe("kor · aac · 2ch");
  });

  it("falls back for missing fields", () => {
    expect(audioTrackLabel({ index: 0 })).toBe("und · audio");
  });
});

describe("applySubtitleChoice", () => {
  it("clears the subtitle when choice is missing", async () => {
    const setSubtitle = vi.fn(async () => {});
    await applySubtitleChoice(undefined, setSubtitle);
    expect(setSubtitle).toHaveBeenCalledWith({ source: "None" });
  });

  it("selects embedded tracks by stream index", async () => {
    const setSubtitle = vi.fn(async () => {});
    await applySubtitleChoice(embeddedChoice(), setSubtitle);
    expect(setSubtitle).toHaveBeenCalledWith({
      source: "Embedded",
      streamIndex: 0,
    });
  });

  it("resolves online choices through the injected loader", async () => {
    const setSubtitle = vi.fn(async () => {});
    const loadOnlineTranscript = vi.fn(async () => ({
      sourcePath: "/cache/online.srt",
      choiceId: "online:abc",
      streamIndex: null,
      language: "en",
      codecName: "subrip",
      cues: [],
    }));
    const choice: SubtitleChoice = {
      ...embeddedChoice(),
      id: "online:abc",
      source: "Sidecar",
      externalPath: null,
    };
    await applySubtitleChoice(
      choice,
      setSubtitle,
      "https://example.test/watch",
      loadOnlineTranscript,
    );
    expect(loadOnlineTranscript).toHaveBeenCalledWith(
      "https://example.test/watch",
      "online:abc",
    );
    expect(setSubtitle).toHaveBeenCalledWith({
      source: "Sidecar",
      externalPath: "/cache/online.srt",
    });
  });

  it("skips online choices without a loader", async () => {
    const setSubtitle = vi.fn(async () => {});
    const choice: SubtitleChoice = {
      ...embeddedChoice(),
      id: "online:abc",
      source: "Sidecar",
      externalPath: null,
    };
    await applySubtitleChoice(choice, setSubtitle, "https://example.test/watch");
    expect(setSubtitle).not.toHaveBeenCalled();
  });
});
