import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { ytdlResolveKey } from "@lumina/query-keys";

import { ChaptersPanel, type ChaptersPanelProps } from "./ChaptersPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

let chapters: unknown[] = [];
let onlineResolve: unknown = null;
let libraryContext: unknown = null;
const segmentationSnapshot = {
  id: 1,
  taskKey: "chapter-segmentation:C:\\v\\a.mp4:C:\\v\\a.mp4",
  taskType: "chapter_segmentation",
  episodeId: null,
  chapterId: null,
  episodeIdentity: { kind: "legacy", reason: "metadata_unavailable" },
  status: "pending",
  sessionId: null,
  promptVersion: "1.0",
  outputContractVersion: "chapter_segment.v1",
  attemptCount: 0,
  retryCount: 0,
  maxAttempts: 3,
  failureCode: null,
  failureMessage: null,
  validationSummary: null,
  canRetry: false,
  retryAction: null,
  agentConfigured: true,
  createdAtMs: 1,
  updatedAtMs: 1,
};

function renderPanel(props: ChaptersPanelProps = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ChaptersPanel {...props} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  chapters = [];
  onlineResolve = null;
  libraryContext = null;
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "media_inspect")
      return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters });
    if (cmd === "ytdl_cached_resolve") return Promise.resolve(onlineResolve);
    if (cmd === "library_context_for_media") return Promise.resolve(libraryContext);
    if (cmd === "chapter_segmentation_status")
      return Promise.reject({ code: "NotFound", message: "尚未开始该媒体的 AI 分段" });
    if (cmd === "chapter_segmentation_start")
      return Promise.resolve(segmentationSnapshot);
    if (cmd === "player_seek")
      return Promise.resolve({ status: "Paused", currentTimeMs: 0 });
    return Promise.resolve(null);
  });
  usePlayerStore.setState({
    currentFile: "C:\\v\\a.mp4",
    sourceKind: null,
    status: "Paused",
    currentTimeMs: 500,
  });
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("ChaptersPanel container chapters and AI entry", () => {
  it("shows the no-chapter reminder and does not auto-start AI segmentation by default", async () => {
    const onStartAiSegmentation = vi.fn();
    renderPanel({ onStartAiSegmentation });

    await waitFor(() => {
      expect(screen.getByText("该文件暂无容器章节。")).toBeInTheDocument();
    });

    const button = screen.getByRole("button", { name: "开始 AI 分段" });
    expect(button).not.toBeDisabled();
    expect(onStartAiSegmentation).not.toHaveBeenCalled();
  });

  it("shows real container chapters and seeks when one is clicked", async () => {
    chapters = [
      { id: 1, startMs: 0, endMs: 10_000, title: "正片" },
      { id: 2, startMs: 30_000, endMs: null, title: "结尾" },
    ];
    renderPanel();

    await waitFor(() => {
      expect(screen.getByText("正片")).toBeInTheDocument();
    });
    expect(screen.getByText("结尾")).toBeInTheDocument();
    expect(screen.queryByText("该文件暂无容器章节。")).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("结尾"));
    await waitFor(() => {
      expect(invokeCmds("player_seek")).toHaveLength(1);
    });
  });

  it("starts AI segmentation when the ready entry is clicked", async () => {
    const onStartAiSegmentation = vi.fn();
    renderPanel({
      aiSegmentationStatus: "ready",
      onStartAiSegmentation,
    });

    await waitFor(() => {
      expect(screen.getByText("该文件暂无容器章节。")).toBeInTheDocument();
    });

    const button = screen.getByRole("button", { name: "开始 AI 分段" });
    expect(button).not.toBeDisabled();
    fireEvent.click(button);
    expect(onStartAiSegmentation).toHaveBeenCalledTimes(1);
  });

  it.each([
    { status: "pending" as const, buttonName: "已排队，等待执行…" },
    { status: "unavailable" as const, buttonName: "开始 AI 分段" },
  ])(
    "does not start AI segmentation when the status is $status",
    async ({ status, buttonName }) => {
      const onStartAiSegmentation = vi.fn();
      renderPanel({ aiSegmentationStatus: status, onStartAiSegmentation });

      await waitFor(() => {
        expect(screen.getByText("该文件暂无容器章节。")).toBeInTheDocument();
      });

      const button = screen.getByRole("button", { name: buttonName });
      expect(button).toBeDisabled();
      fireEvent.click(button);
      expect(onStartAiSegmentation).not.toHaveBeenCalled();
    },
  );

  it("renders a persisted pending task and does not start it again", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "chapter_segmentation_status")
        return Promise.resolve(segmentationSnapshot);
      return Promise.resolve(null);
    });

    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "已排队，等待执行…" })).toBeDisabled();
    });
    expect(invokeCmds("chapter_segmentation_start")).toHaveLength(0);
    expect(
      screen.getByText(
        "任务已保存，章节 Agent 正在独立执行；不会写入自由聊天，也不会自动重复启动。",
      ),
    ).toBeInTheDocument();
  });

  it("shows a safe failure reason and offers a durable retry within the budget", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "chapter_segmentation_status")
        return Promise.resolve({
          ...segmentationSnapshot,
          status: "validation_failure",
          attemptCount: 1,
          retryCount: 3,
          failureCode: "ValidationFailed",
          failureMessage: "章节分段结果未通过校验。",
          validationSummary: "章节结果缺少必要内容。",
          canRetry: true,
          retryAction: "retry",
        });
      if (cmd === "chapter_segmentation_start")
        return Promise.resolve(segmentationSnapshot);
      return Promise.resolve(null);
    });

    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "再次尝试" })).toBeEnabled();
    });
    expect(screen.getByText("章节分段结果未通过校验。")).toBeInTheDocument();
    expect(screen.getByText("章节结果缺少必要内容。")).toBeInTheDocument();
    expect(screen.getByText("已尝试 1 / 3 次。")).toBeInTheDocument();
    expect(screen.getByText(/不会写入自由聊天/)).toBeInTheDocument();
    expect(screen.queryByText(/stderr|JSON-RPC|sqlite|C:\\v\\a\.mp4/i)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "再次尝试" }));
    await waitFor(() => {
      expect(invokeCmds("chapter_segmentation_start")).toHaveLength(1);
    });
  });

  it("shows the attempt limit and disables retry after a terminal failure", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "chapter_segmentation_status")
        return Promise.resolve({
          ...segmentationSnapshot,
          status: "failed",
          attemptCount: 3,
          retryCount: 3,
          failureCode: "ValidationFailed",
          failureMessage: "章节分段结果未通过校验。",
          validationSummary: "章节时间范围或顺序不符合视频时间轴。",
          canRetry: false,
          retryAction: null,
        });
      return Promise.resolve(null);
    });

    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "AI 分段失败" })).toBeDisabled();
    });
    expect(screen.getByText("已尝试 3 / 3 次，已达到尝试上限。")).toBeInTheDocument();
    expect(screen.getByText(/已达到自动校验和作业尝试上限/)).toBeInTheDocument();
  });

  it("asks for Agent configuration when a persisted task is waiting without one", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "chapter_segmentation_status")
        return Promise.resolve({
          ...segmentationSnapshot,
          status: "pending",
          agentConfigured: false,
          retryAction: "configure_agent",
        });
      return Promise.resolve(null);
    });

    renderPanel();

    await waitFor(() => {
      expect(screen.getByText(/尚未配置可用的 AI Agent/)).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "已排队，等待执行…" })).toBeDisabled();
  });

  it("prevents duplicate starts while the start mutation is pending", async () => {
    let resolveStart: ((value: unknown) => void) | undefined;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "chapter_segmentation_status")
        return Promise.reject({ code: "NotFound", message: "尚未开始该媒体的 AI 分段" });
      if (cmd === "chapter_segmentation_start")
        return new Promise((resolve) => {
          resolveStart = resolve;
        });
      return Promise.resolve(null);
    });

    renderPanel();
    const button = await screen.findByRole("button", { name: "开始 AI 分段" });
    fireEvent.click(button);

    await waitFor(() => expect(button).toBeDisabled());
    fireEvent.click(button);
    expect(invokeCmds("chapter_segmentation_start")).toHaveLength(1);

    resolveStart?.(segmentationSnapshot);
  });

  it("waits for library identity before querying or starting segmentation", async () => {
    let resolveContext: ((value: unknown) => void) | undefined;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "library_context_for_media")
        return new Promise((resolve) => {
          resolveContext = resolve;
        });
      return Promise.resolve(null);
    });

    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("该文件暂无容器章节。")).toBeInTheDocument();
    });
    const button = screen.getByRole("button", { name: "开始 AI 分段" });
    expect(button).toBeDisabled();
    expect(invokeCmds("chapter_segmentation_status")).toHaveLength(0);
    expect(invokeCmds("chapter_segmentation_start")).toHaveLength(0);

    resolveContext?.(null);
    await waitFor(() => expect(button).toBeEnabled());
  });

  it("passes authoritative library identity and uses the stable episode key", async () => {
    libraryContext = {
      mediaPath: "C:\\v\\a.mp4",
      group: {
        kind: "series",
        tmdbId: 123,
        title: "示例剧集",
      },
      item: {
        kind: "episode",
        tmdbId: 456,
        seriesTmdbId: 123,
        season: 2,
        episode: 7,
        title: "第七集",
      },
    };
    renderPanel();

    const button = await screen.findByRole("button", { name: "开始 AI 分段" });
    expect(button).toBeEnabled();
    fireEvent.click(button);
    await waitFor(() => {
      expect(invokeCmds("chapter_segmentation_start")).toHaveLength(1);
    });
    const [, args] = invokeCmds("chapter_segmentation_start")[0] as [
      string,
      { request: Record<string, unknown> },
    ];
    expect(args.request.episodeKey).toBe("s02e07");
    expect(args.request.episodeIdentity).toEqual({
      kind: "authoritative",
      seriesStableId: "tmdb:tv:123",
      episodeStableId: "s02e07",
      season: 2,
      episode: 7,
      seriesTitle: "示例剧集",
      title: "第七集",
    });
  });
});

