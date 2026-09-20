import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import type { ChapterSegmentationSnapshot } from "../api";
import { usePlayerStore } from "@/features/player";

import { ChaptersPanel } from "./ChaptersPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const MEDIA_PATH = "C:\\fixtures\\episode-01.mp4";

type StatusError = { code: string; message: string };
type StatusResult = ChapterSegmentationSnapshot | StatusError;

function snapshot(
  status: string,
  overrides: Partial<ChapterSegmentationSnapshot> = {},
): ChapterSegmentationSnapshot {
  return {
    id: 41,
    taskKey: `chapter-segmentation:${MEDIA_PATH}:${MEDIA_PATH}`,
    taskType: "chapter_segmentation",
    episodeId: null,
    chapterId: null,
    episodeIdentity: { kind: "legacy", reason: "metadata_unavailable" },
    status,
    sessionId: null,
    promptVersion: "chapter-agent.v1",
    outputContractVersion: "chapter_tool_workflow.v1",
    attemptCount: status === "pending" ? 1 : 0,
    retryCount: 0,
    maxAttempts: 3,
    failureCode: null,
    failureMessage: null,
    validationSummary: null,
    canRetry: false,
    retryAction: null,
    agentConfigured: true,
    outputJson: null,
    draftChapters: [],
    createdAtMs: 1,
    updatedAtMs: 1,
    ...overrides,
  };
}

function draftChapters(): ChapterSegmentationSnapshot["draftChapters"] {
  return [
    {
      id: 101,
      stableId: "opening",
      startMs: 0,
      endMs: 30_000,
      title: "雨夜开门",
      mainline: null,
      status: "waiting_evidence",
      updatedAtMs: 2,
    },
    {
      id: 102,
      stableId: "decision",
      startMs: 30_000,
      endMs: 60_000,
      title: "决定调查",
      mainline: null,
      status: "analyzing",
      updatedAtMs: 2,
    },
  ];
}

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ChaptersPanel />
    </QueryClientProvider>,
  );
}

async function advanceTimers(milliseconds = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(milliseconds);
  });
}

async function settleUntil(predicate: () => boolean) {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    await advanceTimers(100);
    if (predicate()) return;
  }
  throw new Error(
    `controlled UI state did not settle: ${document.body.textContent ?? ""}`,
  );
}

function commandCalls(command: string) {
  return vi.mocked(invoke).mock.calls.filter(([name]) => name === command);
}

