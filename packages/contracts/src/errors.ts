/**
 * Canonical IPC error envelope (canonical source; migrated from feature types).
 *
 * Shape mirrors the Rust domain errors over Tauri commands:
 * UI shows only `message`; `details` stays in logs.
 */

export type AppError = {
  code: string;
  message: string;
  details?: string;
};

export type PlayerErrorDto = {
  code: string;
  message: string;
  details?: string;
};
