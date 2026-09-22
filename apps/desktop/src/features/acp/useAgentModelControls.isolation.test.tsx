import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSettingsStore } from "@lumina/chat-ui/acpSettingsStore";
import type { AcpStatus } from "./types";
import { useAgentModelControls } from "./useAgentModelControls";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null;
  },
}));

import { invoke } from "@tauri-apps/api/core";

function statusFor(profileId: string): AcpStatus {
  return {
    available: true,
    adapterFound: true,
    codexFound: true,
    activeProfileId: profileId,
    profiles: [],
    message: "ok",
    sessionActive: false,
    busy: false,
  } as unknown as AcpStatus;
}

function optionsFor(profileId: string) {
  return {
    models: [{ value: `model-of-${profileId}`, name: `Model of ${profileId}` }],
    reasoningEfforts: [],
    currentModelId: null,
    currentReasoningEffort: null,
    extraOptions: [],
  };
}

function renderControls(profileId: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return renderHook(() => useAgentModelControls({ status: statusFor(profileId) }), {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  });
}

beforeEach(() => {
  localStorage.clear();
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  useAcpSettingsStore.setState({ modelId: "", reasoningEffort: "" });
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "library_agent_models_discover") {
      const profileId =
        (args as { config?: { profileId?: string } })?.config?.profileId ??
        "codex";
      return Promise.resolve({
        connected: true,
        options: optionsFor(profileId),
        message: "ok",
      });
    }
    return Promise.resolve(null);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("useAgentModelControls profile-bucketed discovery", () => {
  it("keeps each agent's discovered options in its own bucket across switches", async () => {
    const { result } = renderControls("codex");

    await act(async () => {
      await result.current.discoverModels();
    });
    expect(
      result.current.discoveredOptionsByProfile["codex"]?.models.map((m) => m.value),
    ).toEqual(["model-of-codex"]);

    // 切到 claude：旧桶不清全量，只是失活（当前读到空）。
    await act(async () => {
      useAcpProfilesStore.getState().switchActiveProfileId("claude");
    });
    expect(result.current.discoveredOptions).toBeNull();
    expect(
      result.current.discoveredOptionsByProfile["codex"]?.models.map((m) => m.value),
    ).toEqual(["model-of-codex"]);

    await act(async () => {
      await result.current.discoverModels();
    });
    expect(
      result.current.discoveredOptionsByProfile["claude"]?.models.map((m) => m.value),
    ).toEqual(["model-of-claude"]);
    // codex 桶纹丝不动，绝不串台。
    expect(
      result.current.discoveredOptionsByProfile["codex"]?.models.map((m) => m.value),
    ).toEqual(["model-of-codex"]);

    // 切回 codex：自家桶即复用，当前读回 codex 模型。
    await act(async () => {
      useAcpProfilesStore.getState().switchActiveProfileId("codex");
    });
    expect(result.current.discoveredOptions?.models.map((m) => m.value)).toEqual([
      "model-of-codex",
    ]);
  });

  it("keeps the busy guard (switch-time disable mirrors prompting refusal)", () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { result, rerender } = renderHook(
      ({ busy }: { busy: boolean }) =>
        useAgentModelControls({ status: statusFor("codex"), busy }),
      {
        initialProps: { busy: false },
        wrapper: ({ children }: { children: React.ReactNode }) => (
          <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
      },
    );
    expect(result.current.controlsDisabled).toBe(false);
    rerender({ busy: true });
    expect(result.current.controlsDisabled).toBe(true);
  });
});
