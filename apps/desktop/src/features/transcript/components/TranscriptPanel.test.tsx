import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { useAskAboutStore } from "@/features/acp/askAboutStore";
import { useChatUiStore } from "@/features/acp/chatUiStore";

import { TranscriptPanel } from "./TranscriptPanel";
import { useFollowStore } from "../followStore";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const CUES = [
  { index: 0, startMs: 0, endMs: 1000, text: "第一句" },
  { index: 1, startMs: 1000, endMs: 2000, text: "第二句" },
  { index: 2, startMs: 2000, endMs: 3000, text: "第三句" },
];

const scrollIntoView = vi.fn();

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <TranscriptPanel />
    </QueryClientProvider>,
  );
}

function seekCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "player_seek");
}

beforeEach(() => {
  Object.defineProperty(window.HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: scrollIntoView,
  });
  scrollIntoView.mockClear();
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "subtitle_list_choices")
      return Promise.resolve([
        {
          id: "s1",
          source: "Embedded",
          label: "中文",
          supported: true,
          streamIndex: 0,
        },
      ]);
    if (cmd === "subtitle_load_choice")
      return Promise.resolve({ choiceId: "s1", cues: CUES });
    if (cmd === "asr_status")
      return Promise.resolve({
        available: false,
        installSupported: false,
        models: [],
        catalog: [],
      });
    if (cmd === "media_inspect")
      return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
    if (cmd === "library_agent_models_discover")
      return Promise.resolve(null);
    return Promise.resolve(null);
  });
  useFollowStore.setState({ followEnabled: true, browsing: false });
  useTrackStore.setState({ subtitleChoiceId: "s1" });
  useAskAboutStore.setState({ request: null });
  useChatUiStore.getState().closeChat();
  usePlayerStore.setState({
    currentFile: "C:\\v\\a.mp4",
    status: "Paused",
    currentTimeMs: 1500,
  });
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("TranscriptPanel follow mode", () => {
  it("auto-scrolls while following", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第二句")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
  });

  it("wheel pauses follow; resume restores; scroll never seeks", async () => {
    const { container } = renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第二句")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
    const callsAfterMount = scrollIntoView.mock.calls.length;

    const list = container.querySelector("ul");
    expect(list).not.toBeNull();
    fireEvent.wheel(list as Element);
    expect(
      await screen.findByText("回到当前播放位置"),
    ).toBeInTheDocument();

    act(() => {
      usePlayerStore.setState({ currentTimeMs: 2500 });
    });
    await waitFor(() => {
      expect(screen.getByText("第三句")).toBeInTheDocument();
    });
    // Browsing: active cue changed, no yank, no seek.
    expect(scrollIntoView.mock.calls.length).toBe(callsAfterMount);
    expect(seekCalls()).toHaveLength(0);

    fireEvent.click(screen.getByText("回到当前播放位置"));
    await waitFor(() => {
      expect(scrollIntoView.mock.calls.length).toBeGreaterThan(
        callsAfterMount,
      );
    });
    expect(
      screen.queryByText("回到当前播放位置"),
    ).not.toBeInTheDocument();
    expect(seekCalls()).toHaveLength(0);
  });

  it("follow-off stops auto scroll until re-enabled", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第二句")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
    fireEvent.click(screen.getByText("跟随"));
    const callsAfterOff = scrollIntoView.mock.calls.length;
    act(() => {
      usePlayerStore.setState({ currentTimeMs: 500 });
    });
    await waitFor(() => {
      expect(screen.getByText("第一句")).toBeInTheDocument();
    });
    expect(scrollIntoView.mock.calls.length).toBe(callsAfterOff);
  });

  it("clicking a cue seeks and resumes follow", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "player_seek")
        return Promise.resolve({ status: "Paused", currentTimeMs: 0 });
      if (cmd === "subtitle_list_choices")
        return Promise.resolve([
          {
            id: "s1",
            source: "Embedded",
            label: "中文",
            supported: true,
            streamIndex: 0,
          },
        ]);
      if (cmd === "subtitle_load_choice")
        return Promise.resolve({ choiceId: "s1", cues: CUES });
      if (cmd === "asr_status")
        return Promise.resolve({
          available: false,
          installSupported: false,
          models: [],
          catalog: [],
        });
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "library_agent_models_discover")
        return Promise.resolve(null);
      return Promise.resolve(null);
    });
    const { container } = renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第一句")).toBeInTheDocument();
    });
    const list = container.querySelector("ul");
    fireEvent.wheel(list as Element);
    expect(
      await screen.findByText("回到当前播放位置"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByText("第三句"));
    await waitFor(() => {
      expect(seekCalls()).toHaveLength(1);
    });
    expect(
      screen.queryByText("回到当前播放位置"),
    ).not.toBeInTheDocument();
  });

  it("asks about a cue with its own anchor, not the live position", async () => {
    usePlayerStore.setState({ currentTimeMs: 2500 });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第三句")).toBeInTheDocument();
    });
    const askButtons = screen.getAllByText("问");
    expect(askButtons.length).toBeGreaterThan(0);
    fireEvent.click(askButtons[0] as Element);
    const request = useAskAboutStore.getState().request;
    // First cue starts at 0 even though playback is at 2500.
    expect(request).toMatchObject({ anchorMs: 0 });
    expect(request?.text).toContain("解释这一段");
    expect(useChatUiStore.getState().chatOpen).toBe(true);
    useAskAboutStore.getState().consume();
  });
});
