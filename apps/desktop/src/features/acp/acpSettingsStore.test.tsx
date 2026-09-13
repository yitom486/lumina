import { beforeEach, describe, expect, it } from "vitest";

import { useAcpSettingsStore } from "./acpSettingsStore";

beforeEach(() => {
  localStorage.clear();
  useAcpSettingsStore.setState({
    permissionMode: "auto",
    thinkingLevel: "minimal",
    agentMode: "default",
    visionCapable: false,
    modelId: "",
    reasoningEffort: "",
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
});
