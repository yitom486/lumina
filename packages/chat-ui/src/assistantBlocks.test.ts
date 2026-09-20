import { describe, expect, it } from "vitest";

import {
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
});
