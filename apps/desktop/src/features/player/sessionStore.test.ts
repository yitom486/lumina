import { beforeEach, describe, expect, it } from "vitest";

import {
  parentDirectory,
  resolveRestorePositionMs,
  useSessionStore,
} from "./sessionStore";

beforeEach(() => {
  useSessionStore.setState({
    lastPath: null,
    lastDirectory: null,
    lastPositionMs: 0,
    updatedAt: 0,
  });
});

describe("parentDirectory", () => {
  it("returns parent folder on Windows-style paths", () => {
    expect(parentDirectory("D:\\movie\\show\\S01E01.mkv")).toBe("D:\\movie\\show");
  });

  it("returns null for remote URLs (never pollute dialogs/roots)", () => {
    expect(parentDirectory("https://www.youtube.com/watch?v=x")).toBeNull();
    expect(parentDirectory("http://example.com/a.mp4")).toBeNull();
  });
});

describe("saveSession", () => {
  it("keeps the previous local directory after online playback", () => {
    useSessionStore.getState().saveSession({
      path: "D:\\movie\\a.mkv",
      positionMs: 1000,
    });
    expect(useSessionStore.getState().lastDirectory).toBe("D:\\movie");
    // Play a YouTube URL: path updates (restore skips it), directory stays.
    useSessionStore.getState().saveSession({
      path: "https://www.youtube.com/watch?v=x",
      positionMs: 2000,
    });
    const state = useSessionStore.getState();
    expect(state.lastPath).toBe("https://www.youtube.com/watch?v=x");
    expect(state.lastDirectory).toBe("D:\\movie");
  });
});

describe("resolveRestorePositionMs", () => {
  it("prefers session snapshot for the same path", () => {
    expect(
      resolveRestorePositionMs(
        "D:\\a\\b.mkv",
        { lastPath: "D:\\a\\b.mkv", lastPositionMs: 120_000 },
        5_000,
      ),
    ).toBe(120_000);
  });

  it("falls back to progress store when session path differs", () => {
    expect(
      resolveRestorePositionMs(
        "D:\\a\\b.mkv",
        { lastPath: "D:\\other.mkv", lastPositionMs: 120_000 },
        8_000,
      ),
    ).toBe(8_000);
  });
});
