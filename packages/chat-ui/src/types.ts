export type { NoteQuote, VideoAnnotationProposal } from "@lumina/contracts";
import type { VideoAnnotationProposal } from "@lumina/contracts";

export type AgentKind = "Codex" | "Claude" | "Antigravity" | "Custom";

/** Per-profile behavior presets; absent = fully generic ACP behavior. */
export type AgentLauncherPreset = "codex-acp" | "antigravity-acp";
export type AgentEnvPreset = "codex-cli" | "antigravity-proxy";
export type AgentAuthPolicy = "codex-local" | "antigravity-oauth";
export type AgentSessionStoragePreset = "codex-rollouts";

export type AgentProfileStatus = {
  id: string;
  name: string;
  kind: AgentKind;
  command: string;
  args: string[];
  env: Record<string, string>;
  available: boolean;
  resolvedCommand?: string | null;
};

export type SavedSessionHint = {
  sessionId: string;
  profileId: string;
  cwd: string;
};

/**
 * 恢复 AI 记忆的结果。`occupied` 与 `unavailable` 必须分开：被占用的对话内容
 * 完好，占用方放手后还能再恢复；unavailable 才是真的没了。
 */
export type ResumeOutcome = "resumed" | "occupied" | "unavailable";

export type AgentSessionInfo = {
  sessionId: string;
  cwd: string;
  title: string | null;
  updatedAt: string | null;
  kind: string | null;
};

export type AgentSessionListResult = {
  verified: boolean;
  sessions: AgentSessionInfo[];
  truncated: boolean;
};

export type AcpStatus = {
  available: boolean;
  adapterFound: boolean;
  codexFound: boolean;
  codexConfigFound?: boolean;
  antigravityFound?: boolean;
  antigravityCredentialsFound?: boolean;
  activeProfileId: string;
  profiles: AgentProfileStatus[];
  cliPath?: string | null;
  codexPath?: string | null;
  message: string;
  hint: string;
  responsesOnlyNote: string;
  sessionActive: boolean;
  busy: boolean;
  sessionModelOptions?: AcpSessionModelOptions | null;
};

export type AcpSessionOption = {
  value: string;
  name: string;
  description?: string | null;
};

export type AcpSessionModelOptions = {
  models: AcpSessionOption[];
  reasoningEfforts: AcpSessionOption[];
  currentModelId?: string | null;
  currentReasoningEffort?: string | null;
};

export type AcpModelDiscoveryResult = {
  connected: boolean;
  options: AcpSessionModelOptions;
  message: string;
};

export type AgentProfileInput = {
  id: string;
  name: string;
  kind: AgentKind;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  launcher?: AgentLauncherPreset;
  envPreset?: AgentEnvPreset;
  authPolicy?: AgentAuthPolicy;
  authMethods?: string[];
  sessionStorage?: AgentSessionStoragePreset;
};

export type AgentProfilesHint = {
  activeProfileId: string;
  profiles: AgentProfileInput[];
};

export type VideoPromptContext = {
  mediaPath?: string | null;
  mediaTitle?: string | null;
  positionMs?: number | null;
  durationMs?: number | null;
  subtitleChoiceId?: string | null;
  season?: number | null;
  episode?: number | null;
  episodeTitle?: string | null;
  episodeOverview?: string | null;
};

export type PermissionMode = "auto" | "ask";
export type ThinkingLevel = "hidden" | "minimal" | "verbose";

/** Agent warm-up state when the chat tab is open. */
export type AcpConnectionState =
  | "unavailable"
  | "idle"
  | "connecting"
  | "connected"
  | "error";

export type AcpClientSettings = {
  permissionMode: PermissionMode;
  thinkingLevel: ThinkingLevel;
  agentMode: string;
  visionCapable?: boolean;
  modelId?: string | null;
  reasoningEffort?: string | null;
};

export type PermissionOption = {
  optionId: string;
  name: string;
  kind?: string | null;
};

export type AcpEvent =
  | { type: "started" }
  | { type: "progress"; message: string }
  | { type: "agentMessage"; text: string }
  | { type: "agentThought"; text: string }
  | {
      type: "toolCall";
      toolCallId: string;
      title?: string | null;
      kind?: string | null;
      status?: string | null;
      detail?: string | null;
    }
  | {
      type: "toolCallUpdate";
      toolCallId: string;
      status?: string | null;
      title?: string | null;
      detail?: string | null;
      appendDetail?: boolean;
    }
  | { type: "plan"; text: string }
  | {
      type: "permissionRequest";
      requestId: string;
      toolCallId?: string | null;
      title?: string | null;
      options: PermissionOption[];
    }
  | {
      type: "permissionResolved";
      toolCallId?: string | null;
      decision: string;
    }
  | {
      type: "sessionSaved";
      sessionId: string;
      profileId: string;
      cwd: string;
      /** 缺省表示本次没有尝试恢复，即一条全新对话。 */
      resume?: ResumeOutcome | null;
    }
  | { type: "finished"; text: string; stopReason?: string | null }
  | { type: "failed"; code: string; message: string };

export type ChatActivityKind = "thought" | "tool" | "plan";

export type ChatActivity = {
  id: string;
  kind: ChatActivityKind;
  toolCallId?: string;
  title?: string;
  status?: string;
  text?: string;
};

/** User-pasted image: kept as data URL for <img> preview and turn display. */
export type ChatImageAttachment = {
  id: string;
  mimeType: string;
  dataUrl: string;
};

export type ChatMessageStatus = "streaming" | "done" | "error";

export type ChatTurn = {
  id: string;
  userText: string;
  answer: string;
  status: ChatMessageStatus;
  activities: ChatActivity[];
  showActivities: boolean;
  /** Pasted images shown above the question; absent on old/loaded turns. */
  images?: ChatImageAttachment[];
  /** Question anchor (frozen at keystroke); used for note saving. Absent on old turns. */
  anchorMs?: number | null;
  /** Pre-tool agent stream; sealed when a tool call starts. */
  agentDraft?: string;
  /** Sealed agent segments before tool calls (not shown in UI). */
  agentSegments?: string[];
  /** Shown under error answer — fixed business copy, not technical details. */
  errorHint?: string;
  /** Agent-proposed annotation awaiting user confirmation under this turn. */
  annotationProposal?: VideoAnnotationProposal;
  /** User confirmed and saved the proposal for this turn. */
  annotationProposalSaved?: boolean;
};

export type PendingPermission = {
  requestId: string;
  toolCallId?: string | null;
  title?: string | null;
  options: PermissionOption[];
};
