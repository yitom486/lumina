import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";

import { ChaptersPanel } from "./ChaptersPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const CUES = [
  { index: 0, startMs: 0, endMs: 900, text: "a" },
  { index: 1, startMs: 1000, endMs: 1900, text: "b" },
  { index: 2, startMs: 30_000, endMs: 30_900, text: "c" },
];

let chapters: unknown[] = [];

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ChaptersPanel />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  chapters = [];
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "media_inspect")
      return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters });
    if (cmd === "subtitle_load_choice")
      return Promise.resolve({ choiceId: "s1", cues: CUES });
    if (cmd === "player_seek")
      return Promise.resolve({ status: "Paused", currentTimeMs: 0 });
    return Promise.resolve(null);
  });
  useTrackStore.setState({ subtitleChoiceId: "s1" });
  usePlayerStore.setState({
    currentFile: "C:\\v\\a.mp4",
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

describe("ChaptersPanel soft segments", () => {
  it("lists mechanical segments and seeks on click", async () => {
    renderPanel();
    await waitFor(() => {
      expect(
        screen.getByText("无容器章节，按字幕停顿机械分段（非语义章节）。"),
      ).toBeInTheDocument();
    });
    expect(screen.getByText("分段 1 · 0:00–0:01")).toBeInTheDocument();
    expect(screen.getByText("分段 2 · 0:30–0:30")).toBeInTheDocument();
    fireEvent.click(screen.getByText("分段 2 · 0:30–0:30"));
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_seek"),
      ).toHaveLength(1);
    });
  });

  it("prefers container chapters over segments", async () => {
    chapters = [{ id: 1, startMs: 0, endMs: null, title: "正片" }];
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("正片")).toBeInTheDocument();
    });
    expect(screen.queryByText(/按字幕停顿机械分段/)).not.toBeInTheDocument();
    expect(screen.queryByText(/分段 1/)).not.toBeInTheDocument();
  });

  it("stays empty without subtitles", async () => {
    useTrackStore.setState({ subtitleChoiceId: null });
    renderPanel();
    await waitFor(() => {
      expect(
        screen.getByText("该文件没有容器章节元数据，也没有可用字幕可供分段。"),
      ).toBeInTheDocument();
    });
  });
});
