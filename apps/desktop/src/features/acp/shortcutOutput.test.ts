import { afterEach, describe, expect, it, vi } from "vitest";

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
        JSON.stringify({ version: "chapter_recap.v1", recap: "不应显示" }),
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

  it("renders version-drifted but well-formed contracts instead of a generic fallback", () => {
    const drifted = JSON.stringify({
      version: "question_candidates.v2",
      questions: ["第一问？", { question: "第二问？", rationale: "依据" }],
    });
    const restored = normalizeRestoredShortcutOutput(drifted);
    // 版本不在注册表里，但同任务前缀认得出身份、形态完好：原样展示，不吞成“无法展示”。
    expect(restored.answer).toBe(drifted);
    expect(restored.shortcutTaskId).toBe("question_candidates");
  });

  it("renders the current question contract as a structured document", () => {
    const current = JSON.stringify({
      contract: "question_candidates.v1",
      questions: [{ prompt: "崔雄为什么拒绝？", reason: "动机" }],
    });
    const restored = normalizeRestoredShortcutOutput(current);
    expect(restored.answer).toBe(current);
    expect(restored.shortcutTaskId).toBe("question_candidates");
  });

  it("gives every question a one-click ask option", () => {
    const result = adaptShortcutOutput(
      "question_candidates",
      JSON.stringify({
        version: "question_candidates.v1",
        questions: ["第一问？", { question: "第二问？" }],
      }),
    );

    expect(result?.blocks).toEqual([
      expect.objectContaining({
        kind: "question-card",
        question: "第一问？",
        options: [
          expect.objectContaining({
            label: "直接问",
            action: { type: "ask", prompt: "第一问？" },
          }),
        ],
      }),
      expect.objectContaining({
        kind: "question-card",
        question: "第二问？",
      }),
    ]);
    // 第二张卡同样一点即问。
    const second = result?.blocks[1];
    expect(second).toMatchObject({
      kind: "question-card",
      options: [{ action: { type: "ask", prompt: "第二问？" } }],
    });
  });

  it("keeps the generic fallback (and logs the version) for unrenderable JSON", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      // 形似但非法的 JSON（尾逗号，模型常见毛病）：解析失败，只能给通用 fallback。
      expect(
        normalizeRestoredShortcutOutput(
          '{"version":"question_candidates.v1","questions":["第一问？"],}',
        ).answer,
      ).toBe("该结构化结果暂时无法展示，请稍后重试。");
      // 同任务漂移但无可渲染内容：任务级 fallback（比通用句更具体）+ 记版本号方便排查。
      expect(
        normalizeRestoredShortcutOutput('{"version":"question_candidates.v9"}')
          .answer,
      ).toBe("观众问题结果暂时无法展示，请稍后重试。");
      expect(warn).toHaveBeenCalledWith(
        expect.stringContaining("question_candidates.v9"),
      );
      // 前缀都认不出的版本：通用 fallback。
      expect(
        normalizeRestoredShortcutOutput('{"version":"mystery_task.v3"}')
          .answer,
      ).toBe("该结构化结果暂时无法展示，请稍后重试。");
      // 非对象 JSON（裸数组）：同样不暴露。
      expect(normalizeRestoredShortcutOutput('["第一问？"]').answer).toBe(
        "该结构化结果暂时无法展示，请稍后重试。",
      );
    } finally {
      warn.mockRestore();
    }
  });

  it("still rejects cross-task content instead of mislabeling it", () => {
    // 别家任务的 JSON 落到本任务槽位：前缀对不上，宁可 fallback 也不张冠李戴。
    expect(
      adaptShortcutOutput(
        "plot_summary",
        JSON.stringify({ version: "chapter_recap.v1", recap: "串味的内容" }),
      )?.fallbackText,
    ).toBe("剧情梳理结果暂时无法展示，请稍后重试。");
  });

  it("identifies drifted same-task versions on restore and warns once", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const drifted = normalizeRestoredShortcutOutput(
        JSON.stringify({ version: "plot_summary.v9", summary: "新版内容" }),
      );
      expect(drifted.shortcutTaskId).toBe("plot_summary");
      // 同任务无内容：任务级 fallback（不再是通用句），且记一次漂移。
      expect(
        normalizeRestoredShortcutOutput('{"version":"plot_summary.v9"}')
          .answer,
      ).toBe("剧情梳理结果暂时无法展示，请稍后重试。");
      normalizeRestoredShortcutOutput('{"version":"plot_summary.v9"}');
      expect(
        warn.mock.calls.filter(([message]) =>
          String(message).includes("plot_summary.v9"),
        ),
      ).toHaveLength(1);
    } finally {
      warn.mockRestore();
    }
  });

  it("binds backend contract versions instead of a second hardcoded copy", () => {
    setTaskContractVersions([
      { taskId: "plot_summary", outputContractVersion: "plot_summary.v2" },
    ]);
    // 注册表漂移到 v2，盖 v1 戳的内容照样渲染（向后兼容），不再整单吞掉。
    const drifted = adaptShortcutOutput(
      "plot_summary",
      JSON.stringify({ version: "plot_summary.v1", summary: "旧版本" }),
    );
    expect(drifted?.blocks[0]).toMatchObject({
      kind: "watch-feed-card",
      title: "剧情梳理",
    });
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
