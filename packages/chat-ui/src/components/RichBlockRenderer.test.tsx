import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import type { AssistantBlock } from "../assistantBlocks";
import { RichBlockRenderer } from "./RichBlockRenderer";

describe("RichBlockRenderer", () => {
  it("renders only fixed block projections and delegates markdown", () => {
    const renderMarkdown = vi.fn((markdown: string) => <strong>{markdown}</strong>);
    const blocks: AssistantBlock[] = [
      { kind: "narrative", id: "n-1", format: "markdown", markdown: "叙述" },
      {
        kind: "transcript-quote",
        id: "q-1",
        quote: "字幕引用",
        anchor: { startMs: 30_000 },
      },
      {
        kind: "timeline",
        id: "t-1",
        title: "时间线",
        items: [{ id: "t-1-0", atMs: 0, title: "开始" }],
      },
      {
        kind: "agent-task-status",
        id: "a-1",
        taskId: "task-1",
        title: "章节任务",
        status: "running",
        progress: 45,
      },
    ];

    const markup = renderToStaticMarkup(
      <RichBlockRenderer blocks={blocks} renderMarkdown={renderMarkdown} />,
    );

    expect(renderMarkdown).toHaveBeenCalledWith("叙述");
    expect(markup).toContain("<strong>叙述</strong>");
    expect(markup).toContain("字幕引用");
    expect(markup).toContain("时间线");
    expect(markup).toContain("处理中");
  });

  it("keeps raw HTML as text when no markdown renderer is injected", () => {
    const markup = renderToStaticMarkup(
      <RichBlockRenderer
        blocks={[
          {
            kind: "narrative",
            id: "n-1",
            format: "markdown",
            markdown: "<img src=x onerror=alert(1)>",
          },
        ]}
      />,
    );

    expect(markup).toContain("&lt;img src=x onerror=alert(1)&gt;");
    expect(markup).not.toContain("<img");
  });

  it("guards future spoilers before an explicit reveal action", () => {
    const markup = renderToStaticMarkup(
      <RichBlockRenderer
        blocks={[
          {
            kind: "watch-feed-card",
            id: "feed-1",
            title: "观剧提示",
            summary: "后续内容",
            bullets: [],
            spoilerLevel: "future",
            actions: [
              {
                id: "seek-1",
                label: "跳转来源",
                action: { type: "seek", anchor: { startMs: 90_000 } },
              },
            ],
          },
        ]}
      />,
    );

    expect(markup).toContain("内容含后续剧透");
    expect(markup).toContain("查看内容");
    expect(markup).not.toContain("后续内容");
    expect(markup).not.toContain("跳转来源");
  });
});
