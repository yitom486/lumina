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
  // Cursor 官方 CLI：Windows 优先 %LOCALAPPDATA%\cursor-agent\agent.cmd，
  // posix 优先 ~/.local/bin/agent，否则回落 PATH（后端 resolve）。
  const cursor = isWindowsPlatform() ? "agent.cmd" : "agent";
  const gemini = isWindowsPlatform() ? "gemini.cmd" : "gemini";
  const copilot = isWindowsPlatform() ? "copilot.cmd" : "copilot";
  const opencode = isWindowsPlatform() ? "opencode.exe" : "opencode";
  const deepseek = isWindowsPlatform() ? "bunx.exe" : "bunx";
  return [
    {
      id: "codex",
      name: "ChatGPT",
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
      id: "claude",
      name: "Claude",
      kind: "Claude",
      command: claude,
      args: [],
      env: {},
      // 官方渠道本地认证复用：Claude Code 登录（`claude login`）或
      // ANTHROPIC_API_KEY；命中则后端跳过 ACP authenticate。
      authPolicy: "claude-local",
    },
    {
      id: "gemini",
      name: "Gemini",
      kind: "Gemini",
      command: gemini,
      args: ["--acp"],
      env: {},
      // 复用本机 Gemini CLI 登录（~/.gemini/oauth_creds.json 或
      // GEMINI_API_KEY / GOOGLE_API_KEY）。
      authPolicy: "gemini-local",
    },
    {
      id: "copilot",
      name: "Copilot",
      kind: "Copilot",
      command: copilot,
      args: ["--acp", "--stdio"],
      env: {},
      // 复用本机 Copilot CLI 的 GitHub 登录（~/.copilot/config.json 或
      // COPILOT_GITHUB_TOKEN / GH_TOKEN / GITHUB_TOKEN）。
      authPolicy: "copilot-local",
    },
    {
      id: "opencode",
      name: "OpenCode",
      kind: "OpenCode",
      command: opencode,
      args: ["acp"],
      env: {},
      // 复用 `opencode auth login` 写入的 auth.json。
      authPolicy: "opencode-local",
    },
    {
      id: "cursor",
      name: "Cursor",
      kind: "Cursor",
      command: cursor,
      args: ["acp"],
      env: {},
      // 本地已有认证复用（agent login / CURSOR_API_KEY / CURSOR_AUTH_TOKEN），
      // 命中则后端跳过 ACP authenticate；会话走通用 session/list + hint resume。
      authPolicy: "cursor-local",
    },
    {
      id: "deepseek",
      name: "DeepSeek",
      kind: "DeepSeek",
      command: deepseek,
      args: ["-y", "@deepseek-ai/dsh", "--profile", "acp"],
      env: {},
      // 无 ACP 登录，靠 harness 自身凭证（DEEPSEEK_API_KEY）。
      authPolicy: "deepseek-key",
    },
    {
      id: "custom",
      name: "自定义",
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
