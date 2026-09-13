import { describe, expect, it } from "vitest";

import { shouldSuppressPlayerHotkeyDuringAcp } from "./playerHotkeyPolicy";

describe("shouldSuppressPlayerHotkeyDuringAcp", () => {
  it("allows space while agent is responding", () => {
    expect(shouldSuppressPlayerHotkeyDuringAcp("Space", true)).toBe(false);
  });

  it("suppresses other keys while agent is responding", () => {
    expect(shouldSuppressPlayerHotkeyDuringAcp("ArrowLeft", true)).toBe(true);
    expect(shouldSuppressPlayerHotkeyDuringAcp("KeyM", true)).toBe(true);
    expect(shouldSuppressPlayerHotkeyDuringAcp("KeyF", true)).toBe(true);
  });

  it("does not suppress when agent is idle", () => {
    expect(shouldSuppressPlayerHotkeyDuringAcp("ArrowLeft", false)).toBe(false);
    expect(shouldSuppressPlayerHotkeyDuringAcp("Space", false)).toBe(false);
  });
});
