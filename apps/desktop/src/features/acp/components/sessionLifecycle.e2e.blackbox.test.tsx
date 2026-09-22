import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
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
import { resetChatStoreEphemeralState } from "../chatRestore";
import { acpQueryKeys } from "../queries";
import type { LoadedTranscriptEvent } from "../api";
import type { AgentSessionListResult } from "../types";

type ChannelHandler = { onmessage: ((event: unknown) => void) | null };

const { channels } = vi.hoisted(() => ({
  channels: [] as ChannelHandler[],
}));

// 与 profileSwitch 文件同形的 harness：SQLite 主层两表（快照按 profile+session，hint 按 profile）。
type MockSnapshotRow = {
  profileId: string;
  sessionId: string;
  cwd: string | null;
  draft: string;
  turnsJson: string;
};

type MockHintRow = { sessionId: string; cwd: string };

const mockSnapshotTable = new Map<string, MockSnapshotRow>();
const mockHintTable = new Map<string, MockHintRow>();
const mockTranscriptTable = new Map<string, LoadedTranscriptEvent[]>();
let mockAgentSessionList: AgentSessionListResult = {
  verified: false,
  sessions: [],
  truncated: false,
};

const CWD = "D:\\movie";
const MEDIA = "D:\\movie\\ep01.mp4";

function dbSnapshotKey(profileId: string, sessionId: string): string {
  return `${profileId}\0${sessionId}`;
}

function dbTurn(profileId: string, userText: string, answer: string) {
  return {
    id: `db-${profileId}-${userText}`,
    userText,
    answer,
    status: "done",
    activities: [],
    showActivities: false,
  };
}

function seedDbSnapshot(
  profileId: string,
  sessionId: string,
  userText: string,
  answer: string,
) {
  mockSnapshotTable.set(dbSnapshotKey(profileId, sessionId), {
    profileId,
    sessionId,
    cwd: CWD,
    draft: "",
    turnsJson: JSON.stringify([dbTurn(profileId, userText, answer)]),
  });
}

function chatInputOf(args: unknown): Record<string, unknown> {
  if (typeof args !== "object" || args === null) return {};
  const input = (args as { input?: unknown }).input;
  if (typeof input !== "object" || input === null) return {};
  return input as Record<string, unknown>;
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

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

const mockStatus = {
  available: true,
  adapterFound: true,
  codexFound: true,
  activeProfileId: "codex",
  profiles: [
    {
      id: "codex",
      name: "ChatGPT",
      kind: "Codex" as const,
      command: "bunx.exe",
      args: ["@agentclientprotocol/codex-acp"],
      env: {},
      available: true,
      resolvedCommand: "bunx.exe",
    },
    {
      id: "cursor",
      name: "Cursor",
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

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={client}>
      <AcpPanel />
    </QueryClientProvider>,
  );
  return { client, ...view };
}

function connectCalls() {
  return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_connect");
}

function switchCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "acp_switch_session");
}

function loadCallsFor(sessionId: string) {
  return vi
    .mocked(invoke)
    .mock.calls.filter(
      ([cmd, args]) =>
        cmd === "acp_load_session" &&
        typeof args === "object" &&
        args !== null &&
        (args as { sessionId?: unknown }).sessionId === sessionId,
    );
}

async function waitForConnectCount(count: number) {
  await waitFor(() => {
    expect(connectCalls()).toHaveLength(count);
  });
  expect(channels[channels.length - 1]?.onmessage).toBeTypeOf("function");
}

async function waitForSwitchCount(count: number) {
  await waitFor(() => {
    expect(switchCalls()).toHaveLength(count);
  });
  expect(channels[channels.length - 1]?.onmessage).toBeTypeOf("function");
}

