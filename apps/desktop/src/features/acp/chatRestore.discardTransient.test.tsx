import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  chatRestoreKeyFor,
  discardTransientChatState,
  readChatRestore,
  scheduleClearChatRestore,
  schedulePersistChatRestore,
} from "./chatRestore";
import type { ChatTurn } from "./types";

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
    // 内存丢了，盘上什么都没多出来。
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
    // 盘上还是老快照：既没被覆盖，也没被删除。
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
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    expect(readChatRestore("claude")?.draft).toBe("claude 草稿");
  });

  it("ignores blank profile ids and composes with a real clear", () => {
    seedDisk("codex", "盘上老问题");
    discardTransientChatState("  ");
    // 空 id 是 no-op：随后排队的真删除照样生效（命名区分正在于此）。
    scheduleClearChatRestore("codex");
    vi.advanceTimersByTime(1500);
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
  });
});
