import { beforeEach, describe, expect, it } from "vitest";

import { mergeAcpSettings, useAcpSettingsStore } from "./acpSettingsStore";
import { transcriptWindowRadiusSec } from "./types";

beforeEach(() => {
  localStorage.clear();
  useAcpSettingsStore.setState({
    permissionMode: "auto",
    thinkingLevel: "minimal",
    agentMode: "default",
    visionCapable: false,
    modelId: "",
    reasoningEffort: "",
    transcriptWindowPreset: "standard",
  });
});

describe("useAcpSettingsStore", () => {
  it("persists model and reasoning selections", () => {
    useAcpSettingsStore.getState().patchSettings({
      modelId: "gpt-5.6-luna",
      reasoningEffort: "medium",
      permissionMode: "ask",
      agentMode: "plan",
      thinkingLevel: "verbose",
    });

    const raw = localStorage.getItem("lumina-acp-settings");
    expect(raw).toBeTruthy();
    const parsed = JSON.parse(raw!) as {
      state: {
        modelId: string;
        reasoningEffort: string;
        permissionMode: string;
        agentMode: string;
        thinkingLevel: string;
      };
    };

    expect(parsed.state.modelId).toBe("gpt-5.6-luna");
    expect(parsed.state.reasoningEffort).toBe("medium");
    expect(parsed.state.permissionMode).toBe("ask");
    expect(parsed.state.agentMode).toBe("plan");
    expect(parsed.state.thinkingLevel).toBe("verbose");
  });

  it("uses the standard transcript window and maps all presets", () => {
    expect(useAcpSettingsStore.getState().transcriptWindowPreset).toBe("standard");
    expect(transcriptWindowRadiusSec("compact")).toBe(15);
    expect(transcriptWindowRadiusSec("standard")).toBe(30);
    expect(transcriptWindowRadiusSec("expanded")).toBe(60);
  });

  it("keeps the normalized preset when a partial patch omits it", () => {
    useAcpSettingsStore.getState().patchSettings({ modelId: "gpt-5.6-luna" });

    expect(useAcpSettingsStore.getState().transcriptWindowPreset).toBe("standard");
  });

  it("merges legacy persisted settings with a compatible default", () => {
    const merged = mergeAcpSettings(
      { permissionMode: "ask", transcriptWindowPreset: "unexpected" },
      useAcpSettingsStore.getState(),
    );

    expect(merged.permissionMode).toBe("ask");
    expect(merged.transcriptWindowPreset).toBe("standard");
  });
});
