import { invoke } from "@tauri-apps/api/core";

import type { SubtitleChoice, Transcript } from "./types";

export function listSubtitleChoices(path: string): Promise<SubtitleChoice[]> {
  return invoke("subtitle_list_choices", { path });
}

export function loadSubtitleChoice(
  path: string,
  choiceId: string,
): Promise<Transcript> {
  return invoke("subtitle_load_choice", { path, choiceId });
}
