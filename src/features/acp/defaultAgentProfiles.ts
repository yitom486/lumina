import type { AgentProfileInput, AgentProfilesHint } from "./types";

export const CODEX_ACP_PACKAGE = "@agentclientprotocol/codex-acp";

function isWindowsPlatform(): boolean {
  if (typeof navigator !== "undefined" && navigator.platform) {
    return navigator.platform.toLowerCase().includes("win");
  }
  return typeof process !== "undefined" && process.platform === "win32";
}

export function defaultAgentProfiles(): AgentProfileInput[] {
  const bunx = isWindowsPlatform() ? "bunx.exe" : "bunx";
  const claude = isWindowsPlatform() ? "claude-agent-acp.exe" : "claude-agent-acp";
  return [
    {
      id: "codex",
      name: "Codex（默认）",
      kind: "Codex",
      command: bunx,
      args: [CODEX_ACP_PACKAGE],
      env: {},
    },
    {
      id: "claude",
      name: "Claude ACP",
      kind: "Claude",
      command: claude,
      args: [],
      env: {},
    },
    {
      id: "custom",
      name: "自定义 ACP",
      kind: "Custom",
      command: "",
      args: [],
      env: {},
    },
  ];
}

export function defaultProfilesHint(): AgentProfilesHint {
  return {
    activeProfileId: "codex",
    profiles: defaultAgentProfiles(),
  };
}

export function normalizeProfileInput(profile: AgentProfileInput): AgentProfileInput {
  return {
    ...profile,
    args: profile.args ?? [],
    env: profile.env ?? {},
  };
}

export function profilesHintFromStore(
  activeProfileId: string,
  profiles: AgentProfileInput[],
): AgentProfilesHint {
  return {
    activeProfileId,
    profiles: profiles.map(normalizeProfileInput),
  };
}
