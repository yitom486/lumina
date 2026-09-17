import { describe, expect, it } from "vitest";

import {
  mapLoadedTranscript,
  stripAgentEchoes,
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

  it("strips playback details even when the marker landed in another chunk", () => {
    // 真实回放：【当前播放】标记与明细行不在同一 chunk，
    // 有状态机只认“见过标记”，明细就漏网了（线上用户泡即如此）。
    const turns = mapLoadedTranscript([
      {
        role: "user",
        text: [
          "媒体：Our.Beloved.Summer.2021.EP03.HD1080P.X264.AAC.Korean.CHS.Mp4er.mp4",
          "进度：00:00 / 59:34（0ms）",
          "集数：S01E03",
          "字幕轨道：cache:subdl:en",
          "本集标题：我讨厌你的十个理由",
          "",
          "我们刚刚都聊了一些什么内容呢",
        ].join("\n"),
      },
      { role: "agent", text: "我们其实才刚开始聊。" },
    ]);
    expect(turns).toHaveLength(1);
    expect(turns[0]?.userText).toBe("我们刚刚都聊了一些什么内容呢");
  });

  it("strips inline markdown file links and trailing markers", () => {
    // 线上实拍：resource_link 被回放成 markdown 原文，且标记缀在同一行尾。
    const turns = mapLoadedTranscript([
      {
        role: "user",
        text: "[@Our.Beloved.Summer.2021.EP02.HD1080P.X264.AAC.Korean.CHS.Mp4er.mp4](file:///D:/movie/that/Our.Beloved.Summer.2021.EP02.mp4)【当前播放】\nCan Rotten Tomatoes data be scraped freely?",
      },
      { role: "agent", text: "不可以，条款禁止。" },
    ]);
    expect(turns).toHaveLength(1);
    expect(turns[0]?.userText).toBe(
      "Can Rotten Tomatoes data be scraped freely?",
    );
  });

  it("drops tool-id continuation lines from wrapped headers", () => {
    expect(
      stripTranscriptScaffolding(
        [
          "【工具优先】本轮优先使用 Lumina 本地工具",
          "lumina_get_playback_context（播放锚点）、lumina_get_library_context（剧集简介）",
          "这段讲了什么？",
        ].join("\n"),
      ),
    ).toBe("这段讲了什么？");
  });

  it("restores tool calls as done activities on their turn", () => {
    const turns = mapLoadedTranscript([
      { role: "user", text: "这段讲了什么？" },
      { role: "tool", text: "lumina_get_transcript_window 01:00" },
      { role: "tool", text: "lumina_get_transcript_window 01:00" },
      { role: "tool", text: "lumina_capture_frames f32" },
      { role: "agent", text: "这段讲离别。" },
    ]);
    expect(turns).toHaveLength(1);
    const tools =
      turns[0]?.activities.filter((item) => item.kind === "tool") ?? [];
    expect(tools).toHaveLength(3);
    // 同一调用的连续分块共用一个 call id，不虚增计数。
    expect(new Set(tools.map((item) => item.toolCallId)).size).toBe(2);
    expect(tools.every((item) => item.status === "completed")).toBe(true);
    expect(turns[0]?.showActivities).toBe(true);
    expect(turns[0]?.answer).toBe("这段讲离别。");
  });

  it("drops pure-echo agent events but keeps configured prefixes", () => {
    expect(
      stripAgentEchoes(
        "[@demo.mp4](file:///D:/movie/demo.mp4)【当前播放】",
      ),
    ).toBe("");
    expect(
      stripAgentEchoes(
        "Natural English: Summarize.\n\n这段主要讲重逢。",
      ),
    ).toBe("Natural English: Summarize.\n\n这段主要讲重逢。");
    const turns = mapLoadedTranscript([
      { role: "user", text: "目前没有可调用的工具吗？" },
      {
        role: "agent",
        text: "[@demo.mp4](file:///D:/movie/demo.mp4)【当前播放】",
      },
      { role: "agent", text: "有的，已经接好了。" },
    ]);
    expect(turns).toHaveLength(1);
    expect(turns[0]?.userText).toBe("目前没有可调用的工具吗？");
    expect(turns[0]?.answer).toBe("有的，已经接好了。");
  });
});
