import { describe, expect, it } from "vitest";

import {
  parentDirectory,
  resolveRestorePositionMs,
} from "./sessionStore";

describe("parentDirectory", () => {
  it("returns parent folder on Windows-style paths", () => {
    expect(parentDirectory("D:\\movie\\show\\S01E01.mkv")).toBe("D:\\movie\\show");
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