beforeEach(() => {
  vi.useFakeTimers();
  usePlayerStore.setState({
    currentFile: MEDIA_PATH,
    sourceKind: null,
    status: "Paused",
    currentTimeMs: 500,
  });
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("ChaptersPanel command-boundary integration", () => {
  it("projects no chapters through pending/running outline, draft skeleton, and success", async () => {
    const pending = snapshot("pending", { attemptCount: 1 });
    const running = snapshot("running");
    const outlined = snapshot("running", {
      draftChapters: draftChapters(),
    });
    const succeeded = snapshot("succeeded", {
      draftChapters: draftChapters().map((chapter) => ({
        ...chapter,
        status: "generated",
        updatedAtMs: 3,
      })),
    });
    const statusResults: StatusResult[] = [
      { code: "NotFound", message: "尚未开始该媒体的 AI 分段" },
      running,
      outlined,
      succeeded,
    ];
    let statusIndex = 0;
    let resolveStart: ((value: ChapterSegmentationSnapshot) => void) | undefined;
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "media_inspect") {
        return Promise.resolve({ path: MEDIA_PATH, streams: [], chapters: [] });
      }
      if (command === "library_context_for_media") return Promise.resolve(null);
      if (command === "chapter_segmentation_status") {
        const result = statusResults[Math.min(statusIndex++, statusResults.length - 1)];
        return "code" in result ? Promise.reject(result) : Promise.resolve(result);
      }
      if (command === "chapter_segmentation_start") {
        return new Promise<ChapterSegmentationSnapshot>((resolve) => {
          resolveStart = resolve;
        });
      }
      return Promise.resolve(null);
    });

    renderPanel();
    await settleUntil(() => Boolean(screen.queryByText("该文件暂无容器章节。")));
    const startButton = screen.getByRole("button", { name: "开始 AI 分段" });
    expect(startButton).toBeEnabled();

    fireEvent.click(startButton);
    await settleUntil(() => startButton.hasAttribute("disabled"));
    expect(screen.getByRole("button", { name: "已排队，等待执行…" })).toBeDisabled();
    expect(
      screen.getByText(
        "任务已保存，章节 Agent 正在独立执行；不会写入自由聊天，也不会自动重复启动。",
      ),
    ).toBeInTheDocument();

    fireEvent.click(startButton);
    expect(commandCalls("chapter_segmentation_start")).toHaveLength(1);

    await act(async () => {
      resolveStart?.(pending);
    });
    await settleUntil(() => Boolean(screen.queryByText("正在分析字幕与画面…")));
    expect(commandCalls("chapter_segmentation_status").length).toBeGreaterThanOrEqual(2);

    await advanceTimers(1_000);
    await settleUntil(() => Boolean(screen.queryByText("雨夜开门")));
    expect(screen.getByText("等待取证")).toBeInTheDocument();
    expect(screen.getByText("决定调查")).toBeInTheDocument();
    expect(screen.getByText("分析中")).toBeInTheDocument();
    expect(screen.queryByText(/stderr|JSON-RPC|tool payload|\{"status"/i)).not.toBeInTheDocument();

    await advanceTimers(1_000);
    await settleUntil(() => screen.queryAllByText("已生成").length === 2);
    expect(screen.getByText("AI 章节草稿 · 已从任务大纲持久化")).toBeInTheDocument();
    expect(commandCalls("chapter_segmentation_start")).toHaveLength(1);
  });

  it("shows validation failure retry without publishing or exposing raw command data", async () => {
    const running = snapshot("running");
    const validationFailure = snapshot("validation_failure", {
      attemptCount: 1,
      retryCount: 1,
      failureCode: "ValidationFailed",
      failureMessage: "章节分段结果未通过校验。",
      validationSummary: "章节结果缺少必要内容。",
      canRetry: true,
      retryAction: "retry",
    });
    const statusResults: StatusResult[] = [
      { code: "NotFound", message: "尚未开始该媒体的 AI 分段" },
      running,
      validationFailure,
    ];
    let statusIndex = 0;
    let startCount = 0;
    let resolveRetry: ((value: ChapterSegmentationSnapshot) => void) | undefined;
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "media_inspect") {
        return Promise.resolve({ path: MEDIA_PATH, streams: [], chapters: [] });
      }
      if (command === "library_context_for_media") return Promise.resolve(null);
      if (command === "chapter_segmentation_status") {
        const result = statusResults[Math.min(statusIndex++, statusResults.length - 1)];
        return "code" in result ? Promise.reject(result) : Promise.resolve(result);
      }
      if (command === "chapter_segmentation_start") {
        startCount += 1;
        if (startCount === 2) {
          return new Promise<ChapterSegmentationSnapshot>((resolve) => {
            resolveRetry = resolve;
          });
        }
        return Promise.resolve(snapshot("pending", { attemptCount: 1 }));
      }
      return Promise.resolve(null);
    });

    renderPanel();
    await settleUntil(() => Boolean(screen.queryByRole("button", { name: "开始 AI 分段" })));
    fireEvent.click(screen.getByRole("button", { name: "开始 AI 分段" }));
    await settleUntil(() => Boolean(screen.queryByText("正在分析字幕与画面…")));

    await advanceTimers(1_000);
    await settleUntil(() => Boolean(screen.queryByRole("button", { name: "再次尝试" })));
    expect(screen.getByText("章节分段结果未通过校验。")).toBeInTheDocument();
    expect(screen.getByText("章节结果缺少必要内容。")).toBeInTheDocument();
    expect(screen.getByText("已尝试 1 / 3 次。")).toBeInTheDocument();
    expect(screen.getByText(/不会写入自由聊天/)).toBeInTheDocument();
    expect(screen.queryByText(/stderr|JSON-RPC|tool payload|\{"status"|C:\\fixtures/i)).not.toBeInTheDocument();

    const retryButton = screen.getByRole("button", { name: "再次尝试" });
    fireEvent.click(retryButton);
    await settleUntil(() => retryButton.hasAttribute("disabled"));
    fireEvent.click(retryButton);
    expect(commandCalls("chapter_segmentation_start")).toHaveLength(2);

    await act(async () => {
      resolveRetry?.(snapshot("pending", { attemptCount: 2 }));
    });
  });
});
