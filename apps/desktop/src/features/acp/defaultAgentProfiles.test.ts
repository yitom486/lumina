import { describe, expect, it } from "vitest";

import {
  defaultAgentProfiles,
  profilesHintFromStore,
} from "@lumina/chat-ui/defaultAgentProfiles";

describe("profilesHintFromStore", () => {
  it("falls back when persisted profiles are missing", () => {
    const hint = profilesHintFromStore("codex", undefined);
    expect(hint.activeProfileId).toBe("codex");
    expect(hint.profiles).toEqual(defaultAgentProfiles());
  });

  it("ships a cursor profile with local-auth reuse and acp args", () => {
    const cursor = defaultAgentProfiles().find(
      (profile) => profile.id === "cursor",
    );
    expect(cursor).toMatchObject({
      kind: "Cursor",
      args: ["acp"],
      authPolicy: "cursor-local",
    });
    expect(cursor?.command.trim()).not.toBe("");
  });

  it("ships a claude profile with official local-auth reuse", () => {
    const claude = defaultAgentProfiles().find(
      (profile) => profile.id === "claude",
    );
    expect(claude).toMatchObject({
      kind: "Claude",
      authPolicy: "claude-local",
    });
    expect(claude?.command.trim()).not.toBe("");
  });

  it("ships the full agent lineup with per-agent commands and policies", () => {
    const profiles = defaultAgentProfiles();
    const byId = (id: string) => profiles.find((profile) => profile.id === id);
    expect(byId("gemini")).toMatchObject({
      kind: "Gemini",
      args: ["--acp"],
      authPolicy: "gemini-local",
    });
    expect(byId("copilot")).toMatchObject({
      kind: "Copilot",
      args: ["--acp", "--stdio"],
      authPolicy: "copilot-local",
    });
    expect(byId("opencode")).toMatchObject({
      kind: "OpenCode",
      args: ["acp"],
      authPolicy: "opencode-local",
    });
    expect(byId("deepseek")).toMatchObject({
      kind: "DeepSeek",
      args: ["-y", "@deepseek-ai/dsh", "--profile", "acp"],
      authPolicy: "deepseek-key",
    });
    for (const profile of profiles) {
      if (profile.id === "custom") continue;
      expect(profile.command.trim()).not.toBe("");
    }
  });

  it("ships display names without technical suffixes in the default order", () => {
    const profiles = defaultAgentProfiles();
    expect(profiles.map((profile) => profile.id)).toEqual([
      "codex",
      "claude",
      "gemini",
      "copilot",
      "opencode",
      "cursor",
      "deepseek",
      "custom",
    ]);
    expect(profiles.map((profile) => profile.name)).toEqual([
      "ChatGPT",
      "Claude",
      "Gemini",
      "Copilot",
      "OpenCode",
      "Cursor",
      "DeepSeek",
      "自定义",
    ]);
  });
});
