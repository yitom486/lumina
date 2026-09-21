import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import { AcpPanel } from "./AcpPanel";

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
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("AcpPanel", () => {
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

  it("clears any persisted resume hint on mount so a blank chat never silently resumes", async () => {
    useAcpSessionStore.getState().setSavedSession({
      sessionId: "stale-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();
    await waitFor(() => {
      expect(useAcpSessionStore.getState().savedSession).toBeNull();
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

  it("does not present the single chat dock as the planned two-tab experience", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });

    expect(screen.queryByRole("tab", { name: "AI 观剧流" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "自由聊天" })).not.toBeInTheDocument();
  });
});

it.todo("keeps AI watch-feed and free-chat drafts, activities, and errors isolated on tab switch");
