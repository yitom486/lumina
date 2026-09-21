import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { getAcpWatchFeed, type AcpWatchFeedItem } from "../api";
import { WatchFeedView } from "./WatchFeedView";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return { ...actual, getAcpWatchFeed: vi.fn() };
});

afterEach(() => {
  cleanup();
  usePlayerStore.setState({ currentFile: null, currentTimeMs: 0 });
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

function makeItem(
  id: number,
  content: string,
  startMs: number,
  itemType: string,
  chapterId = id,
): AcpWatchFeedItem {
  return {
    id,
    episodeId: 2,
    chapterId,
    revisionId: null,
    taskId: null,
    itemType,
    source: "ai",
    content,
    spoilerLevel: "current_chapter",
    contentVersion: "v1",
    publishedAtMs: id,
    chapter: {
      id: chapterId,
      startMs,
      endMs: startMs + 10_000,
      spoilerLevel: "current_chapter",
      title: `章节 ${chapterId}`,
      mainline: null,
      status: "ready",
    },
    revision: null,
    questionCandidate: null,
    screenshotRefs: [],
    coverRef: null,
  };
}

describe("WatchFeedView", () => {
  it("shows a compact empty state when no durable projection exists", () => {
    renderFeed({ onSelectTask: vi.fn() });

    expect(screen.getByText("AI 观剧流已就绪")).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "AI 观剧流" })).toBeInTheDocument();
  });

  it("shows watch-feed shortcuts and emits a stable task id", async () => {
    const onSelectTask = vi.fn();
    renderFeed({ onSelectTask });

    expect(screen.getByRole("button", { name: "本段总结" })).toBeInTheDocument();
    await screen.getByRole("button", { name: "本段总结" }).click();
    expect(onSelectTask).toHaveBeenCalledWith("chapter_recap");
  });

  it("renders one replaceable card per slot and filters future chapters", async () => {
    usePlayerStore.setState({
      currentFile: "C:\\videos\\episode-01.mp4",
      currentTimeMs: 10_000,
    });
    vi.mocked(getAcpWatchFeed).mockResolvedValue({
      source: "sqlite",
      items: [
        makeItem(1, "当前观剧记录", 0, "chapter", 1),
        makeItem(
          2,
          JSON.stringify({
            version: "chapter_recap.v1",
            scope: { label: "已观看内容" },
            recap: "已格式化的前情提要",
            evidence: ["字幕证据"],
          }),
          0,
          "recap",
          1,
        ),
        makeItem(3, "本段需要留意人物的选择", 0, "watch_point", 1),
        makeItem(4, "尚未播放的章节", 30_000, "chapter", 2),
      ],
    });

    renderFeed({ onSelectTask: vi.fn() });

    expect(await screen.findByText("当前观剧记录")).toBeInTheDocument();
    expect(screen.queryByText("尚未播放的章节")).not.toBeInTheDocument();
    expect(document.querySelectorAll("[data-watch-feed-slot]")).toHaveLength(3);

    fireEvent.click(screen.getByRole("button", { name: /前情提要/ }));
    expect(await screen.findByText("已格式化的前情提要")).toBeInTheDocument();
    expect(screen.queryByText(/chapter_recap\.v1/)).not.toBeInTheDocument();

    usePlayerStore.setState({ currentTimeMs: 30_000 });

    expect(await screen.findByText("尚未播放的章节")).toBeInTheDocument();
    expect(screen.queryByText("当前观剧记录")).not.toBeInTheDocument();
  });

  it("prefers SQLite content and keeps asset references non-visual metadata", async () => {
    usePlayerStore.setState({
      currentFile: "C:\\videos\\episode-01.mp4",
      currentTimeMs: 10_000,
    });
    vi.mocked(getAcpWatchFeed).mockResolvedValue({
      source: "sqlite",
      items: [
        {
          ...makeItem(7, "已保存的章节主线", 10_000, "chapter", 3),
          revisionId: 4,
          taskId: 5,
          screenshotRefs: ["opaque-frame-1"],
          coverRef: "opaque-cover-1",
        },
      ],
    });

    renderFeed({ onSelectTask: vi.fn() });

    expect(await screen.findByText("已保存的章节主线")).toBeInTheDocument();
    expect(screen.getByText(/含章节封面引用/)).toBeInTheDocument();
    expect(screen.getByText(/含 1 个画面引用/)).toBeInTheDocument();
  });

  it("keeps the watch feed compact when the SQLite read fails", async () => {
    usePlayerStore.setState({ currentFile: "C:\\videos\\episode-01.mp4" });
    vi.mocked(getAcpWatchFeed).mockRejectedValue(new Error("read failed"));

    renderFeed({ onSelectTask: vi.fn() });

    expect(
      await screen.findByText("本地观剧流暂时不可用，聊天仍可继续使用。"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本段总结" })).toBeInTheDocument();
  });

  it("shows a safe empty state for an empty SQLite projection", async () => {
    usePlayerStore.setState({ currentFile: "C:\\videos\\episode-01.mp4" });
    vi.mocked(getAcpWatchFeed).mockResolvedValue({ source: "empty", items: [] });

    renderFeed({ onSelectTask: vi.fn() });

    expect(await screen.findByText("AI 观剧流已就绪")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "剧情梳理" })).toBeInTheDocument();
  });
});
