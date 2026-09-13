import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";

import { ChatMarkdown } from "./ChatMarkdown";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("ChatMarkdown", () => {
  it("renders headings and inline code", () => {
    render(<ChatMarkdown content={"## 标题\n\n这是 `code` 示例。"} />);
    expect(screen.getByRole("heading", { level: 2, name: "标题" })).toBeInTheDocument();
    expect(screen.getByText("code")).toBeInTheDocument();
  });

  it("renders block math with KaTeX", () => {
    const { container } = render(
      <ChatMarkdown content={"$$E = mc^2$$"} />,
    );
    expect(container.querySelector(".katex")).toBeTruthy();
  });
});

function renderMarkdown(content: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ChatMarkdown content={content} />
    </QueryClientProvider>,
  );
}

describe("ChatMarkdown citations", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({
          path: "C:\\v\\b.mp4",
          durationMs: 120_000,
          streams: [],
          chapters: [],
        });
      if (cmd === "library_resolve_episode_file")
        return Promise.resolve("C:\\v\\b.mp4");
      if (cmd === "player_open")
        return Promise.resolve({
          status: "Paused",
          currentFile: "C:\\v\\b.mp4",
        });
      if (cmd === "player_seek")
        return Promise.resolve({ status: "Paused", currentTimeMs: 192000 });
      return Promise.resolve(null);
    });
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 0,
      durationMs: 3_600_000,
    });
  });

  it("linkifies verified same-media citations; code stays plain", async () => {
    renderMarkdown(
      "关键在[03:12]这里。\n\n```\n[03:12] 不是引用\n```\n\n行内 `[04:00]` 也不链。",
    );
    const link = await screen.findByRole("button", { name: "[03:12]" });
    expect(link).toBeInTheDocument();
    // Bare prose time is not a citation.
    expect(screen.queryByText("3:12")).not.toBeInTheDocument();
  });

  it("clicking seeks without extra invokes", async () => {
    renderMarkdown("看[03:12]。");
    const link = await screen.findByRole("button", { name: "[03:12]" });
    fireEvent.click(link);
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_seek"),
      ).toHaveLength(1);
    });
  });

  it("out-of-range citations render unverified and never seek", async () => {
    renderMarkdown("超长的[99:59:59]伪造。");
    await waitFor(() => {
      expect(screen.getByText("[99:59:59]")).toBeInTheDocument();
    });
    expect(
      screen.queryByRole("button", { name: "[99:59:59]" }),
    ).not.toBeInTheDocument();
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_seek"),
    ).toHaveLength(0);
  });

  it("cross-episode citations confirm, save position and jump", async () => {
    renderMarkdown("见[第2集 · 01:00]。");
    const link = await screen.findByRole("button", {
      name: "[第2集 · 01:00]",
    });
    fireEvent.click(link);
    expect(
      await screen.findByText("切换媒体并保存当前位置？"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByText("切换"));
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_open"),
      ).toHaveLength(1);
    });
  });
});
