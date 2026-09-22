import { describe, expect, it } from "vitest";

import {
  describeJsonShape,
  normalizeAssistantBlocks,
  normalizeAssistantBlocksWithNotices,
  parseAssistantBlocksText,
} from "./assistantBlocks";

describe("normalizeAssistantBlocks", () => {
  it("normalizes the closed rich-output block set", () => {
    const blocks = normalizeAssistantBlocks({
      blocks: [
        { type: "markdown", text: "## 叙述" },
        {
          kind: "transcript_quote",
          quote: "我们现在出发。",
          speaker: "角色 A",
          startMs: 12_500,
        },
        {
          kind: "timeline",
          title: "关键节点",
          items: [{ atMs: 0, title: "开场" }],
        },
        {
          kind: "watch_feed_card",
          title: "当前观察",
          summary: "注意角色之间的关系变化。",
          spoilerLevel: "current",
          actions: [
            {
              label: "跳转",
              action: { type: "seek", startMs: 42_000 },
            },
          ],
        },
        {
          kind: "question_card",
          question: "你注意到什么？",
          options: [
            {
              label: "继续追问",
              action: { type: "ask", prompt: "请解释这个变化。", chapterId: "ch-1" },
            },
          ],
        },
        {
          kind: "agent_task_status",
          taskId: "task-1",
          title: "生成章节",
          status: "running",
          progress: 120,
        },
        {
          kind: "action_chip",
          label: "保存笔记",
          action: { type: "save-note", text: "重要线索", startMs: 3_000 },
        },
      ],
    });

    expect(blocks.map((block) => block.kind)).toEqual([
      "narrative",
      "transcript-quote",
      "timeline",
      "watch-feed-card",
      "question-card",
      "agent-task-status",
      "action-chip",
    ]);
    expect(blocks[5]).toMatchObject({ kind: "agent-task-status", progress: 100 });
    expect(blocks[3]).toMatchObject({
      kind: "watch-feed-card",
      actions: [{ action: { type: "seek", anchor: { startMs: 42_000 } } }],
    });
    expect(blocks[4]).toMatchObject({
      kind: "question-card",
      options: [
        {
          action: {
            type: "ask",
            anchor: { chapterId: "ch-1" },
            prompt: "请解释这个变化。",
          },
        },
      ],
    });
  });

  it("keeps anchorless ask actions for one-click questions", () => {
    const blocks = normalizeAssistantBlocks({
      blocks: [
        {
          kind: "question-card",
          question: "崔雄为什么拒绝？",
          options: [{ label: "直接问", action: { type: "ask", prompt: "崔雄为什么拒绝？" } }],
        },
        {
          kind: "action-chip",
          label: "空问题",
          action: { type: "ask" },
        },
      ],
    });

    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({
      kind: "question-card",
      options: [
        {
          label: "直接问",
          action: { type: "ask", prompt: "崔雄为什么拒绝？" },
        },
      ],
    });
    // 无 prompt 的 ask 照样丢掉，不会产生空按钮。
    expect(JSON.stringify(blocks)).not.toContain("空问题");
  });

  it("falls back incomplete or unknown structured blocks to safe narrative text", () => {
    const result = normalizeAssistantBlocksWithNotices({
      blocks: [
        {
          type: "watch-feed-card",
          complete: false,
          title: "半截卡片",
          text: "正在生成内容…",
        },
        { type: "not-allowlisted", text: "未知组件不要直接渲染" },
        { type: "not-allowlisted", component: "DangerousComponent" },
      ],
    });

    expect(result.blocks).toEqual([
      {
        kind: "narrative",
        id: "fallback-0",
        format: "markdown",
        markdown: "正在生成内容…",
      },
      {
        kind: "narrative",
        id: "fallback-1",
        format: "markdown",
        markdown: "未知组件不要直接渲染",
      },
    ]);
    expect(result.notices).toEqual([
      { index: 0, reason: "incomplete", fallback: true },
      { index: 1, reason: "unknown-kind", fallback: true },
      { index: 2, reason: "unknown-kind", fallback: false },
    ]);
  });

  it("drops unsafe actions and malformed anchors", () => {
    const blocks = normalizeAssistantBlocks({
      blocks: [
        {
          kind: "action-chip",
          label: "任意组件",
          action: { type: "react-component", component: "Button", startMs: 1 },
        },
        {
          kind: "action-chip",
          label: "反向时间轴",
          action: { type: "seek", startMs: 20, endMs: 10 },
        },
        {
          kind: "transcript-quote",
          quote: "没有来源不能作为引用。",
        },
      ],
    });

    expect(blocks).toEqual([]);
  });

  it("treats explicit streaming status as incomplete only when streaming", () => {
    const blocks = normalizeAssistantBlocks(
      { kind: "narrative", status: "partial", text: "临时文本" },
      { streaming: true },
    );

    expect(blocks).toEqual([
      {
        kind: "narrative",
        id: "fallback-0",
        format: "markdown",
        markdown: "临时文本",
      },
    ]);
  });

  it("parses complete JSON and json fences without guessing Markdown", () => {
    const payload = { blocks: [{ kind: "narrative", text: "结构化内容" }] };
    const json = parseAssistantBlocksText(JSON.stringify(payload));
    const fenced = parseAssistantBlocksText(`\`\`\`json\n${JSON.stringify(payload)}\n\`\`\``);

    expect(json?.blocks[0]).toMatchObject({ kind: "narrative", markdown: "结构化内容" });
    expect(fenced?.blocks[0]).toMatchObject({ kind: "narrative", markdown: "结构化内容" });
    expect(parseAssistantBlocksText("普通 Markdown {不是完整 JSON}")).toBeNull();
  });

  it("parses a versioned contract into a rich document instead of exposing JSON", () => {
    const result = parseAssistantBlocksText(
      JSON.stringify({
        contract: "chapter_recap.v1",
        chapter: {
          season: 1,
          episode: 2,
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
      {
        kind: "structured-result",
        id: "structured-0",
        contract: "chapter_recap.v1",
        title: "本段总结",
        metadata: [
          { id: "metadata-0", label: "季", value: "1" },
          { id: "metadata-1", label: "集", value: "2" },
          { id: "metadata-2", label: "章节", value: "1792个夏日" },
          { id: "metadata-3", label: "位置", value: "12:09" },
          { id: "metadata-4", label: "剧透边界", value: "截至当前播放位置" },
        ],
        summary: "崔雄与国延秀重新面对过去的关系。",
        sections: [
          {
            id: "section-0",
            title: "证据",
            paragraphs: [],
            items: ["两人讨论未来选择。 · [11:38]-[11:59]"],
          },
          {
            id: "section-1",
            title: "待确认",
            paragraphs: [],
            items: ["当前台词窗口并不完整。"],
          },
        ],
        spoilerLevel: "current",
        actions: [
          {
            id: "structured-0-seek-0",
            label: "跳转到 11:38",
            action: { type: "seek", anchor: { startMs: 698_000 } },
          },
          {
            id: "structured-0-seek-1",
            label: "跳转到 11:59",
            action: { type: "seek", anchor: { startMs: 719_000 } },
          },
        ],
      },
    ]);
  });

  it("derives one-click ask chips from contract questions", () => {
    const result = parseAssistantBlocksText(
      JSON.stringify({
        contract: "question_candidates.v1",
        questions: ["第一问？", "第二问？"],
      }),
    );

    expect(result?.blocks[0]).toMatchObject({
      kind: "structured-result",
      actions: [
        {
          id: "structured-0-ask-0",
          label: "问：第一问？",
          action: { type: "ask", prompt: "第一问？" },
        },
        {
          id: "structured-0-ask-1",
          label: "问：第二问？",
          action: { type: "ask", prompt: "第二问？" },
        },
      ],
    });
  });

  it("returns null for malformed, empty, or incomplete structured payloads", () => {
    expect(parseAssistantBlocksText('{"blocks":')).toBeNull();
    expect(parseAssistantBlocksText('{"blocks":[]}')).toBeNull();
    expect(
      parseAssistantBlocksText(
        JSON.stringify({ blocks: [{ kind: "watch-feed-card", status: "streaming" }] }),
        { streaming: true },
      ),
    ).toBeNull();
  });

  it("refuses metadata-only hollow cards instead of rendering an empty shell", () => {
    // 只有章节元数据、没有任何正文条目：上层应走任务级 fallback 诚实失败，
    // 而不是摆出只有标题和元数据的空壳（用户会问“内容呢”）。
    expect(
      parseAssistantBlocksText(
        JSON.stringify({
          contract: "chapter_outlook.v1",
          chapter: { title: "1792个夏日" },
          spoiler_boundary: "current_position",
        }),
      ),
    ).toBeNull();
  });
});

describe("describeJsonShape", () => {
  it("reports keys and shapes without leaking values", () => {
    const shape = describeJsonShape({
      version: "chapter_outlook.v1",
      chapter: { title: "机密章节名" },
      highlights: ["第一条机密", "第二条机密"],
      count: 3,
    });

    expect(shape).toContain("version:string(");
    expect(shape).toContain("chapter:object");
    expect(shape).toContain("highlights:array[2]");
    expect(shape).toContain("count:number");
    expect(shape).not.toContain("机密");
    expect(shape).not.toContain("chapter_outlook");
  });

  it("handles non-object envelopes", () => {
    expect(describeJsonShape(["a", "b"])).toBe("array[2]");
    expect(describeJsonShape(null)).toBe("object");
    expect(describeJsonShape(42)).toBe("number");
  });
});
