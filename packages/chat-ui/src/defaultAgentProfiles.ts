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
  const antigravity = isWindowsPlatform() ? "agy_acp_server.exe" : "agy_acp_server";
  // Cursor 官方 CLI：Windows 优先 %LOCALAPPDATA%\cursor-agent\agent.cmd，
  // posix 优先 ~/.local/bin/agent，否则回落 PATH（后端 resolve）。
  const cursor = isWindowsPlatform() ? "agent.cmd" : "agent";
  return [
    {
      id: "codex",
      name: "Codex（默认）",
      kind: "Codex",
      command: bunx,
      args: [CODEX_ACP_PACKAGE],
      env: {},
      launcher: "codex-acp",
      envPreset: "codex-cli",
      authPolicy: "codex-local",
      sessionStorage: "codex-rollouts",
    },
    {
      id: "antigravity",
      name: "Google Antigravity",
      kind: "Antigravity",
      command: antigravity,
      args: [],
      env: { ACP_PROXY_PORT: "7897" },
      launcher: "antigravity-acp",
      envPreset: "antigravity-proxy",
      authPolicy: "antigravity-oauth",
      authMethods: ["oauth-personal"],
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
      id: "cursor",
      name: "Cursor CLI",
      kind: "Cursor",
      command: cursor,
      args: ["acp"],
      env: {},
      // 本地已有认证复用（agent login / CURSOR_API_KEY / CURSOR_AUTH_TOKEN），
      // 命中则后端跳过 ACP authenticate；会话走通用 session/list + hint resume。
      authPolicy: "cursor-local",
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
  profiles: AgentProfileInput[] | undefined,
): AgentProfilesHint {
  const safeProfiles =
    Array.isArray(profiles) && profiles.length > 0
      ? profiles
      : defaultAgentProfiles();
  return {
    activeProfileId: activeProfileId || "codex",
    profiles: safeProfiles.map(normalizeProfileInput),
  };
}
