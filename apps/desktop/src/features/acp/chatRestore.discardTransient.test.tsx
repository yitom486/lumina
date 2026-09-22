import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  chatRestoreKeyFor,
  discardTransientChatState,
  readChatRestore,
  resetChatStoreEphemeralState,
  scheduleClearChatRestore,
  schedulePersistChatRestore,
} from "./chatRestore";
import type { ChatTurn } from "./types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
}));

function turn(id: string, userText: string): ChatTurn {
  return {
    id,
    userText,
    answer: "答",
    status: "done",
    activities: [],
    showActivities: false,
  };
}

function seedDisk(profileId: string, userText: string) {
  localStorage.setItem(
    chatRestoreKeyFor(profileId),
    JSON.stringify({
      version: 1,
      profileId,
      cwd: null,
      draft: "",
      turns: [turn(`disk-${profileId}`, userText)],
      updatedAtMs: 1,
    }),
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  // B3 的排队 timer/内存镜像/会话记忆跨用例不泄漏：每个用例从干净态起步。
  resetChatStoreEphemeralState();
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe("discardTransientChatState", () => {
  it("drops a scheduled write before it lands (memory only)", () => {
    schedulePersistChatRestore({
      profileId: "codex",
      cwd: null,
      draft: "未发完",
      turns: [turn("t1", "问题")],
    });
    discardTransientChatState("codex");
    vi.advanceTimersByTime(1500);
    // 内存丢了，DB 没收到，备层从不写入。
    expect(readChatRestore("codex")).toBeNull();
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
  });

  it("never touches the landed snapshot", () => {
    seedDisk("codex", "盘上老问题");
    schedulePersistChatRestore({
      profileId: "codex",
      cwd: null,
      draft: "新草稿",
      turns: [turn("t-new", "新问题")],
    });
    discardTransientChatState("codex");
    vi.advanceTimersByTime(1500);
    // 落定内容（此处为迁移前备层）还在：既没被覆盖，也没被删除。
    // B3 原语义保持：discard 只删 pending + 内存，绝不删已落定的 DB 行/备层。
    expect(readChatRestore("codex")?.turns.map((t) => t.id)).toEqual([
      "disk-codex",
    ]);
  });

  it("is scoped to one profile (other worlds keep flushing)", () => {
    schedulePersistChatRestore({
      profileId: "codex",
      cwd: null,
      draft: "codex 草稿",
      turns: [turn("t-codex", "codex 问")],
    });
    schedulePersistChatRestore({
      profileId: "claude",
      cwd: null,
      draft: "claude 草稿",
      turns: [turn("t-claude", "claude 问")],
    });
    discardTransientChatState("codex");
    vi.advanceTimersByTime(1500);
    expect(readChatRestore("codex")).toBeNull();
    expect(readChatRestore("claude")?.draft).toBe("claude 草稿");
  });

  it("ignores blank profile ids and composes with a real clear", () => {
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: null,
      draft: "草稿",
      turns: [turn("t1", "问题")],
    });
    discardTransientChatState("  ");
    // 空 id 是 no-op：内存与排队都不受影响，随后排队的真删除照样生效。
    expect(readChatRestore("codex", "s-codex")?.draft).toBe("草稿");
    scheduleClearChatRestore("codex", "s-codex");
    vi.advanceTimersByTime(1500);
    expect(readChatRestore("codex", "s-codex")).toBeNull();
  });
});
