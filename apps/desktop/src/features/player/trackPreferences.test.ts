import { describe, expect, it } from "vitest";

import type { SubtitleChoice } from "@/features/transcript/types";

import {
  mediaDirectoryKey,
  resolveDirectoryTrackSelection,
  resolveSubtitleChoiceId,
  sidecarTokenForMedia,
  subtitlePreferenceFromChoice,
} from "./trackPreferences";

describe("trackPreferences", () => {
  it("uses the same directory key for files in one folder", () => {
    expect(
      mediaDirectoryKey("D:\\shows\\ep01.mkv"),
    ).toBe(mediaDirectoryKey("D:\\shows\\ep02.mkv"));
  });

  it("reuses sidecar token across episodes in the same directory", () => {
    const choiceEp1: SubtitleChoice = {
      id: "sidecar:D:\\shows\\ep01.zh.srt",
      source: "Sidecar",
      label: "外挂 · zh",
      supported: true,
      externalPath: "D:\\shows\\ep01.zh.srt",
      language: "zh",
    };
    const token = sidecarTokenForMedia(
      "D:\\shows\\ep01.mkv",
      "D:\\shows\\ep01.zh.srt",
    );
    expect(token).toBe(".zh.srt");

    const choices: SubtitleChoice[] = [
      {
        id: "sidecar:D:\\shows\\ep02.zh.srt",
        source: "Sidecar",
        label: "外挂 · zh",
        supported: true,
        externalPath: "D:\\shows\\ep02.zh.srt",
        language: "zh",
      },
    ];

    const pref = subtitlePreferenceFromChoice(choiceEp1, "D:\\shows\\ep01.mkv");
    expect(
      resolveSubtitleChoiceId(pref, choices, "D:\\shows\\ep02.mkv"),
    ).toBe("sidecar:D:\\shows\\ep02.zh.srt");
  });

  it("falls back to default subtitle when directory preference cannot match", () => {
    const pref = subtitlePreferenceFromChoice(
      {
        id: "embedded:3",
        source: "Embedded",
        label: "内嵌 · ja",
        supported: true,
        streamIndex: 3,
        language: "ja",
      },
      "D:\\shows\\ep01.mkv",
    );

    const choices: SubtitleChoice[] = [
      {
        id: "embedded:1",
        source: "Embedded",
        label: "内嵌 · en",
        supported: true,
        streamIndex: 1,
        language: "en",
      },
    ];

    expect(
      resolveSubtitleChoiceId(pref, choices, "D:\\shows\\ep02.mkv"),
    ).toBe("embedded:1");
  });

  it("applies stored directory prefs when opening another file", () => {
    const dir = mediaDirectoryKey("D:\\shows\\ep01.mkv");
    const resolved = resolveDirectoryTrackSelection(
      "D:\\shows\\ep02.mkv",
      [
        {
          id: "embedded:2",
          source: "Embedded",
          label: "内嵌 · zh",
          supported: true,
          streamIndex: 2,
          language: "zh",
        },
      ],
      [
        { index: 1, language: "ja" },
        { index: 2, language: "zh" },
      ],
      {
        [dir]: {
          subtitle: {
            mode: "embedded",
            language: "zh",
            streamIndex: 2,
          },
          audio: { language: "zh", streamIndex: 2 },
        },
      },
    );

    expect(resolved.subtitleChoiceId).toBe("embedded:2");
    expect(resolved.audioStreamIndex).toBe(2);
  });
});
