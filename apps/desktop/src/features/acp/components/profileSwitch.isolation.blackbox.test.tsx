import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";
import { useAcpSettingsStore } from "@lumina/chat-ui/acpSettingsStore";

import { AcpPanel } from "./AcpPanel";
import { chatRestoreKeyFor } from "../chatRestore";

type ChannelHandler = { onmessage: ((event: unknown) => void) | null };

const { channels } = vi.hoisted(() => ({
  channels: [] as ChannelHandler[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null;
    constructor() {
      channels.push(this);
    }
  },
}));

import { invoke } from "@tauri-apps/api/core";

const { promptResolvers } = vi.hoisted(() => ({
  promptResolvers: [] as ((value: unknown) => void)[],
}));

const mockStatus = {
  available: true,
  adapterFound: true,
  codexFound: true,
  activeProfileId: "codex",
  profiles: [
    {
      id: "codex",
      name: "Codex（默认）",
      kind: "Codex" as const,
      command: "bunx.exe",
      args: ["@agentclientprotocol/codex-acp"],
      env: {},
      available: true,
      resolvedCommand: "bunx.exe",
    },
    {
      id: "cursor",
      name: "Cursor CLI",
      kind: "Cursor" as const,
      command: "agent.cmd",
      args: ["acp"],
      env: {},
      available: true,
      resolvedCommand: "agent.cmd",
    },
  ],
  message: "ok",
  sessionActive: false,
  busy: false,
};

function seedSnapshot(profileId: string, userText: string, answer: string) {
  localStorage.setItem(
    chatRestoreKeyFor(profileId),
    JSON.stringify({
      version: 1,
      profileId,
      cwd: null,
      draft: "",
      turns: [
        {
          id: `cached-${profileId}`,
          userText,
          answer,
          status: "done",
          activities: [],
          showActivities: false,
        },
      ],
      updatedAtMs: 1,
    }),
  );
}

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
  channels.length = 0;
  promptResolvers.length = 0;
  localStorage.clear();
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  useAcpSessionStore.setState({ savedSessions: {} });
  useAcpSettingsStore.setState({
    permissionMode: "auto",
    thinkingLevel: "minimal",
    modelId: "",
    reasoningEffort: "",
  });
  usePlayerStore.setState({ currentFile: null, status: "Idle" });
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    if (cmd === "acp_connect") return Promise.resolve(null);
    if (cmd === "acp_close") return Promise.resolve(null);
    if (cmd === "acp_prompt") {
      return new Promise((resolve) => {
        promptResolvers.push(resolve);
      });
    }
    return Promise.resolve(null);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("profile switch isolation blackbox (switch world entry)", () => {
  it("swaps worlds via the toolbar dropdown: turns swap, model clears, hints and prefs stay", async () => {
    seedSnapshot("codex", "codex 的旧问题", "codex 的旧回答");
    seedSnapshot("cursor", "cursor 的问题", "cursor 的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    useAcpSessionStore.getState().setSavedSessionFor("cursor", {
      sessionId: "cursor-thread",
      profileId: "cursor",
      cwd: "D:\\movie",
    });
    useAcpSettingsStore
      .getState()
      .patchSettings({ modelId: "gpt-5.6-luna", permissionMode: "ask", thinkingLevel: "verbose" });
    renderPanel();
    expect((await screen.findAllByText("codex 的旧问题")).length).toBeGreaterThanOrEqual(1);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/输入问题/)).toBeInTheDocument();
    });

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("切换 Agent"));
    await user.click(await screen.findByRole("menuitem", { name: "Cursor CLI" }));

    // 新世界摆新快照，旧世界内容不泄漏。
    expect((await screen.findAllByText("cursor 的问题")).length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByText("codex 的旧问题")).not.toBeInTheDocument();
    // codex 的模型绝不漏进 cursor；全局偏好保持现状（待产品确认，不擅自改）。
    expect(useAcpSettingsStore.getState().modelId).toBe("");
    expect(useAcpSettingsStore.getState().permissionMode).toBe("ask");
    expect(useAcpSettingsStore.getState().thinkingLevel).toBe("verbose");
    // 两家 hint 都在：切回即 resume，谁也不删谁的。
    expect(useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId).toBe(
      "codex-thread",
    );
    expect(useAcpSessionStore.getState().savedSessionFor("cursor")?.sessionId).toBe(
      "cursor-thread",
    );
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).not.toBeNull();
    expect(localStorage.getItem(chatRestoreKeyFor("cursor"))).not.toBeNull();
  });

  it("does not leak the old world's pending permission and progress into the new world", async () => {
    seedSnapshot("codex", "codex 的旧问题", "codex 的旧回答");
    seedSnapshot("cursor", "cursor 的问题", "cursor 的回答");
    renderPanel();
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/输入问题/)).toBeInTheDocument();
    });

    // 真实开一轮：旧世界转出 permission 弹窗 + 进度行。
    fireEvent.change(screen.getByPlaceholderText(/输入问题/), {
      target: { value: "旧世界的问题" },
    });
    fireEvent.keyDown(screen.getByPlaceholderText(/输入问题/), {
      key: "Enter",
      shiftKey: false,
    });
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_prompt"),
      ).toHaveLength(1);
    });
    const promptChannel = channels[channels.length - 1];
    expect(promptChannel?.onmessage).toBeTypeOf("function");
    await act(async () => {
      promptChannel?.onmessage?.({ type: "progress", message: "旧世界发送中…" });
    });
    expect(await screen.findByText("旧世界发送中…")).toBeInTheDocument();
    await act(async () => {
      promptChannel?.onmessage?.({
        type: "permissionRequest",
        requestId: "r1",
        toolCallId: "t1",
        title: "旧世界审批",
        options: [
          { optionId: "allow", name: "允许", kind: "allow" },
          { optionId: "deny", name: "拒绝", kind: "reject" },
        ],
      });
    });
    expect(await screen.findByText("需要你的审批：旧世界审批")).toBeInTheDocument();

    // 回合落定但审批未决：busy 已回落（切换放行），弹窗仍挂着——切 profile 必清。
    await act(async () => {
      promptChannel?.onmessage?.({ type: "finished", text: "旧世界答完了" });
    });
    promptResolvers.splice(0).forEach((resolve) => resolve("ok"));
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/输入问题/)).toBeInTheDocument();
    });
    expect(screen.getByText("需要你的审批：旧世界审批")).toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("切换 Agent"));
    await user.click(await screen.findByRole("menuitem", { name: "Cursor CLI" }));

    expect((await screen.findAllByText("cursor 的问题")).length).toBeGreaterThanOrEqual(1);
    // 旧世界的弹窗、进度、排队指示一个都不许进新世界。
    expect(screen.queryByText("需要你的审批：旧世界审批")).not.toBeInTheDocument();
    expect(screen.queryByText("旧世界发送中…")).not.toBeInTheDocument();
    expect(screen.queryByText(/排队/)).not.toBeInTheDocument();
    expect(screen.queryByText("旧世界的问题")).not.toBeInTheDocument();
  });
});
