import { describe, expect, it, vi } from "vitest";

import { getLogDir, revealLogDir } from "./system";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

describe("system log export", () => {
  it("reveals the backend-provided log dir", async () => {
    vi.mocked(invoke).mockResolvedValue("C:\\logs");
    vi.mocked(revealItemInDir).mockResolvedValue(undefined);

    expect(await getLogDir()).toBe("C:\\logs");
    await revealLogDir();

    expect(invoke).toHaveBeenCalledWith("system_log_dir");
    expect(revealItemInDir).toHaveBeenCalledWith("C:\\logs");
  });
});
