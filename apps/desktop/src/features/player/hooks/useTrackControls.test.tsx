import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player/store";
import { useTrackStore } from "@/features/player/trackStore";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: vi.fn().mockImplementation(() => ({ onmessage: null })),
}));

import { invoke } from "@tauri-apps/api/core";

import { useTrackControls } from "./useTrackControls";

function Probe() {
  const { subtitleChoiceId, subtitleVisible } = useTrackControls();
  return (
    <div data-testid="probe">{`${subtitleChoiceId}|${subtitleVisible}`}</div>
  );
}

function renderProbe() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <Probe />
    </QueryClientProvider>,
  );
}

function subtitleCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "player_set_subtitle")
    .map(([, args]) => args as Record<string, unknown>);
}

function lastSubtitleCall() {
  const calls = subtitleCalls();
  return calls[calls.length - 1];
}

beforeEach(() => {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "subtitle_list_choices")
      return Promise.resolve([
        {
          id: "embedded:0",
          source: "Embedded",
          label: "内嵌",
          supported: true,
          streamIndex: 0,
        },
      ]);
    if (cmd === "media_inspect")
      return Promise.resolve({
        path: "C:\\v\\a.mp4",
        streams: [],
        chapters: [],
      });
    // 上屏调用只断言参数，不污染 player store（残缺快照会冲掉 currentFile）。
    return Promise.resolve(null);
  });
  useTrackStore.setState({
    subtitleChoiceId: null,
    subtitleVisible: true,
    audioStreamIndex: null,
    directoryPrefs: {},
  });
  usePlayerStore.setState({
    currentFile: "C:\\v\\a.mp4",
    status: "Paused",
    currentTimeMs: 0,
  });
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("useTrackControls subtitle visibility", () => {
  it("hides the surface without clearing the selected track", async () => {
    renderProbe();
    // 初始化会选中默认轨并上屏。
    await waitFor(
      () => {
        expect(screen.getByTestId("probe")).toHaveTextContent(
          "embedded:0|true",
        );
      },
      { timeout: 8000 },
    );
    await waitFor(() => {
      expect(subtitleCalls().length).toBeGreaterThan(0);
    });

    // 关显示：画面清掉，但选中轨保留（文稿/AI 照常可用）。
    act(() => {
      useTrackStore.getState().setSubtitleVisible(false);
    });
    await waitFor(() => {
      expect(screen.getByTestId("probe")).toHaveTextContent(
        "embedded:0|false",
      );
    });
    expect(lastSubtitleCall()).toMatchObject({ source: "None" });
    expect(useTrackStore.getState().subtitleChoiceId).toBe("embedded:0");
  });

  it("re-applies the selected track when shown again", async () => {
    renderProbe();
    await waitFor(
      () => {
        expect(screen.getByTestId("probe")).toHaveTextContent(
          "embedded:0|true",
        );
      },
      { timeout: 8000 },
    );
    act(() => {
      useTrackStore.getState().setSubtitleVisible(false);
    });
    await waitFor(() => {
      expect(lastSubtitleCall()).toMatchObject({ source: "None" });
    });
    act(() => {
      useTrackStore.getState().setSubtitleVisible(true);
    });
    await waitFor(() => {
      expect(lastSubtitleCall()).toMatchObject({
        source: "Embedded",
        streamIndex: 0,
      });
    });
  });
});
