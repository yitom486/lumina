import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import { AcpPanel } from "./AcpPanel";
import { CHAT_RESTORE_KEY, readChatRestore, schedulePersistChatRestore } from "../chatRestore";

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

function seedSnapshot() {
  localStorage.setItem(
    CHAT_RESTORE_KEY,
    JSON.stringify({
      version: 1,
      profileId: "codex",
      cwd: null,
      draft: "",
      turns: [
        {
          id: "cached-1",
          userText: "上次退出前的问题",
          answer: "上次退出前的回答",
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

beforeEach(() => {
  channels.length = 0;
  localStorage.clear();
  useAcpSessionStore.setState({ savedSession: null });
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  usePlayerStore.setState({ currentFile: null, status: "Idle" });
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    if (cmd === "acp_connect") return Promise.resolve(null);
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

  it("writes a throttled snapshot and reads it back", () => {
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
    expect(localStorage.getItem(CHAT_RESTORE_KEY)).toBeNull();
    vi.advanceTimersByTime(1500);

    const snapshot = readChatRestore();
    expect(snapshot?.profileId).toBe("codex");
    expect(snapshot?.cwd).toBe("D:\\movie");
    expect(snapshot?.draft).toBe("未发完");
    expect(snapshot?.turns.map((turn) => turn.id)).toEqual(["t1"]);
  });

  it("removes the snapshot when there is nothing worth restoring", () => {
    localStorage.setItem(
      CHAT_RESTORE_KEY,
      JSON.stringify({
        version: 1,
        profileId: "codex",
        cwd: null,
        draft: "",
        turns: [
          {
            id: "t1",
            userText: "问题",
            answer: "回答",
            status: "done",
            activities: [],
            showActivities: false,
          },
        ],
        updatedAtMs: 1,
      }),
    );
    schedulePersistChatRestore(null);
    vi.advanceTimersByTime(1500);
    expect(localStorage.getItem(CHAT_RESTORE_KEY)).toBeNull();
  });

  it("rejects dirty snapshots instead of rendering them", () => {
    for (const dirty of [
      "not-json",
      JSON.stringify({ version: 999, profileId: "codex", turns: [] }),
      JSON.stringify({ version: 1, turns: [] }),
      JSON.stringify({ version: 1, profileId: "codex", cwd: 42, turns: [] }),
      JSON.stringify({ version: 1, profileId: "codex", draft: "", turns: [] }),
    ]) {
      localStorage.setItem(CHAT_RESTORE_KEY, dirty);
      expect(readChatRestore()).toBeNull();
    }
  });
});

describe("chat restore blackbox: instant render + auto resume", () => {
  it("renders the cached thread instantly with zero network before connect", async () => {
    seedSnapshot();
    renderPanel();

    // 秒开：首屏即见上次内容，不等 acp_connect。
    // 首句会同时出现在工具栏标题与气泡里，所以用 All 断言。
    const cached = await screen.findAllByText("上次退出前的问题");
    expect(cached.length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("上次退出前的回答")).toBeInTheDocument();
  });

  it("passes the persisted session hint to acp_connect for resume", async () => {
    seedSnapshot();
    useAcpSessionStore.getState().setSavedSession({
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
    seedSnapshot();
    useAcpSessionStore.getState().setSavedSession({
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
    seedSnapshot();
    useAcpSessionStore.getState().setSavedSession({
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
    localStorage.setItem(
      CHAT_RESTORE_KEY,
      JSON.stringify({
        version: 1,
        profileId: "claude",
        cwd: null,
        draft: "",
        turns: [
          {
            id: "cached-1",
            userText: "别的画像的问题",
            answer: "别的画像的回答",
            status: "done",
            activities: [],
            showActivities: false,
          },
        ],
        updatedAtMs: 1,
      }),
    );
    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });
    expect(screen.queryByText("别的画像的问题")).not.toBeInTheDocument();
  });
});
