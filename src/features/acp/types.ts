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

export type AcpStatus = {
  available: boolean;
  adapterFound: boolean;
  codexFound: boolean;
  activeProfileId: string;
  profiles: AgentProfileStatus[];
  cliPath?: string | null;
  codexPath?: string | null;
  message: string;
  hint: string;
  responsesOnlyNote: string;
  sessionActive: boolean;
  busy: boolean;
};

export type AgentProfileInput = {
  id: string;
  name: string;
  kind: AgentKind;
  command: string;
  args?: string[];
  env?: Record<string, string>;
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
    }
  | {
      type: "toolCallUpdate";
      toolCallId: string;
      status?: string | null;
      title?: string | null;
    }
  | { type: "plan"; text: string }
  | {
      type: "permissionResolved";
      toolCallId?: string | null;
      decision: string;
    }
  | { type: "finished"; text: string; stopReason?: string | null }
  | { type: "failed"; code: string; message: string };

/** UI chat shell — video context linkage comes later. */
export type ChatRole = "user" | "assistant" | "system";

export type ChatMessageStatus = "streaming" | "done" | "error";

export type ChatMessage = {
  id: string;
  role: ChatRole;
  content: string;
  status?: ChatMessageStatus;
};
