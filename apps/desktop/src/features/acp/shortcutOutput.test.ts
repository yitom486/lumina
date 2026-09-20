import { describe, expect, it } from "vitest";

import {
  adaptShortcutOutput,
  normalizeRestoredShortcutOutput,
} from "./shortcutOutput";

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
});
