import type { Transcript } from "@/features/transcript";

export type AsrModelInfo = {
  id: string;
  path: string;
  sizeBytes?: number | null;
};

export type AsrCatalogModel = {
  id: string;
  fileName: string;
  label: string;
  approxBytes: number;
  installed: boolean;
};

export type AsrStatus = {
  available: boolean;
  cliPath?: string | null;
  modelPath?: string | null;
  models?: AsrModelInfo[];
  catalog?: AsrCatalogModel[];
  cliReady?: boolean;
  installSupported?: boolean;
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

export type AsrInstallEvent =
  | {
      type: "Progress";
      payload: {
        stage: string;
        message: string;
        bytesReceived?: number | null;
        bytesTotal?: number | null;
      };
    }
  | { type: "Finished"; payload: { status: AsrStatus } }
  | { type: "Failed"; payload: { code: string; message: string } };
