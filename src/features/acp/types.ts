export type AcpStatus = {
  available: boolean;
  cliPath?: string | null;
  codexPath?: string | null;
  message: string;
};

export type AcpEvent =
  | { type: "started" }
  | { type: "progress"; message: string }
  | { type: "agentMessage"; text: string }
  | { type: "finished"; text: string }
  | { type: "failed"; code: string; message: string };
