import { describe, expect, it } from "vitest";

import {
  buildAgentModelOptions,
  isDirectResolverReady,
  mergeAgentDiscoverySettings,
} from "./resolverSettings";
import type { AgentModelDiscoveryResult } from "./types";

const connectedResult = (
  overrides: Partial<AgentModelDiscoveryResult> = {},
): AgentModelDiscoveryResult => ({
  connected: true,
  message: "ok",
  options: {
    models: [
      { value: "gpt-4o-mini", name: "Mini" },
      { value: "gpt-4o-luna", name: "Luna" },
    ],
    reasoningEfforts: [{ value: "medium", name: "Medium" }],
    currentModelId: "gpt-4o-mini",
    currentReasoningEffort: "minimal",
  },
  ...overrides,
});

describe("mergeAgentDiscoverySettings", () => {
  it("does not overwrite saved model on reconnect", () => {
    const patch = mergeAgentDiscoverySettings(
      { agentModelId: "gpt-4o-luna", agentReasoningEffort: "medium" },
      connectedResult(),
    );
    expect(patch).toEqual({});
  });

  it("fills defaults only when saved values are empty", () => {
    const patch = mergeAgentDiscoverySettings(
      { agentModelId: "", agentReasoningEffort: "" },
      connectedResult(),
    );
    expect(patch).toEqual({
      agentModelId: "gpt-4o-mini",
      agentReasoningEffort: "minimal",
    });
  });
});

describe("buildAgentModelOptions", () => {
  it("keeps saved model visible when disconnected", () => {
    expect(
      buildAgentModelOptions(null, "gpt-4o-luna").map((option) => option.value),
    ).toEqual(["gpt-4o-luna"]);
  });
});

describe("isDirectResolverReady", () => {
  it("works with saved api key without live connection", () => {
    expect(
      isDirectResolverReady({
        modelId: "gpt-4o-mini",
        modelBaseUrl: "https://example.com/v1",
        modelApiKeySaved: true,
        pendingApiKey: "",
      }),
    ).toBe(true);
  });
});
