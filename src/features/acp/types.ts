import type { VideoAnnotationProposal } from "@/features/notes/proposalTypes";

export type AgentKind = "Codex" | "Claude" | "Custom";

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

export type AcpStatus = {
  available: boolean;
  adapterFound: boolean;
  codexFound: boolean;
  codexConfigFound?: boolean;
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
  chapterTitle?: string | null;
  subtitleChoiceId?: string | null;
  notesExcerpt?: string | null;
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

export type ChatMessageStatus = "streaming" | "done" | "error";

export type ChatTurn = {
  id: string;
  userText: string;
  answer: string;
  status: ChatMessageStatus;
  activities: ChatActivity[];
  showActivities: boolean;
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
