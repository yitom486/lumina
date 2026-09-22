import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import { AcpPanel, handleAssistantAction } from "./AcpPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null;
  },
}));

import { invoke } from "@tauri-apps/api/core";

const mockStatus = {
  available: false,
  adapterFound: false,
  codexFound: false,
  activeProfileId: "codex",
  profiles: [
    {
      id: "codex",
      name: "Codex（默认）",
      kind: "Codex" as const,
      command: "bunx.exe",
      args: ["@agentclientprotocol/codex-acp"],
      env: {},
      available: false,
      resolvedCommand: null,
    },
  ],
  message: "未找到可用 ACP Agent",
  hint: "请安装 Bun 或配置 Agent",
  responsesOnlyNote: "Responses API",
  sessionActive: false,
  busy: false,
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AcpPanel />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    return Promise.resolve(null);
  });
  usePlayerStore.setState({
    currentFile: null,
    status: "Idle",
  });
  useAcpSessionStore.setState({ savedSessions: {} });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("AcpPanel", () => {
  it("handles rich actions through the existing player, ask, and note capabilities", async () => {
    const seek = vi.fn(async () => undefined);
    const askAbout = vi.fn();
    const saveNote = vi.fn(async () => undefined);
    const notify = vi.fn();
    const deps = {
      mediaPath: "D:\\show.mp4",
      currentTimeMs: 12_000,
      busy: false,
      seek,
      askAbout,
      saveNote,
      notify,
    };

    await handleAssistantAction(
      { type: "seek", anchor: { startMs: 42_000 } },
      deps,
    );
    await handleAssistantAction(
      { type: "ask", anchor: { startMs: 50_000 }, prompt: "解释这段" },
      deps,
    );
    await handleAssistantAction(
      { type: "save-note", anchor: { chapterId: "ch-1" }, content: "重要线索" },
      deps,
    );

    expect(seek).toHaveBeenCalledWith(42_000);
    expect(askAbout).toHaveBeenCalledWith(50_000, "解释这段");
    expect(saveNote).toHaveBeenCalledWith({
      mediaPath: "D:\\show.mp4",
      positionMs: 12_000,
      body: "重要线索",
    });
    expect(notify).toHaveBeenCalledWith("已保存为笔记");
  });

  it("safely rejects rich actions without media", async () => {
    const notify = vi.fn();
    await handleAssistantAction(
      { type: "seek", anchor: { startMs: 42_000 } },
      {
        mediaPath: null,
        currentTimeMs: 0,
        busy: false,
        seek: vi.fn(async () => undefined),
        askAbout: vi.fn(),
        saveNote: vi.fn(async () => undefined),
        notify,
      },
    );
    expect(notify).toHaveBeenCalledWith("请先打开视频");
  });

  it("falls back to the live position for anchorless ask actions", async () => {
    const askAbout = vi.fn();
    const deps = {
      mediaPath: "D:\\show.mp4",
      currentTimeMs: 12_000,
      busy: false,
      seek: vi.fn(async () => undefined),
      askAbout,
      saveNote: vi.fn(async () => undefined),
      notify: vi.fn(),
    };

    // 章节锚点没有毫秒：取当前播放位置，和存批注同策略。
    await handleAssistantAction(
      { type: "ask", anchor: { chapterId: "ch-1" }, prompt: "解释" },
      deps,
    );
    expect(askAbout).toHaveBeenCalledWith(12_000, "解释");

    // 完全无锚点（如观众问题的一点即问）：同样取当前播放位置直接发送。
    await handleAssistantAction({ type: "ask", prompt: "直接问" }, deps);
    expect(askAbout).toHaveBeenCalledWith(12_000, "直接问");
  });

  it("mounts without throwing while status is loading", async () => {
    expect(() => renderPanel()).not.toThrow();
    await waitFor(() => {
      expect(screen.getByText("未找到可用 ACP Agent")).toBeInTheDocument();
      expect(screen.getByText("未配置")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });
  });

  it("does not auto-connect when agent is unavailable", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("未配置")).toBeInTheDocument();
    });
    const connectCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => cmd === "acp_connect");
    expect(connectCalls).toHaveLength(0);
  });

  it("keeps the persisted resume hint on mount so restart can auto-resume", async () => {
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "last-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex"),
    ).toMatchObject({
      sessionId: "last-thread",
    });
  });

  it("does not trigger infinite re-renders with persisted profiles", async () => {
    localStorage.setItem(
      "lumina-acp-profiles",
      JSON.stringify({
        state: {
          activeProfileId: "codex",
          profiles: [
            {
              id: "codex",
              name: "Codex（默认）",
              kind: "Codex",
              command: "bunx.exe",
              args: ["@agentclientprotocol/codex-acp"],
              env: {},
            },
          ],
        },
        version: 0,
      }),
    );

    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });

    const depthErrors = errorSpy.mock.calls.filter((call) =>
      String(call[0]).includes("Maximum update depth"),
    );
    expect(depthErrors).toHaveLength(0);
  });

  it("renders a single merged companion surface without mode tabs", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });

    // 模式切换已合并：没有 tab，只有手风琴里的观剧流（默认展开）。
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "AI 观剧流" }),
    ).toBeInTheDocument();
  });
});
