import type { Transcript } from "@/features/transcript";

export type AsrModelInfo = {
  id: string;
  path: string;
  sizeBytes?: number | null;
};

export type AsrStatus = {
  available: boolean;
  cliPath?: string | null;
  modelPath?: string | null;
  models?: AsrModelInfo[];
  message: string;
};

export type AsrRange =
  | { kind: "window"; fromMs: number; toMs: number }
  | { kind: "chapter"; chapterId: number };

export type AsrEvent =
  | { type: "Started"; payload: { path: string } }
  | { type: "Progress"; payload: { stage: string; message: string } }
  | { type: "Finished"; payload: { transcript: Transcript } }
  | { type: "Failed"; payload: { code: string; message: string } };
