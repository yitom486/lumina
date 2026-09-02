import { Channel, invoke } from "@tauri-apps/api/core";

import type { AgentProfilesHint } from "@/features/acp/types";

import type { Cue, SubtitleChoice, Transcript } from "./types";

export function listSubtitleChoices(path: string): Promise<SubtitleChoice[]> {
  return invoke("subtitle_list_choices", { path });
}

export function loadSubtitleChoice(
  path: string,
  choiceId: string,
): Promise<Transcript> {
  return invoke("subtitle_load_choice", { path, choiceId });
}

export function exportSubtitleSidecar(
  path: string,
  langToken: string,
  cues: Cue[],
): Promise<Transcript> {
  return invoke("subtitle_export_sidecar", { path, langToken, cues });
}

export type SubtitleTranslateEvent =
  | { type: "Progress"; payload: { message: string } }
  | { type: "Finished"; payload: { transcript: Transcript } }
  | { type: "Failed"; payload: { code: string; message: string } };

export async function translateSubtitleTrack(
  options: {
    path: string;
    choiceId: string;
    targetLang: string;
    profileId: string;
    profiles: AgentProfilesHint;
    modelId?: string | null;
    reasoningEffort?: string | null;
    onEvent?: (event: SubtitleTranslateEvent) => void;
  },
): Promise<Transcript> {
  const channel = new Channel<SubtitleTranslateEvent>();
  if (options.onEvent) {
    channel.onmessage = options.onEvent;
  }
  return invoke("subtitle_translate_track", {
    path: options.path,
    choiceId: options.choiceId,
    targetLang: options.targetLang,
    profileId: options.profileId,
    profiles: options.profiles,
    modelId: options.modelId ?? null,
    reasoningEffort: options.reasoningEffort ?? null,
    onEvent: channel,
  });
}
