import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { ytdlResolveKey } from "@lumina/query-keys";

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
let onlineResolve: unknown = null;

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
  onlineResolve = null;
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "media_inspect")
      return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters });
    if (cmd === "subtitle_load_choice")
      return Promise.resolve({ choiceId: "s1", cues: CUES });
    if (cmd === "ytdl_cached_resolve") return Promise.resolve(onlineResolve);
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
  it("shows a pending probe state while media metadata is loading", () => {
    let resolveProbe: ((value: unknown) => void) | undefined;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect") {
        return new Promise((resolve) => {
          resolveProbe = resolve;
        });
      }
      if (cmd === "subtitle_load_choice")
        return Promise.resolve({ choiceId: "s1", cues: CUES });
      return Promise.resolve(null);
    });

    renderPanel();

    expect(screen.getByText("正在探测章节…")).toBeInTheDocument();
    resolveProbe?.({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
  });

  it("shows a business-safe failed state without exposing probe details", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect") {
        return Promise.reject(new Error("ffprobe stderr: private diagnostic"));
      }
      return Promise.resolve(null);
    });

    renderPanel();

    await waitFor(() => {
      expect(screen.getByText("无法读取媒体信息（章节依赖探测）。")).toBeInTheDocument();
    });
    expect(screen.queryByText(/ffprobe|private diagnostic/)).not.toBeInTheDocument();
  });

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

it.todo("projects a user-visible pending AI chapter task without starting a real Agent");
it.todo("projects a user-visible running AI chapter task in the AI watch feed");
it.todo("projects a failed AI chapter task with an incremental retry entry point");
it.todo("projects a succeeded AI chapter task without adding a turn to free chat");

const REMOTE_URL = "https://www.youtube.com/watch?v=remote1";

function renderRemote() {
  usePlayerStore.setState({
    currentFile: REMOTE_URL,
    sourceKind: "remote",
    status: "Paused",
    currentTimeMs: 500,
  });
  renderPanel();
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

  it("shows empty state without chapters and never falls back to segments", async () => {
    onlineResolve = { chapters: [] };
    renderRemote();
    await waitFor(() => {
      expect(screen.getByText("该在线视频暂无章节信息。")).toBeInTheDocument();
    });
    expect(screen.queryByText(/按字幕停顿机械分段/)).not.toBeInTheDocument();
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
