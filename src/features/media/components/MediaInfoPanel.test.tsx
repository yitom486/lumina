import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";

import { MediaInfoPanel } from "./MediaInfoPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const probeMissing = {
  code: "ProbeNotFound",
  message: "媒体分析组件未就绪",
  details: "place ffprobe.exe under native/ffmpeg/",
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MediaInfoPanel />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "media_inspect") return Promise.reject(probeMissing);
    if (cmd === "media_tool_status")
      return Promise.resolve({
        available: false,
        message: "媒体分析组件未就绪",
        hint: "安装包通常自带该组件；仍缺失时请重装应用。",
      });
    return Promise.resolve(null);
  });
  usePlayerStore.setState({
    currentFile: "C:\\videos\\a.mp4",
    status: "Paused",
    sourceKind: null,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("MediaInfoPanel", () => {
  it("shows business message plus install hint when ffprobe is missing", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("媒体分析组件未就绪")).toBeInTheDocument();
    });
    expect(
      await screen.findByText("安装包通常自带该组件；仍缺失时请重装应用。"),
    ).toBeInTheDocument();
  });

  it("never surfaces code enum or tool details", async () => {
    const { container } = renderPanel();
    await waitFor(() => {
      expect(screen.getByText("媒体分析组件未就绪")).toBeInTheDocument();
    });
    expect(container.textContent).not.toContain("ProbeNotFound");
    expect(container.textContent).not.toContain("ffprobe");
  });
});
