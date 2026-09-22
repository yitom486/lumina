import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import { AcpPanel } from "./AcpPanel";
import {
  CHAT_RESTORE_KEY,
  chatRestoreKeyFor,
  readChatRestore,
  scheduleClearChatRestore,
  schedulePersistChatRestore,
} from "../chatRestore";

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

const mockStatus = {
  available: true,
  adapterFound: true,
  codexFound: true,
  activeProfileId: "codex",
  profiles: [],
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

async function waitForConnect() {
  await waitFor(() => {
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_connect"),
    ).toHaveLength(1);
  });
  expect(channels[0]?.onmessage).toBeTypeOf("function");
}

function connectSessionIds(): (string | null)[] {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "acp_connect")
    .map(
      ([, args]) =>
        (args as { savedSession?: { sessionId?: string } | null })?.savedSession
          ?.sessionId ?? null,
    );
}

beforeEach(() => {
  channels.length = 0;
  localStorage.clear();
  useAcpSessionStore.setState({ savedSessions: {} });
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  usePlayerStore.setState({ currentFile: null, status: "Idle" });
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    if (cmd === "acp_connect") return Promise.resolve(null);
    if (cmd === "acp_load_session")
      return Promise.resolve([
        { role: "user", text: "cursor 远端老问题" },
        { role: "agent", text: "cursor 远端老回答" },
      ]);
    return Promise.resolve(null);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("chat restore persistence (happy-dom localStorage)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("writes a throttled snapshot to the profile-scoped key and reads it back", () => {
    schedulePersistChatRestore({
      profileId: "codex",
      cwd: "D:\\movie",
      draft: "未发完",
      turns: [
        {
          id: "t1",
          userText: "问题",
          answer: "上次的回答",
          status: "done",
          activities: [],
          showActivities: false,
        },
      ],
    });
    // 节流：时间没到不落盘。
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    vi.advanceTimersByTime(1500);

    const snapshot = readChatRestore("codex");
    expect(snapshot?.profileId).toBe("codex");
    expect(snapshot?.cwd).toBe("D:\\movie");
    expect(snapshot?.draft).toBe("未发完");
    expect(snapshot?.turns.map((turn) => turn.id)).toEqual(["t1"]);
    // 别家的键纹丝不动。
    expect(localStorage.getItem(chatRestoreKeyFor("claude"))).toBeNull();
  });

  it("removes only the owning profile snapshot when there is nothing worth restoring", () => {
    seedSnapshot("codex", "问题", "回答");
    seedSnapshot("claude", "别家问题", "别家回答");
    scheduleClearChatRestore("codex");
    vi.advanceTimersByTime(1500);
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    expect(readChatRestore("claude")?.turns.map((turn) => turn.id)).toEqual([
      "cached-claude",
    ]);
  });

  it("rejects dirty snapshots instead of rendering them", () => {
    for (const dirty of [
      "not-json",
      JSON.stringify({ version: 999, profileId: "codex", turns: [] }),
      JSON.stringify({ version: 1, turns: [] }),
      JSON.stringify({ version: 1, profileId: "codex", cwd: 42, turns: [] }),
      JSON.stringify({ version: 1, profileId: "codex", draft: "", turns: [] }),
    ]) {
      localStorage.setItem(chatRestoreKeyFor("codex"), dirty);
      expect(readChatRestore("codex")).toBeNull();
    }
  });

  it("falls back to the legacy single key only on profile match", () => {
    localStorage.setItem(
      CHAT_RESTORE_KEY,
      JSON.stringify({
        version: 1,
        profileId: "codex",
        cwd: null,
        draft: "",
        turns: [
          {
            id: "legacy-1",
            userText: "升级前的旧快照",
            answer: "旧回答",
            status: "done",
            activities: [],
            showActivities: false,
          },
        ],
        updatedAtMs: 1,
      }),
    );
    // 同 profile 认一次（下次落盘迁到分键），别家的一律拒掉。
    expect(
      readChatRestore("codex")?.turns.map((turn) => turn.id),
    ).toEqual(["legacy-1"]);
    expect(readChatRestore("claude")).toBeNull();
  });

  it("refuses blank profile ids instead of writing to the bare key", () => {
    schedulePersistChatRestore({
      profileId: "  ",
      cwd: null,
      draft: "x",
      turns: [],
    });
    scheduleClearChatRestore("  ");
    vi.advanceTimersByTime(1500);
    expect(localStorage.getItem(CHAT_RESTORE_KEY)).toBeNull();
  });
});

