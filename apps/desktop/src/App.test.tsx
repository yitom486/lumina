import { describe, expect, it } from "vitest";

import {
  isSettingsWorkspace,
  shouldRenderPlayerWorkspace,
  WORKSPACE_RAIL_ITEMS,
} from "./App";

describe("App workspace layout", () => {
  it("keeps online resources inside Settings instead of the top-level rail", () => {
    expect(WORKSPACE_RAIL_ITEMS.map((item) => item.id)).toEqual([
      "playlist",
      "transcript",
      "notes",
      "chapters",
      "library",
      "settings",
    ]);
    expect(WORKSPACE_RAIL_ITEMS.map((item) => item.label)).not.toContain(
      "在线资源",
    );
  });

  it("makes Settings replace the player only outside fullscreen", () => {
    expect(isSettingsWorkspace(false, "settings")).toBe(true);
    expect(shouldRenderPlayerWorkspace(false, "settings")).toBe(false);
    expect(isSettingsWorkspace(true, "settings")).toBe(false);
    expect(shouldRenderPlayerWorkspace(true, "settings")).toBe(true);
    expect(shouldRenderPlayerWorkspace(false, "transcript")).toBe(true);
  });
});
