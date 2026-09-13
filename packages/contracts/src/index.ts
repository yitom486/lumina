/** Canonical IPC DTOs. Pure types only — no React, stores, invoke, or native code. */

export type { AppError, PlayerErrorDto } from "./errors";
export type { MediaSourceKind, PlayerEvent, PlayerSnapshot } from "./player";
export { IDLE_SNAPSHOT } from "./player";
export type { Cue, SubtitleChoice, SubtitleSource, Transcript } from "./subtitle";