const REMOTE_URL = "https://www.youtube.com/watch?v=remote1";

function renderRemote(props: ChaptersPanelProps = {}) {
  usePlayerStore.setState({
    currentFile: REMOTE_URL,
    sourceKind: "remote",
    status: "Paused",
    currentTimeMs: 500,
  });
  renderPanel(props);
}

function invokeCmds(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([name]) => name === cmd);
}

describe("ChaptersPanel online chapters", () => {
  it("lists cached resolve chapters without fresh resolve or probe", async () => {
    onlineResolve = {
      chapters: [
        { id: 1, startMs: 0, endMs: 60_000, title: " 开场 " },
        { id: 2, startMs: 60_000, endMs: null, title: "" },
      ],
    };
    renderRemote();
    await waitFor(() => {
      // Title is trimmed, empty title falls back to 章节 N.
      expect(screen.getByText("开场")).toBeInTheDocument();
    });
    expect(screen.getByText("章节 2")).toBeInTheDocument();
    expect(invokeCmds("ytdl_cached_resolve")).toHaveLength(1);
    expect(invokeCmds("ytdl_resolve")).toHaveLength(0);
    expect(invokeCmds("media_inspect")).toHaveLength(0);
    expect(invokeCmds("subtitle_load_choice")).toHaveLength(0);
  });

  it("shows the no-chapter reminder without generating fallback chapters", async () => {
    onlineResolve = { chapters: [] };
    renderRemote();
    await waitFor(() => {
      expect(screen.getByText("该在线视频暂无真实章节。")).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "开始 AI 分段" })).toBeInTheDocument();
  });

  it("seeks when an online chapter is clicked", async () => {
    onlineResolve = {
      chapters: [{ id: 7, startMs: 30_000, endMs: null, title: "中段" }],
    };
    renderRemote();
    await waitFor(() => {
      expect(screen.getByText("中段")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("中段"));
    await waitFor(() => {
      expect(invokeCmds("player_seek")).toHaveLength(1);
    });
  });

  it("shares one frozen query key shape across surfaces", () => {
    expect(ytdlResolveKey(REMOTE_URL)).toEqual(["ytdl-resolve", REMOTE_URL]);
  });
});

describe("ChaptersPanel boundaries", () => {
  it("does not query or start segmentation without a media file", async () => {
    usePlayerStore.setState({ currentFile: null, sourceKind: null });
    renderPanel();

    expect(screen.getByText("打开含章节元数据的视频后显示。")).toBeInTheDocument();
    await Promise.resolve();
    expect(invokeCmds("chapter_segmentation_status")).toHaveLength(0);
    expect(invokeCmds("chapter_segmentation_start")).toHaveLength(0);
  });

  it("does not show the AI segmentation entry when real chapters exist", async () => {
    chapters = [{ id: 1, startMs: 0, endMs: null, title: "正片" }];
    renderPanel();

    await waitFor(() => expect(screen.getByText("正片")).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "开始 AI 分段" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "已排队，等待执行…" })).not.toBeInTheDocument();
  });
});
