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
  | { type: "finished"; text: string }
  | { type: "failed"; code: string; message: string };
