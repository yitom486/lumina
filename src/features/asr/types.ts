import type { Transcript } from "@/features/transcript";

export type AsrStatus = {
  available: boolean;
  cliPath?: string | null;
  modelPath?: string | null;
  message: string;
};

export type AsrEvent =
  | { type: "Started"; payload: { path: string } }
  | { type: "Progress"; payload: { stage: string; message: string } }
  | { type: "Finished"; payload: { transcript: Transcript } }
  | { type: "Failed"; payload: { code: string; message: string } };