describe("chat restore blackbox: instant render + auto resume", () => {
  it("renders the cached thread instantly with zero network before connect", async () => {
    seedSnapshot("codex", "上次退出前的问题", "上次退出前的回答");
    renderPanel();

    // 秒开：首屏即见上次内容，不等 acp_connect。
    // 首句会同时出现在工具栏标题与气泡里，所以用 All 断言。
    const cached = await screen.findAllByText("上次退出前的问题");
    expect(cached.length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("上次退出前的回答")).toBeInTheDocument();
  });

  it("passes the persisted session hint to acp_connect for resume", async () => {
    seedSnapshot("codex", "上次退出前的问题", "上次退出前的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "old-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();

    await waitFor(() => {
      const connect = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === "acp_connect");
      expect(connect?.[1]).toMatchObject({
        savedSession: expect.objectContaining({ sessionId: "old-thread" }),
      });
    });
  });

  it("keeps cached turns when the backend resumes the same session", async () => {
    seedSnapshot("codex", "上次退出前的问题", "上次退出前的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "old-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();
    expect((await screen.findAllByText("上次退出前的问题")).length).toBeGreaterThanOrEqual(1);
    await waitForConnect();

    await act(async () => {
      channels[0]?.onmessage?.({
        type: "sessionSaved",
        sessionId: "old-thread",
        profileId: "codex",
        cwd: "D:\\movie",
        resume: "resumed",
      });
    });

    expect(
      (await screen.findAllByText("上次退出前的问题")).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("drops cached turns when the backend lands on a different new session", async () => {
    seedSnapshot("codex", "上次退出前的问题", "上次退出前的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "old-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();
    expect((await screen.findAllByText("上次退出前的问题")).length).toBeGreaterThanOrEqual(1);
    await waitForConnect();

    await act(async () => {
      channels[0]?.onmessage?.({
        type: "sessionSaved",
        sessionId: "brand-new",
        profileId: "codex",
        cwd: "D:\\movie",
        resume: null,
      });
    });

    await waitFor(() => {
      expect(screen.queryAllByText("上次退出前的问题")).toHaveLength(0);
    });
    expect(screen.queryByText("上次退出前的回答")).not.toBeInTheDocument();
  });

  it("ignores snapshots from another profile", async () => {
    seedSnapshot("claude", "别的画像的问题", "别的画像的回答");
    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });
    expect(screen.queryByText("别的画像的问题")).not.toBeInTheDocument();
  });

  it("keeps each agent's session hint and snapshot across profile switches", async () => {
    seedSnapshot("codex", "codex 的问题", "codex 的回答");
    seedSnapshot("claude", "claude 的问题", "claude 的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    useAcpSessionStore.getState().setSavedSessionFor("claude", {
      sessionId: "claude-thread",
      profileId: "claude",
      cwd: "D:\\movie",
    });
    renderPanel();

    // codex 世界：自家快照秒开。
    expect((await screen.findAllByText("codex 的问题")).length).toBeGreaterThanOrEqual(1);
    await waitForConnect();

    // 切到 claude：面板换成 claude 的快照，codex 的 hint 原样保留（以前这里会被删掉）。
    await act(async () => {
      useAcpProfilesStore.setState({ activeProfileId: "claude" });
    });
    expect((await screen.findAllByText("claude 的问题")).length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByText("codex 的问题")).not.toBeInTheDocument();
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-thread");
    expect(
      useAcpSessionStore.getState().savedSessionFor("claude")?.sessionId,
    ).toBe("claude-thread");

    // 切回 codex：自家快照与 hint 都在，直接 resume。
    await act(async () => {
      useAcpProfilesStore.setState({ activeProfileId: "codex" });
    });
    expect((await screen.findAllByText("codex 的问题")).length).toBeGreaterThanOrEqual(1);
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-thread");

    // 三次 connect 带的都是当时世界的自家 hint，绝不串台。
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_connect"),
      ).toHaveLength(3);
    });
    expect(connectSessionIds()).toEqual([
      "codex-thread",
      "claude-thread",
      "codex-thread",
    ]);
  });

  it("reloads the resumed thread text when the panel has no cached snapshot", async () => {
    // 快照缺失（localStorage 是空的）但 hint 还在：resume 续记忆，
    // 这次 reload 把远端真实文本摆出来——切到有记忆的 Agent 不再看空白框。
    useAcpSessionStore.getState().setSavedSessionFor("cursor", {
      sessionId: "cursor-thread",
      profileId: "cursor",
      cwd: "D:\\movie",
    });
    useAcpProfilesStore.setState({ activeProfileId: "cursor" });
    renderPanel();
    await waitForConnect();

    await act(async () => {
      channels[0]?.onmessage?.({
        type: "sessionSaved",
        sessionId: "cursor-thread",
        profileId: "cursor",
        cwd: "D:\\movie",
        resume: "resumed",
      });
    });

    expect(
      (await screen.findAllByText("cursor 远端老问题")).length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("cursor 远端老回答")).toBeInTheDocument();
    // 文本走的是自家线程的 load，不是编出来的。
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_load_session"),
    ).not.toHaveLength(0);
  });
});
