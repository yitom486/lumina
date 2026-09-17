import { describe, expect, it } from "vitest";

import {
  mapLoadedTranscript,
  stripTranscriptScaffolding,
} from "./conversationTranscript";

describe("mapLoadedTranscript", () => {
  it("filters the scaffold trio but keeps the real question and agent body", () => {
    const turns = mapLoadedTranscript([
      {
        role: "user",
        text: [
          "【工具优先】本轮优先使用 Lumina 本地工具：lumina_get_playback_context。",
          "resource_link file:///D:/movie/demo.mp4 demo.mp4",
          "file:///D:/movie/demo.mp4",
          "【当前播放】",
          "媒体：demo.mp4",
          "进度：01:00 / 02:00（60000ms）",
          "字幕轨道：embedded:0",
          "",
          "这段讲了什么？",
        ].join("\n"),
      },
      { role: "tool", text: "playback context json" },
      { role: "agent", text: "这段主要讲离别。" },
    ]);
    expect(turns).toHaveLength(1);
    expect(turns[0]?.userText).toBe("这段讲了什么？");
    expect(turns[0]?.userText).not.toMatch(/【工具优先】|【当前播放】|resource_link|file:\/\//);
    expect(turns[0]?.answer).toContain("这段主要讲离别");
  });

  it("keeps the Natural English prefix untouched", () => {
    const turns = mapLoadedTranscript([
      { role: "user", text: "总结一下" },
      {
        role: "agent",
        text: "Natural English: Summarize this part.\n\n这段主要讲重逢。",
      },
    ]);
    expect(turns).toHaveLength(1);
    expect(turns[0]?.answer).toMatch(/Natural English/);
    expect(turns[0]?.answer).toContain("这段主要讲重逢");
  });

  it("returns [] for empty or scaffold-only input without crashing", () => {
    expect(mapLoadedTranscript([])).toEqual([]);
    expect(mapLoadedTranscript(null)).toEqual([]);
    expect(mapLoadedTranscript(undefined)).toEqual([]);
    expect(
      mapLoadedTranscript([
        {
          role: "user",
          text: "【工具优先】xxx\n【当前播放】\n媒体：demo.mp4",
        },
      ]),
    ).toEqual([]);
    expect(stripTranscriptScaffolding("")).toBe("");
  });
});
