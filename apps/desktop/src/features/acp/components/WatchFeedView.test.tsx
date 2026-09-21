import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { getAcpWatchFeed } from "../api";
import type { ChatTurn } from "../types";
import { WatchFeedView } from "./WatchFeedView";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return { ...actual, getAcpWatchFeed: vi.fn() };
});

afterEach(() => {
  cleanup();
  usePlayerStore.setState({ currentFile: null });
  vi.mocked(getAcpWatchFeed).mockReset();
});

function renderFeed(props: ComponentProps<typeof WatchFeedView>) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <WatchFeedView {...props} />
    </QueryClientProvider>,
  );
}

function makeTurn(partial: Partial<ChatTurn> & Pick<ChatTurn, "id">): ChatTurn {
  return {
    userText: "用户问题",
    answer: "",
    status: "streaming",
    activities: [],
    showActivities: true,
    ...partial,
  };
}

describe("WatchFeedView", () => {
  it("shows a non-session empty state when there are no turns", () => {
    renderFeed({
      turns: [],
      notices: [],
      followEnd: false,
      onSelectTask: vi.fn(),
    });

    expect(screen.getByText("AI 观剧流已就绪")).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "AI 观剧流" })).toBeInTheDocument();
  });

  it("shows watch-feed shortcuts and emits a stable task id", async () => {
    const onSelectTask = vi.fn();
    renderFeed({
      turns: [],
      notices: [],
      followEnd: false,
      onSelectTask,
    });

    expect(screen.getByRole("button", { name: "本段总结" })).toBeInTheDocument();
    await screen.getByRole("button", { name: "本段总结" }).click();
    expect(onSelectTask).toHaveBeenCalledWith("chapter_recap");
  });

  it("keeps the real turn anchor visible in the feed card", async () => {
    renderFeed({
      turns: [
        makeTurn({
          id: "turn-1",
          answer: "当前片段的分析",
          status: "done",
          anchorMs: 125_000,
          activities: [
            {
              id: "tool-1",
              kind: "tool",
              title: "读取当前台词",
              status: "completed",
            },
          ],
        }),
      ],
      notices: [],
      followEnd: false,
      onSelectTask: vi.fn(),
    });

    expect(screen.getByText("2:05")).toBeInTheDocument();
    expect(screen.getByText("1 个工具活动 · 已完成")).toBeInTheDocument();
    expect(await screen.findByText("当前片段的分析")).toBeInTheDocument();
  });

  it("prefers SQLite items over the temporary ACP-turn fallback", async () => {
    usePlayerStore.setState({ currentFile: "C:\\videos\\episode-01.mp4" });
    vi.mocked(getAcpWatchFeed).mockResolvedValue({
      source: "sqlite",
      items: [
        {
          id: 7,
          episodeId: 2,
          chapterId: 3,
          revisionId: 4,
          taskId: 5,
          itemType: "chapter",
          source: "ai",
          content: "已保存的章节主线",
          spoilerLevel: "current_chapter",
          contentVersion: "v1",
          publishedAtMs: 1,
          chapter: {
            id: 3,
            startMs: 10_000,
            endMs: 20_000,
            spoilerLevel: "current_chapter",
            title: "初见",
            mainline: "主线",
            status: "ready",
          },
          revision: null,
          questionCandidate: null,
          screenshotRefs: ["opaque-frame-1"],
          coverRef: "opaque-cover-1",
        },
      ],
    });

    renderFeed({
      turns: [makeTurn({ id: "turn-1", answer: "不应显示的临时记录" })],
      notices: [],
      followEnd: false,
      onSelectTask: vi.fn(),
    });

    expect(await screen.findByText("已保存的章节主线")).toBeInTheDocument();
    expect(screen.queryByText("不应显示的临时记录")).not.toBeInTheDocument();
    expect(screen.getByText(/含章节封面引用/)).toBeInTheDocument();
    expect(screen.getByText(/含 1 个画面引用/)).toBeInTheDocument();
  });

  it("keeps ACP turns as an explicit fallback when the SQLite read fails", async () => {
    usePlayerStore.setState({ currentFile: "C:\\videos\\episode-01.mp4" });
    vi.mocked(getAcpWatchFeed).mockRejectedValue(new Error("read failed"));

    renderFeed({
      turns: [makeTurn({ id: "turn-1", answer: "实时会话记录" })],
      notices: [],
      followEnd: false,
      onSelectTask: vi.fn(),
    });

    expect(
      await screen.findByText("本地观剧流暂时不可用，当前显示本次会话的临时记录。"),
    ).toBeInTheDocument();
    expect(await screen.findByText("实时会话记录")).toBeInTheDocument();
  });

  it("shows a safe empty state for an empty SQLite projection", async () => {
    usePlayerStore.setState({ currentFile: "C:\\videos\\episode-01.mp4" });
    vi.mocked(getAcpWatchFeed).mockResolvedValue({ source: "empty", items: [] });

    renderFeed({
      turns: [],
      notices: [],
      followEnd: false,
      onSelectTask: vi.fn(),
    });

    expect(
      await screen.findByText("暂无已保存的观剧条目；本次会话结果会安全显示在这里。"),
    ).toBeInTheDocument();
    expect(screen.getByText("AI 观剧流已就绪")).toBeInTheDocument();
  });
});
