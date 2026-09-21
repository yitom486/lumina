import { afterEach, describe, expect, it } from "vitest";

import {
  adaptShortcutOutput,
  normalizeRestoredShortcutOutput,
  resetTaskContractVersions,
  setTaskContractVersions,
} from "./shortcutOutput";

afterEach(() => {
  resetTaskContractVersions();
});

describe("adaptShortcutOutput", () => {
  it("adapts the versioned plot summary contract into a safe rich card", () => {
    const result = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({
        version: "plot_summary.v1",
        scope: { label: "当前观看范围" },
        summary: "主角在车站重新确认了调查方向。",
        evidence: ["字幕：我们必须回到车站。"],
      }),
    );

    expect(result?.blocks).toEqual([
      expect.objectContaining({
        kind: "watch-feed-card",
        title: "剧情梳理",
        eyebrow: "当前观看范围",
        summary: "主角在车站重新确认了调查方向。",
        bullets: ["字幕：我们必须回到车站。"],
      }),
    ]);
  });

  it("adapts outlook items without forwarding arbitrary object fields", () => {
    const result = adaptShortcutOutput(
      "chapter_outlook",
      JSON.stringify({
        version: "chapter_outlook.v1",
        items: [
          {
            title: "留意车站里的反应",
            private_debug_value: "must not render",
          },
        ],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      title: "后续看点",
      bullets: ["留意车站里的反应"],
    });
    expect(JSON.stringify(result)).not.toContain("private_debug_value");
  });

  it("renders the chapter recap contract with chapter scope, evidence, and uncertainty", () => {
    const result = adaptShortcutOutput(
      "chapter_recap",
      JSON.stringify({
        contract: "chapter_recap.v1",
        chapter: {
          title: "1792个夏日",
          position: "12:09",
        },
        spoiler_boundary: "current_position",
        recap: "崔雄与国延秀重新面对过去的关系。",
        evidence: [
          { ref: "[11:38]-[11:59]", fact: "两人讨论未来选择。" },
        ],
        uncertainty: ["当前台词窗口并不完整。"],
      }),
    );

    expect(result?.blocks).toEqual([
      expect.objectContaining({
        kind: "watch-feed-card",
        title: "本段总结",
        eyebrow: "1792个夏日 · 12:09",
        summary: "崔雄与国延秀重新面对过去的关系。",
        bullets: [
          "两人讨论未来选择。 · [11:38]-[11:59]",
          "待确认：当前台词窗口并不完整。",
        ],
      }),
    ]);
  });

  it("returns a readable business fallback for malformed or unknown output", () => {
    expect(
      adaptShortcutOutput("plot_summary", '{"version":"plot_summary.v1"}')
        ?.fallbackText,
    ).toBe("剧情梳理结果暂时无法展示，请稍后重试。");
    expect(
      adaptShortcutOutput(
        "plot_summary",
        JSON.stringify({ version: "plot_summary.v9", summary: "不应显示" }),
      )?.fallbackText,
    ).toBe("剧情梳理结果暂时无法展示，请稍后重试。");
  });

  it("leaves ordinary text to the normal Markdown renderer", () => {
    expect(adaptShortcutOutput("plot_summary", "普通回答")).toBeNull();
  });

  it("re-identifies only allowlisted restored contracts and hides unknown JSON", () => {
    const known = normalizeRestoredShortcutOutput(
      JSON.stringify({
        version: "plot_summary.v1",
        summary: "主线逐渐清晰。",
      }),
    );
    expect(known.shortcutTaskId).toBe("plot_summary");

    expect(
      normalizeRestoredShortcutOutput('{"version":"unknown.v1"}').answer,
    ).toBe("该结构化结果暂时无法展示，请稍后重试。");
    expect(normalizeRestoredShortcutOutput("普通回答")).toEqual({
      answer: "普通回答",
    });
  });

  it("binds backend contract versions instead of a second hardcoded copy", () => {
    setTaskContractVersions([
      { taskId: "plot_summary", outputContractVersion: "plot_summary.v2" },
    ]);
    expect(
      adaptShortcutOutput(
        "plot_summary",
        JSON.stringify({ version: "plot_summary.v1", summary: "旧版本" }),
      )?.fallbackText,
    ).toBe("剧情梳理结果暂时无法展示，请稍后重试。");
    const fresh = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({
        version: "plot_summary.v2",
        summary: "新版本",
        evidence: [],
      }),
    );
    expect(fresh?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      title: "剧情梳理",
    });
  });

  it("exposes two seek jumps for a ranged evidence segment", () => {
    const result = adaptShortcutOutput(
      "chapter_recap",
      JSON.stringify({
        version: "chapter_recap.v1",
        recap: "两人关系进展。",
        evidence: [{ ref: "[11:38]-[11:59]", fact: "两人讨论未来选择。" }],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      bullets: ["两人讨论未来选择。 · [11:38]-[11:59]"],
      actions: [
        {
          label: "跳转到 11:38",
          action: { type: "seek", anchor: { startMs: 698_000 } },
        },
        {
          label: "跳转到 11:59",
          action: { type: "seek", anchor: { startMs: 719_000 } },
        },
      ],
    });
  });

  it("exposes a single seek jump for one timestamp", () => {
    const result = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({
        version: "plot_summary.v1",
        summary: "主线推进。",
        evidence: [{ ref: "[12:32]", fact: "关键转折。" }],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      bullets: ["关键转折。 · [12:32]"],
      actions: [
        {
          label: "跳转到 12:32",
          action: { type: "seek", anchor: { startMs: 752_000 } },
        },
      ],
    });
  });

  it("supports hour timestamps", () => {
    const result = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({
        version: "plot_summary.v1",
        summary: "长片梳理。",
        evidence: [{ fact: "高潮戏", ref: "[1:02:03]" }],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      actions: [
        {
          label: "跳转到 1:02:03",
          action: { type: "seek", anchor: { startMs: 3_723_000 } },
        },
      ],
    });
  });

  it("keeps actions empty when evidence has no timestamp", () => {
    const result = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({
        version: "plot_summary.v1",
        summary: "主角在车站重新确认了调查方向。",
        evidence: ["字幕：我们必须回到车站。"],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      bullets: ["字幕：我们必须回到车站。"],
      actions: [],
    });
  });

  it("humanizes library_context without inventing mappings", () => {
    const result = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({
        version: "plot_summary.v1",
        summary: "背景梳理。",
        evidence: [
          { fact: "剧集背景介绍", ref: "library_context" },
          { fact: "未知来源事实", ref: "mystery_source" },
        ],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      bullets: ["剧集背景介绍 · 剧集简介", "未知来源事实 · mystery_source"],
      actions: [],
    });
    expect(JSON.stringify(result)).not.toContain("library_context");
  });
});
