import { beforeEach, describe, expect, it } from "vitest";

import type { AgentProfileInput } from "./types";
import { defaultAgentProfiles } from "./defaultAgentProfiles";
import { mergeProfiles, useAcpProfilesStore } from "./acpProfilesStore";

beforeEach(() => {
  localStorage.clear();
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
});

describe("switchActiveProfileId (switch world entry)", () => {
  it("flips the active key to the target profile", () => {
    useAcpProfilesStore.getState().switchActiveProfileId("claude");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("claude");
  });

  it("is a no-op for the same id and for blank ids", () => {
    useAcpProfilesStore.getState().switchActiveProfileId("codex");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("codex");

    useAcpProfilesStore.getState().switchActiveProfileId("  ");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("codex");
  });

  it("keeps the profile list intact (switching never edits the registry)", () => {
    const before = useAcpProfilesStore.getState().profiles.map((p) => p.id);
    useAcpProfilesStore.getState().switchActiveProfileId("cursor");
    const after = useAcpProfilesStore.getState().profiles.map((p) => p.id);
    expect(after).toEqual(before);
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("cursor");
  });

  it("leaves setActiveProfileId as the bare setter", () => {
    // Bare setter flips blindly (legacy direct writes); the guarded entry above
    // is what UI switches must use.
    useAcpProfilesStore.getState().setActiveProfileId("cursor");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("cursor");
  });
});

describe("mergeProfiles (upgrade migration)", () => {
  const fallback = defaultAgentProfiles();
  const asInput = (items: unknown[]) => items as AgentProfileInput[];

  it("drops removed runtime entries by id or stale variant markers", () => {
    const persisted = asInput([
      {
        id: "antigravity",
        name: "Google Antigravity",
        kind: "Antigravity",
        command: "agy_acp_server.exe",
        launcher: "antigravity-acp",
        envPreset: "antigravity-proxy",
        authPolicy: "antigravity-oauth",
      },
      {
        id: "sneaky",
        name: "Renamed copy",
        kind: "Antigravity",
        command: "agy_acp_server.exe",
      },
      { id: "codex", name: "Codex（默认）", kind: "Codex", command: "bunx" },
    ]);
    const ids = mergeProfiles(persisted, fallback).map((p) => p.id);
    expect(ids).not.toContain("antigravity");
    expect(ids).not.toContain("sneaky");
    expect(ids).toContain("codex");
  });

  it("refreshes builtin display names while preserving the user's launch command", () => {
    const persisted = asInput([
      {
        id: "codex",
        name: "Codex（默认）",
        kind: "Codex",
        command: "C:\\tools\\bunx.exe",
        args: ["pkg"],
        env: { X: "1" },
      },
    ]);
    const codex = mergeProfiles(persisted, fallback).find(
      (p) => p.id === "codex",
    );
    expect(codex?.name).toBe("ChatGPT");
    expect(codex?.command).toBe("C:\\tools\\bunx.exe");
    expect(codex?.args).toEqual(["pkg"]);
    expect(codex?.env).toEqual({ X: "1" });
  });

  it("orders builtins per new defaults and keeps custom ids last", () => {
    const persisted = asInput([
      { id: "my-agent", name: "Mine", kind: "Custom", command: "my-acp" },
      { id: "cursor", name: "Cursor CLI", kind: "Cursor", command: "agent" },
    ]);
    const ids = mergeProfiles(persisted, fallback).map((p) => p.id);
    expect(ids.slice(0, fallback.length)).toEqual(
      fallback.map((p) => p.id),
    );
    expect(ids[ids.length - 1]).toBe("my-agent");
  });

  it("resets an active key pointing at a removed profile on rehydrate", async () => {
    localStorage.setItem(
      "lumina-acp-profiles",
      JSON.stringify({
        state: {
          activeProfileId: "antigravity",
          profiles: [
            {
              id: "antigravity",
              name: "Google Antigravity",
              kind: "Antigravity",
              command: "agy_acp_server.exe",
              launcher: "antigravity-acp",
              envPreset: "antigravity-proxy",
              authPolicy: "antigravity-oauth",
            },
            {
              id: "codex",
              name: "Codex（默认）",
              kind: "Codex",
              command: "bunx",
            },
          ],
        },
        version: 0,
      }),
    );
    await useAcpProfilesStore.persist.rehydrate();
    const state = useAcpProfilesStore.getState();
    expect(state.activeProfileId).toBe("codex");
    expect(state.profiles.map((p) => p.id)).not.toContain("antigravity");
    expect(
      state.profiles.find((p) => p.id === "codex")?.name,
    ).toBe("ChatGPT");
  });
});
