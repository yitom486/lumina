/** Transcript cue/time selection view-model (no Tauri, no stores, no invoke). */

import type { Cue } from "@lumina/contracts";

export function activeCueIndex(cues: Cue[], timeMs: number): number {
  return cues.findIndex((c) => timeMs >= c.startMs && timeMs < c.endMs);
}

export type AsrScope = "full" | "chapter";

export const LANG_PRESETS = [
  { value: "en", label: "英语 (en)" },
  { value: "zh", label: "中文 (zh)" },
  { value: "ja", label: "日语 (ja)" },
  { value: "ko", label: "韩语 (ko)" },
];
