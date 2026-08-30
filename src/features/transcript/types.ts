export type SubtitleTrackInfo = {
  streamIndex: number;
  codecName?: string | null;
  language?: string | null;
  title?: string | null;
};

export type Cue = {
  index: number;
  startMs: number;
  endMs: number;
  text: string;
};

export type Transcript = {
  sourcePath: string;
  streamIndex?: number | null;
  language?: string | null;
  codecName?: string | null;
  cues: Cue[];
};
