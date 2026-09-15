import { Channel, invoke } from "@tauri-apps/api/core";

import type { AgentProfilesHint } from "@/features/acp/types";

import type { Cue, SubtitleChoice, Transcript } from "@lumina/contracts";

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

export type SubtitleProviderStatus = {
  id: string;
  needsKey: boolean;
  hasKey: boolean;
};

export type SubtitleQuery = {
  title?: string | null;
  tmdbId?: number | null;
  season?: number | null;
  episode?: number | null;
};

export type SubtitleCandidate = {
  provider: string;
  language: string;
  releaseName: string;
  sizeBytes: number;
  format: string;
  season?: number | null;
  episode?: number | null;
  downloadUrl: string;
  cached: boolean;
};

export function getSubtitleProviderStatus(): Promise<SubtitleProviderStatus[]> {
  return invoke("subtitle_provider_status");
}

export function setSubtitleProviderKey(
  provider: string,
  key: string,
): Promise<SubtitleProviderStatus[]> {
  return invoke("subtitle_set_provider_key", { provider, key });
}

export type SubtitleKeyValidation = {
  verified: boolean;
  message: string;
};

export function validateSubtitleProviderKey(
  provider: string,
  key: string,
): Promise<SubtitleKeyValidation> {
  return invoke("subtitle_validate_provider_key", { provider, key });
}

export type ParsedMediaName = {
  key: string;
  season?: number | null;
  episode?: number | null;
  year?: number | null;
};

export function parseMediaFilename(fileName: string): Promise<ParsedMediaName> {
  return invoke("library_parse_filename", { fileName });
}

export function searchOnlineSubtitles(
  path: string,
  query: SubtitleQuery,
  prefer: string[],
): Promise<SubtitleCandidate[]> {
  return invoke("subtitle_search_online", { path, query, prefer });
}

export function downloadSubtitleCandidate(
  path: string,
  candidate: SubtitleCandidate,
): Promise<Transcript> {
  return invoke("subtitle_download_candidate", { path, candidate });
}

export type SubtitleTranslateEvent =
  | {
      type: "Progress";
      payload: { message: string; done: number | null; total: number | null };
    }
  | { type: "Finished"; payload: { transcript: Transcript } }
  | { type: "Failed"; payload: { code: string; message: string } };

export type WorkshopJobPhase = "Running" | "Finished" | "Failed";

export type WorkshopJobSnapshot = {
  jobId: string;
  mediaPath: string;
  choiceId: string;
  targetLang: string;
  phase: WorkshopJobPhase;
  done: number | null;
  total: number | null;
  message: string;
  updatedAtMs: number;
};

export const WORKSHOP_PROGRESS_EVENT = "subtitle-workshop-progress";

export function getWorkshopStatus(
  mediaPath: string,
): Promise<WorkshopJobSnapshot | null> {
  return invoke("subtitle_workshop_status", { mediaPath });
}

export async function proofreadSubtitleTrack(
  options: {
    path: string;
    choiceId: string;
    stripSoundTags?: boolean;
    profileId: string;
    profiles: AgentProfilesHint;
    modelId?: string | null;
    reasoningEffort?: string | null;
    onEvent?: (event: SubtitleTranslateEvent) => void;
  },
): Promise<Transcript> {
  const channel = new Channel<SubtitleTranslateEvent>();
  if (options.onEvent) channel.onmessage = options.onEvent;
  return invoke("subtitle_proofread_track", {
    path: options.path,
    choiceId: options.choiceId,
    stripSoundTags: options.stripSoundTags ?? true,
    profileId: options.profileId,
    profiles: options.profiles,
    modelId: options.modelId ?? null,
    reasoningEffort: options.reasoningEffort ?? null,
    onEvent: channel,
  });
}

export async function translateSubtitleTrack(
  options: {
    path: string;
    choiceId: string;
    targetLang: string;
    profileId: string;
    profiles: AgentProfilesHint;
    modelId?: string | null;
    reasoningEffort?: string | null;
    glossaryBackfill?: boolean;
    glossaryReviewMode?: boolean;
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
    glossaryBackfill: options.glossaryBackfill ?? true,
    glossaryReviewMode: options.glossaryReviewMode ?? false,
    onEvent: channel,
  });
}
