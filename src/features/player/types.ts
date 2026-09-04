/** Player domain types mirrored from Rust (camelCase JSON). */

export type PlayerErrorDto = {
  code: string;
  message: string;
  details?: string;
};

export type MediaSourceKind = "local" | "remote";

export type PlayerSnapshot = {
  status: string;
  currentTimeMs: number;
  durationMs: number;
  volume: number;
  rate: number;
  currentFile: string | null;
  /** Stable id for notes / history (`path` or `youtube:…` / `bilibili:…`). */
  mediaId?: string | null;
  sourceKind?: MediaSourceKind | null;
  /** Active online format id when remote; local stays null. */
  playbackFormatId?: string | null;
  error: PlayerErrorDto | null;
};

export type PlayerEvent =
  | { type: "StateChanged"; payload: { status: string } }
  | { type: "PositionChanged"; payload: { positionMs: number } }
  | { type: "DurationChanged"; payload: { durationMs: number } }
  | { type: "FileLoaded"; payload: { path: string; durationMs: number } }
  | { type: "Ended" }
  | { type: "Error"; payload: { error: PlayerErrorDto } }
  | { type: "SurfaceClick" }
  | { type: "SurfaceDoubleClick" };

export const IDLE_SNAPSHOT: PlayerSnapshot = {
  status: "Idle",
  currentTimeMs: 0,
  durationMs: 0,
  volume: 100,
  rate: 1,
  currentFile: null,
  mediaId: null,
  sourceKind: null,
  playbackFormatId: null,
  error: null,
};
