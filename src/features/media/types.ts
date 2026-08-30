export type StreamKind =
  | "Video"
  | "Audio"
  | "Subtitle"
  | "Data"
  | "Attachment"
  | "Unknown";

export type MediaStream = {
  index: number;
  kind: StreamKind;
  codecName?: string | null;
  codecLongName?: string | null;
  width?: number | null;
  height?: number | null;
  frameRate?: number | null;
  sampleRate?: number | null;
  channels?: number | null;
  bitRate?: number | null;
  language?: string | null;
};

export type MediaChapter = {
  id: number;
  startMs: number;
  endMs?: number | null;
  title?: string | null;
};

export type MediaInfo = {
  path: string;
  formatName?: string | null;
  formatLongName?: string | null;
  durationMs?: number | null;
  sizeBytes?: number | null;
  bitRate?: number | null;
  streams: MediaStream[];
  chapters?: MediaChapter[];
};

export type MediaErrorDto = {
  code: string;
  message: string;
  details?: string;
};
