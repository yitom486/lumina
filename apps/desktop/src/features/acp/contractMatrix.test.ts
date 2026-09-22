import { afterEach, describe, expect, it } from "vitest";

import type { AcpTaskId } from "./api";
import {
  adaptShortcutOutput,
  normalizeRestoredShortcutOutput,
  resetTaskContractVersions,
  setTaskContractVersions,
  SHORTCUT_TASK_LABELS,
} from "./shortcutOutput";

afterEach(() => {
  resetTaskContractVersions();
});

/**
 * 契约矩阵：升级再也不能悄悄吞功能。
 *
 * 背景：渲染曾严格依赖“戳版本 == 注册表版本”，后端发版/模型自带新戳/
 * 版本表异步到达三者任一错位，所有已渲染的卡片都会翻成“无法展示”。
 * 本矩阵锁死以下不变量（4 任务 × 版本变体 × 畸形）：
 * - 形态完好的内容：永远渲染（exact/drift/contract 别名都不吞）；
 * - 同任务无内容：任务级 fallback（告诉用户是哪个任务失败）；
 * - 跨任务串味/非法 JSON：fallback，绝不张冠李戴或暴露原文结构。
 */

const TASKS = [
  "chapter_recap",
  "chapter_outlook",
  "plot_summary",
  "question_candidates",
] as const satisfies readonly AcpTaskId[];

function minimalPayload(taskId: AcpTaskId, version: string): Record<string, unknown> {
  switch (taskId) {
    case "chapter_recap":
      return {
        version,
        chapter: { title: "1792个夏日" },
        recap: "两人重逢。",
        evidence: [{ ref: "[00:10]", fact: "重逢戏。" }],
      };
    case "chapter_outlook":
      return {
        version,
        items: ["留意车站里的反应"],
      };
    case "plot_summary":
      return {
        version,
        summary: "主线推进。",
        evidence: [{ ref: "[00:20]", fact: "关键转折。" }],
      };
    case "question_candidates":
      return {
        version,
        questions: ["崔雄为什么拒绝？"],
      };
  }
}

function taskFallback(taskId: AcpTaskId): string {
  return `${SHORTCUT_TASK_LABELS[taskId]}结果暂时无法展示，请稍后重试。`;
}

const GENERIC_FALLBACK = "该结构化结果暂时无法展示，请稍后重试。";

describe("contract matrix: well-formed content always renders", () => {
  for (const taskId of TASKS) {
    it(`${taskId}: exact version, contract alias and drifted versions all render`, () => {
      const base = minimalPayload(taskId, `${taskId}.v1`);

      // 精确版本。
      expect(adaptShortcutOutput(taskId, JSON.stringify(base))?.blocks.length).toBeGreaterThan(0);
      const restoredExact = normalizeRestoredShortcutOutput(JSON.stringify(base));
      expect(restoredExact.answer).toBe(JSON.stringify(base));
      expect(restoredExact.shortcutTaskId).toBe(taskId);

      // contract 别名字段。
      const { version, ...rest } = base;
      void version;
      const aliased = JSON.stringify({ contract: `${taskId}.v1`, ...rest });
      expect(adaptShortcutOutput(taskId, aliased)?.blocks.length).toBeGreaterThan(0);
      expect(normalizeRestoredShortcutOutput(aliased).shortcutTaskId).toBe(taskId);

      // 双向漂移：旧内容 + 新注册表，新内容 + 旧注册表，都渲染。
      const drifted = JSON.stringify({ ...base, version: `${taskId}.v9` });
      expect(adaptShortcutOutput(taskId, drifted)?.blocks.length).toBeGreaterThan(0);
      const restoredDrifted = normalizeRestoredShortcutOutput(drifted);
      expect(restoredDrifted.answer).toBe(drifted);
      expect(restoredDrifted.shortcutTaskId).toBe(taskId);

      setTaskContractVersions([
        { taskId, outputContractVersion: `${taskId}.v2` },
      ]);
      try {
        const stale = JSON.stringify(base);
        expect(adaptShortcutOutput(taskId, stale)?.blocks.length).toBeGreaterThan(0);
        expect(normalizeRestoredShortcutOutput(stale).answer).toBe(stale);
      } finally {
        resetTaskContractVersions();
      }
    });
  }
});

describe("contract matrix: failures stay specific and never leak structure", () => {
  for (const taskId of TASKS) {
    it(`${taskId}: empty content falls back to the task-specific message`, () => {
      expect(
        adaptShortcutOutput(
          taskId,
          JSON.stringify({ version: `${taskId}.v1` }),
        )?.fallbackText,
      ).toBe(taskFallback(taskId));
      expect(
        normalizeRestoredShortcutOutput(
          JSON.stringify({ version: `${taskId}.v9` }),
        ).answer,
      ).toBe(taskFallback(taskId));
    });
  }

  it("cross-task content is rejected instead of mislabeled", () => {
    const cases: Array<{ taskId: AcpTaskId; payload: Record<string, unknown> }> = [
      {
        taskId: "plot_summary",
        payload: { version: "chapter_recap.v1", recap: "串味的内容" },
      },
      {
        taskId: "chapter_recap",
        payload: { version: "plot_summary.v1", summary: "串味的内容" },
      },
      {
        taskId: "plot_summary",
        payload: { version: "question_candidates.v1", questions: ["串味的问题？"] },
      },
      {
        taskId: "question_candidates",
        payload: { version: "plot_summary.v1", summary: "串味的内容" },
      },
    ];
    for (const { taskId, payload } of cases) {
      expect(
        adaptShortcutOutput(taskId, JSON.stringify(payload))?.fallbackText,
      ).toBe(taskFallback(taskId));
    }
  });

  it("malformed envelopes fall back without exposing structure", () => {
    // 非法 JSON（形似）：通用 fallback。
    expect(
      normalizeRestoredShortcutOutput('{"version":"plot_summary.v1","summary":,}')
        .answer,
    ).toBe(GENERIC_FALLBACK);
    // 非对象 JSON：通用 fallback。
    expect(normalizeRestoredShortcutOutput('["第一问？"]').answer).toBe(
      GENERIC_FALLBACK,
    );
    // 前缀都认不出的版本：通用 fallback。
    expect(
      normalizeRestoredShortcutOutput('{"version":"mystery_task.v3","x":1}')
        .answer,
    ).toBe(GENERIC_FALLBACK);
    // 无版本无内容的对象：通用 fallback（不猜身份）。
    expect(normalizeRestoredShortcutOutput('{"notes":"随手记"}').answer).toBe(
      GENERIC_FALLBACK,
    );
    // 普通文本：原样走 Markdown。
    expect(normalizeRestoredShortcutOutput("普通回答")).toEqual({
      answer: "普通回答",
    });
  });
});
