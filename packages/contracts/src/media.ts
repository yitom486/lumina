/** Media DTOs mirrored from Rust (camelCase JSON). */

export type MediaChapter = {
  id: number;
  startMs: number;
  endMs?: number | null;
  title?: string | null;
};