beforeEach(() => {
  channels.length = 0;
  localStorage.clear();
  resetChatStoreEphemeralState();
  mockSnapshotTable.clear();
  mockHintTable.clear();
  mockTranscriptTable.clear();
  mockAgentSessionList = { verified: false, sessions: [], truncated: false };
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  useAcpSessionStore.setState({ savedSessions: {} });
  useAcpSettingsStore.setState({
    permissionMode: "auto",
    thinkingLevel: "minimal",
    modelId: "",
    reasoningEffort: "",
  });
  usePlayerStore.setState({ currentFile: null, status: "Idle" });
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (typeof cmd === "string" && cmd.startsWith("chat_")) {
      const input = chatInputOf(args);
      const profileId = asString(input.profileId);
      const sessionId = asString(input.sessionId);
      if (cmd === "chat_snapshot_get") {
        return Promise.resolve(
          mockSnapshotTable.get(dbSnapshotKey(profileId, sessionId)) ?? null,
        );
      }
      if (cmd === "chat_snapshot_upsert") {
        const row: MockSnapshotRow = {
          profileId,
          sessionId,
          cwd:
            input.cwd === null || typeof input.cwd === "string"
              ? (input.cwd as string | null)
              : null,
          draft: asString(input.draft),
          turnsJson: asString(input.turnsJson, "[]"),
        };
        mockSnapshotTable.set(dbSnapshotKey(profileId, sessionId), row);
        return Promise.resolve(row);
      }
      if (cmd === "chat_snapshot_delete") {
        return Promise.resolve(
          mockSnapshotTable.delete(dbSnapshotKey(profileId, sessionId)),
        );
      }
      if (cmd === "chat_hint_get") {
        const hint = mockHintTable.get(profileId) ?? null;
        return Promise.resolve(
          hint
            ? { profileId, sessionId: hint.sessionId, cwd: hint.cwd }
            : null,
        );
      }
      if (cmd === "chat_hint_upsert") {
        mockHintTable.set(profileId, {
          sessionId: asString(input.sessionId),
          cwd: asString(input.cwd),
        });
        return Promise.resolve(null);
      }
      if (cmd === "chat_hint_delete") {
        return Promise.resolve(mockHintTable.delete(profileId));
      }
      return Promise.resolve(null);
    }
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    if (cmd === "acp_connect") return Promise.resolve(null);
    if (cmd === "acp_switch_session") return Promise.resolve(null);
    if (cmd === "acp_new_chat") return Promise.resolve(null);
    if (cmd === "acp_close") return Promise.resolve(null);
    if (cmd === "acp_list_agent_sessions")
      return Promise.resolve(mockAgentSessionList);
    if (cmd === "acp_load_session") {
      const record =
        typeof args === "object" && args !== null
          ? (args as { sessionId?: unknown })
          : {};
      const sessionId =
        typeof record.sessionId === "string" ? record.sessionId : "";
      return Promise.resolve(mockTranscriptTable.get(sessionId) ?? []);
    }
    if (cmd === "acp_task_contracts") return Promise.resolve([]);
    if (cmd === "acp_prompt") return new Promise(() => {});
    return Promise.resolve(null);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("session lifecycle e2e blackbox (connect resume + history reload)", () => {
  it("switch resume success: empty panel reloads remote text, hint unchanged, banner reports recovered", async () => {
    usePlayerStore.setState({ currentFile: MEDIA });
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-hint-1",
      profileId: "codex",
      cwd: CWD,
    });
    useAcpSessionStore.getState().setSavedSessionFor("cursor", {
      sessionId: "cursor-hint-1",
      profileId: "cursor",
      cwd: CWD,
    });
    mockTranscriptTable.set("cursor-hint-1", [
      { role: "user", text: "cursor 远端老问题" },
      { role: "agent", text: "cursor 远端老回答" },
    ]);
    const { client } = renderPanel();
    await waitForConnectCount(1);

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("切换 Agent"));
    await user.click(await screen.findByRole("menuitem", { name: "Cursor" }));
    await waitForConnectCount(2);
    const connectChannel = channels[channels.length - 1];
    await act(async () => {
      connectChannel?.onmessage?.({
        type: "sessionSaved",
        sessionId: "cursor-hint-1",
        profileId: "cursor",
        cwd: CWD,
        resume: "resumed",
      });
    });

    await waitFor(() => {
      expect(loadCallsFor("cursor-hint-1").length).toBeGreaterThanOrEqual(1);
    });
    expect(
      (await screen.findAllByText("cursor 远端老问题")).length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("cursor 远端老回答")).toBeInTheDocument();
    expect(
      (await screen.findAllByText(/已恢复/)).length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      useAcpSessionStore.getState().savedSessionFor("cursor")?.sessionId,
    ).toBe("cursor-hint-1");
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-hint-1");
    const cached = client.getQueryData<LoadedTranscriptEvent[]>(
      acpQueryKeys.transcript("cursor", CWD, "cursor-hint-1"),
    );
    expect(cached?.length).toBeGreaterThanOrEqual(2);
  });

  it("switch resume failure creates honest new chat: old turns cleared, banner reports new, hint moves", async () => {
    usePlayerStore.setState({ currentFile: MEDIA });
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-hint-1",
      profileId: "codex",
      cwd: CWD,
    });
    useAcpSessionStore.getState().setSavedSessionFor("cursor", {
      sessionId: "cursor-old-1",
      profileId: "cursor",
      cwd: CWD,
    });
    seedDbSnapshot("codex", "codex-hint-1", "codex 旧问题", "codex 旧回答");
    seedDbSnapshot(
      "cursor",
      "cursor-old-1",
      "cursor 旧快照问题",
      "cursor 旧快照回答",
    );
    renderPanel();
    await waitForConnectCount(1);

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("切换 Agent"));
    await user.click(await screen.findByRole("menuitem", { name: "Cursor" }));
    expect(
      (await screen.findAllByText("cursor 旧快照问题")).length,
    ).toBeGreaterThanOrEqual(1);
    await waitForConnectCount(2);
    const connectChannel = channels[channels.length - 1];
    await act(async () => {
      connectChannel?.onmessage?.({
        type: "sessionSaved",
        sessionId: "cursor-new-2",
        profileId: "cursor",
        cwd: CWD,
        resume: "unavailable",
      });
    });

    await waitFor(() => {
      expect(screen.queryByText("cursor 旧快照问题")).not.toBeInTheDocument();
    });
    expect(screen.queryByText("cursor 旧快照回答")).not.toBeInTheDocument();
    expect(
      (await screen.findAllByText(/已作为新对话继续/)).length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      useAcpSessionStore.getState().savedSessionFor("cursor")?.sessionId,
    ).toBe("cursor-new-2");
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-hint-1");
    expect(
      mockSnapshotTable.get(dbSnapshotKey("codex", "codex-hint-1")),
    ).toBeDefined();
  });

  it("history open resumes then reloads: cached empty replaced by loaded text with recovered banner", async () => {
    usePlayerStore.setState({ currentFile: MEDIA });
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-current-0",
      profileId: "codex",
      cwd: CWD,
    });
    mockAgentSessionList = {
      verified: true,
      sessions: [
        {
          sessionId: "hist-aaa-1111",
          cwd: CWD,
          title: "历史线程 A",
          updatedAt: "2026-09-01T10:00:00.000Z",
          kind: "chat",
        },
        {
          sessionId: "hist-bbb-2222",
          cwd: CWD,
          title: "历史线程 B",
          updatedAt: "2026-09-02T10:00:00.000Z",
          kind: "chat",
        },
      ],
      truncated: false,
    };
    mockTranscriptTable.set("hist-aaa-1111", [
      { role: "user", text: "历史远端问题 A" },
      { role: "agent", text: "历史远端回答 A" },
    ]);
    const { client } = renderPanel();
    await waitForConnectCount(1);

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("历史对话"));
    expect(await screen.findByText("历史线程 A")).toBeInTheDocument();
    expect(screen.getByText("历史线程 B")).toBeInTheDocument();
    await user.click(screen.getByText("历史线程 A"));
    await waitForSwitchCount(1);
    const switchChannel = channels[channels.length - 1];
    await act(async () => {
      switchChannel?.onmessage?.({
        type: "sessionSaved",
        sessionId: "hist-aaa-1111",
        profileId: "codex",
        cwd: CWD,
        resume: "resumed",
      });
    });

    expect(
      (await screen.findAllByText("历史远端问题 A")).length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("历史远端回答 A")).toBeInTheDocument();
    expect(
      (await screen.findAllByText(/已恢复/)).length,
    ).toBeGreaterThanOrEqual(1);
    expect(loadCallsFor("hist-aaa-1111").length).toBeGreaterThanOrEqual(1);
    const cached = client.getQueryData<LoadedTranscriptEvent[]>(
      acpQueryKeys.transcript("codex", CWD, "hist-aaa-1111"),
    );
    expect(cached?.length).toBeGreaterThanOrEqual(2);
  });

  it("history resume failure is honest: reports missing memory, skips wasted reload, keeps cache", async () => {
    usePlayerStore.setState({ currentFile: MEDIA });
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-current-0",
      profileId: "codex",
      cwd: CWD,
    });
    mockAgentSessionList = {
      verified: true,
      sessions: [
        {
          sessionId: "hist-bbb-2222",
          cwd: CWD,
          title: "历史线程 B",
          updatedAt: "2026-09-02T10:00:00.000Z",
          kind: "chat",
        },
      ],
      truncated: false,
    };
    const { client } = renderPanel();
    await waitForConnectCount(1);
    client.setQueryData<LoadedTranscriptEvent[]>(
      acpQueryKeys.transcript("codex", CWD, "hist-bbb-2222"),
      [
        { role: "user", text: "缓存问题 B" },
        { role: "agent", text: "缓存回答 B" },
      ],
    );

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("历史对话"));
    expect(await screen.findByText("历史线程 B")).toBeInTheDocument();
    await user.click(screen.getByText("历史线程 B"));
    await waitForSwitchCount(1);
    const switchChannel = channels[channels.length - 1];
    await act(async () => {
      switchChannel?.onmessage?.({
        type: "sessionSaved",
        sessionId: "codex-new-9",
        profileId: "codex",
        cwd: CWD,
        resume: "unavailable",
      });
    });

    expect(
      (await screen.findAllByText(/AI 记忆已不存在/)).length,
    ).toBeGreaterThanOrEqual(2);
    expect(screen.getByText(/仅可查看缓存/)).toBeInTheDocument();
    expect(loadCallsFor("hist-bbb-2222")).toHaveLength(0);
    const cached = client.getQueryData<LoadedTranscriptEvent[]>(
      acpQueryKeys.transcript("codex", CWD, "hist-bbb-2222"),
    );
    expect(cached?.length).toBeGreaterThanOrEqual(2);
    expect(
      (await screen.findAllByText("缓存问题 B")).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("end-to-end isolation: active world resumes while the other profile hint, rows and prefs stay intact", async () => {
    usePlayerStore.setState({ currentFile: MEDIA });
    useAcpSettingsStore
      .getState()
      .patchSettings({ permissionMode: "ask", thinkingLevel: "verbose" });
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-hint-1",
      profileId: "codex",
      cwd: CWD,
    });
    useAcpSessionStore.getState().setSavedSessionFor("cursor", {
      sessionId: "cursor-hint-1",
      profileId: "cursor",
      cwd: CWD,
    });
    seedDbSnapshot("codex", "codex-hint-1", "codex 旧问题", "codex 旧回答");
    mockTranscriptTable.set("cursor-hint-1", [
      { role: "user", text: "cursor 远端老问题" },
      { role: "agent", text: "cursor 远端老回答" },
    ]);
    renderPanel();
    await waitForConnectCount(1);

    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("切换 Agent"));
    await user.click(await screen.findByRole("menuitem", { name: "Cursor" }));
    await waitForConnectCount(2);
    const connectChannel = channels[channels.length - 1];
    await act(async () => {
      connectChannel?.onmessage?.({
        type: "sessionSaved",
        sessionId: "cursor-hint-1",
        profileId: "cursor",
        cwd: CWD,
        resume: "resumed",
      });
    });
    expect(
      (await screen.findAllByText("cursor 远端老问题")).length,
    ).toBeGreaterThanOrEqual(1);

    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-hint-1");
    expect(
      useAcpSessionStore.getState().savedSessionFor("cursor")?.sessionId,
    ).toBe("cursor-hint-1");
    expect(
      mockSnapshotTable.get(dbSnapshotKey("codex", "codex-hint-1")),
    ).toBeDefined();
    expect(useAcpSettingsStore.getState().permissionMode).toBe("ask");
    expect(useAcpSettingsStore.getState().thinkingLevel).toBe("verbose");
  });

  it("cleanup leaves no restore residue and the harness resets both tables between tests", async () => {
    expect(mockSnapshotTable.size).toBe(0);
    expect(mockHintTable.size).toBe(0);
    expect(
      Array.from({ length: localStorage.length }, (_, index) =>
        localStorage.key(index),
      ).filter(
        (key) => typeof key === "string" && key.startsWith("lumina-acp-chat-restore"),
      ),
    ).toEqual([]);

    usePlayerStore.setState({ currentFile: MEDIA });
    const { unmount } = renderPanel();
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/输入问题/)).toBeInTheDocument();
    });
    unmount();

    const restoreKeys = Array.from(
      { length: localStorage.length },
      (_, index) => localStorage.key(index),
    ).filter(
      (key) => typeof key === "string" && key.startsWith("lumina-acp-chat-restore"),
    );
    expect(restoreKeys).toEqual([]);
  });
});
