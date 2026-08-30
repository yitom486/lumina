export type SubtitleSource = "Embedded" | "Sidecar";

export type SubtitleChoice = {
  id: string;
  source: SubtitleSource;
  label: string;
  supported: boolean;
  streamIndex?: number | null;
  externalPath?: string | null;
  codecName?: string | null;
  language?: string | null;
};

export type Cue = {
  index: number;
  startMs: number;
  endMs: number;
  text: string;
};

export type Transcript = {
  sourcePath: string;
  choiceId: string;
  streamIndex?: number | null;
  language?: string | null;
  codecName?: string | null;
  cues: Cue[];
};
